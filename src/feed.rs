use std::fs::File;
use std::io::{self, IsTerminal, Read};
use std::path::Path;

use anyhow::{Context, bail};
use html2text::render::RichAnnotation;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span, Text};

use crate::{AnyResult, feed_tui};

pub(crate) struct Feed {
    pub(crate) title: String,
    pub(crate) entries: Vec<Entry>,
}

pub(crate) struct Entry {
    pub(crate) title: String,
    pub(crate) date: String,
    pub(crate) link: String,
    pub(crate) body: String,
    pub(crate) format: BodyFormat,
    pub(crate) summary: bool,
}

#[derive(Debug, Default, PartialEq, Eq)]
pub(crate) enum BodyFormat {
    #[default]
    PlainText,
    Html,
    Xhtml,
    Markdown,
    Other(String),
}

impl BodyFormat {
    fn from_mime(mime: &str) -> Self {
        match mime
            .split(';')
            .next()
            .unwrap_or_default()
            .trim()
            .to_ascii_lowercase()
            .as_str()
        {
            "text/html" => Self::Html,
            "application/xhtml+xml" => Self::Xhtml,
            "text/markdown" | "text/x-markdown" => Self::Markdown,
            "text/plain" | "" => Self::PlainText,
            _ => Self::Other(mime.to_owned()),
        }
    }

    pub(crate) fn label(&self) -> &str {
        match self {
            Self::PlainText => "Plain text",
            Self::Html => "HTML",
            Self::Xhtml => "XHTML",
            Self::Markdown => "Markdown",
            Self::Other(mime) => mime,
        }
    }
}

#[derive(Clone)]
struct ReaderStyle;

impl tui_markdown::StyleSheet for ReaderStyle {
    fn heading_marker(&self, _level: u8) -> &str {
        ""
    }

    fn code_block_fence(&self) -> &str {
        ""
    }
}

/// 读取 RSS/Atom 内容并打开交互式阅读器。
pub fn browse(path: &Path) -> AnyResult {
    let feed = if path.as_os_str() == "-" {
        parse(io::stdin().lock())?
    } else {
        parse(File::open(path).with_context(|| format!("failed to open {}", path.display()))?)?
    };
    if !io::stderr().is_terminal() {
        bail!("feed reader requires an interactive terminal");
    }
    feed_tui::run(feed)
}

pub(crate) fn parse(input: impl Read) -> AnyResult<Feed> {
    let parsed = feed_rs::parser::parse(input).context("failed to parse RSS or Atom feed")?;
    let title = parsed
        .title
        .map(|text| text.content)
        .unwrap_or_else(|| "Untitled feed".into());
    let entries = parsed
        .entries
        .into_iter()
        .map(|entry| {
            let (body, format, summary) = entry
                .content
                .and_then(|content| {
                    content
                        .body
                        .filter(|body| !body.trim().is_empty())
                        .map(|body| {
                            let mime = content.content_type.to_string();
                            (body, BodyFormat::from_mime(&mime), false)
                        })
                })
                .or_else(|| {
                    entry.summary.map(|text| {
                        let mime = text.content_type.to_string();
                        (text.content, BodyFormat::from_mime(&mime), true)
                    })
                })
                .unwrap_or_default();
            Entry {
                title: entry
                    .title
                    .map(|text| text.content)
                    .unwrap_or_else(|| "Untitled entry".into()),
                date: entry
                    .published
                    .or(entry.updated)
                    .map(|date| date.to_rfc3339())
                    .unwrap_or_default(),
                link: entry
                    .links
                    .into_iter()
                    .find(|link| link.rel.as_deref().is_none_or(|rel| rel == "alternate"))
                    .map(|link| link.href)
                    .unwrap_or_default(),
                body,
                format,
                summary,
            }
        })
        .collect();
    Ok(Feed { title, entries })
}

pub(crate) fn render_body(entry: &Entry, width: u16, raw: bool) -> AnyResult<Text<'static>> {
    if entry.body.is_empty() {
        return Ok(Text::raw("No content available."));
    }
    if raw {
        return Ok(Text::raw(entry.body.clone()));
    }
    match entry.format {
        BodyFormat::Html | BodyFormat::Xhtml => {
            let lines = html2text::config::rich()
                .link_footnotes(true)
                .unicode_strikeout(false)
                .lines_from_read(entry.body.as_bytes(), usize::from(width.max(10)))
                .context("failed to render feed HTML")?;
            Ok(Text::from(
                lines
                    .iter()
                    .map(|line| {
                        Line::from(
                            line.tagged_strings()
                                .map(|part| {
                                    let mut style = Style::default();
                                    for annotation in &part.tag {
                                        style = match annotation {
                                            RichAnnotation::Strong => {
                                                style.add_modifier(Modifier::BOLD)
                                            }
                                            RichAnnotation::Emphasis => {
                                                style.add_modifier(Modifier::ITALIC)
                                            }
                                            RichAnnotation::Strikeout => {
                                                style.add_modifier(Modifier::CROSSED_OUT)
                                            }
                                            RichAnnotation::Code | RichAnnotation::Preformat(_) => {
                                                style.fg(Color::Yellow)
                                            }
                                            RichAnnotation::Link(_) => style
                                                .fg(Color::Cyan)
                                                .add_modifier(Modifier::UNDERLINED),
                                            RichAnnotation::Image(_) => {
                                                style.add_modifier(Modifier::DIM)
                                            }
                                            // 使用终端主题，避免订阅自带颜色降低可读性。
                                            _ => style,
                                        };
                                    }
                                    Span::styled(part.s.clone(), style)
                                })
                                .collect::<Vec<_>>(),
                        )
                    })
                    .collect::<Vec<_>>(),
            ))
        }
        BodyFormat::Markdown => {
            let options = tui_markdown::Options::new(ReaderStyle).table_width(width.max(1));
            let rendered = tui_markdown::from_str_with_options(&entry.body, &options);
            // 阅读器缓存需独立于输入借用；保留库生成的行、span 和样式。
            Ok(Text::from(
                rendered
                    .lines
                    .into_iter()
                    .map(|line| Line {
                        spans: line
                            .spans
                            .into_iter()
                            .map(|span| Span::styled(span.content.into_owned(), span.style))
                            .collect(),
                        style: line.style,
                        alignment: line.alignment,
                    })
                    .collect::<Vec<_>>(),
            )
            .style(rendered.style))
        }
        _ => Ok(Text::raw(entry.body.clone())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn body_text(entry: &Entry, width: u16) -> AnyResult<String> {
        Ok(render_body(entry, width, false)?.to_string())
    }

    #[test]
    fn reads_atom_html_and_plain_summary_without_changing_order() {
        let feed = parse(include_bytes!("../tests/fixtures/sample.atom").as_slice()).unwrap();
        assert_eq!(feed.title, "示例订阅");
        assert_eq!(feed.entries.len(), 2);
        let first = &feed.entries[0];
        assert_eq!(first.title, "第一篇文章");
        assert_eq!(first.link, "https://example.invalid/first");
        assert!(first.date.starts_with("2024-01-02"));
        let body = body_text(first, 80).unwrap();
        assert!(body.contains("阅读示例"));
        assert!(body.contains("列表内容"));
        assert!(body.contains("let answer = 42;"));
        assert!(body.contains('&'));
        assert!(!body.contains("<p>"));
        assert!(!body.contains("备用摘要"));
        assert_eq!(
            body_text(&feed.entries[1], 80).unwrap(),
            "Plain text: a < b & c."
        );
    }

    #[test]
    fn reads_rss_full_content_before_description_and_handles_missing_fields() {
        let feed = parse(include_bytes!("../tests/fixtures/sample.rss").as_slice()).unwrap();
        assert_eq!(feed.entries.len(), 3);
        let body = body_text(&feed.entries[0], 80).unwrap();
        assert!(body.contains("Full article body"));
        assert!(!body.contains("Short summary"));
        assert!(body.contains("https://example.invalid/more"));
        assert!(
            body_text(&feed.entries[1], 80)
                .unwrap()
                .contains("Summary body")
        );
        assert_eq!(feed.entries[2].title, "Untitled entry");
        assert_eq!(
            body_text(&feed.entries[2], 80).unwrap(),
            "No content available."
        );
    }

    #[test]
    fn accepts_empty_feeds_and_rejects_non_feed_input() {
        let feed = parse(
            b"<feed xmlns=\"http://www.w3.org/2005/Atom\"><title>Empty</title></feed>".as_slice(),
        )
        .unwrap();
        assert!(feed.entries.is_empty());
        assert!(parse(b"not a feed".as_slice()).is_err());
        assert!(parse(b"<html><body>not a feed</body></html>".as_slice()).is_err());
    }

    #[test]
    fn reads_atom_xhtml_content() {
        let feed = parse(br#"<feed xmlns="http://www.w3.org/2005/Atom"><title>Example</title><entry><title>XHTML</title><content type="xhtml"><div xmlns="http://www.w3.org/1999/xhtml"><p>Formatted <b>body</b></p></div></content></entry></feed>"#.as_slice()).unwrap();
        let body = body_text(&feed.entries[0], 40).unwrap();
        assert!(body.contains("Formatted"));
        assert!(body.contains("body"));
        assert!(!body.contains("<div"));
    }

    fn formats() -> Feed {
        parse(include_bytes!("../tests/fixtures/formats.atom").as_slice()).unwrap()
    }

    fn styled_fragment(text: &Text<'_>, fragment: &str, modifier: Modifier) -> bool {
        text.lines.iter().any(|line| {
            line.spans.iter().any(|span| {
                span.content.contains(fragment)
                    && text
                        .style
                        .patch(line.style)
                        .patch(span.style)
                        .add_modifier
                        .contains(modifier)
            })
        })
    }

    #[test]
    fn detects_declared_formats_and_summary_source_without_guessing() {
        let feed = formats();
        assert_eq!(feed.entries[0].format, BodyFormat::Markdown);
        assert_eq!(feed.entries[1].format, BodyFormat::Html);
        assert!(!feed.entries[1].summary);
        assert_eq!(feed.entries[2].format, BodyFormat::PlainText);
        assert!(feed.entries[2].summary);
        let plain = render_body(&feed.entries[2], 80, false).unwrap();
        assert!(plain.to_string().contains("**Keep this literal**"));
        assert!(!styled_fragment(&plain, "literal", Modifier::BOLD));
        assert_eq!(
            BodyFormat::from_mime("text/markdown; charset=utf-8"),
            BodyFormat::Markdown
        );
        assert_eq!(
            BodyFormat::from_mime("application/xhtml+xml"),
            BodyFormat::Xhtml
        );
        assert_eq!(
            BodyFormat::from_mime("text/x-example"),
            BodyFormat::Other("text/x-example".into())
        );
    }

    #[test]
    fn html_rich_output_preserves_styles_and_link_targets() {
        let feed = formats();
        let rich = render_body(&feed.entries[1], 80, false).unwrap();
        assert!(styled_fragment(&rich, "bold phrase", Modifier::BOLD));
        assert!(styled_fragment(&rich, "emphasis", Modifier::ITALIC));
        assert!(styled_fragment(&rich, "removed", Modifier::CROSSED_OUT));
        assert!(styled_fragment(&rich, "More", Modifier::UNDERLINED));
        assert!(rich.to_string().contains("https://example.invalid/more"));
        assert!(!rich.to_string().contains("<strong>"));
        let raw = render_body(&feed.entries[1], 80, true).unwrap();
        assert_eq!(raw.to_string(), feed.entries[1].body);
        assert!(!styled_fragment(&raw, "bold phrase", Modifier::BOLD));
    }

    #[test]
    fn markdown_renders_tables_emphasis_and_highlighted_code() {
        let feed = formats();
        let rich = render_body(&feed.entries[0], 40, false).unwrap();
        assert!(styled_fragment(&rich, "bold phrase", Modifier::BOLD));
        assert!(styled_fragment(&rich, "emphasis", Modifier::ITALIC));
        assert!(rich.to_string().contains("Example heading"));
        assert!(rich.to_string().contains('─'));
        assert!(
            rich.lines
                .iter()
                .flat_map(|line| &line.spans)
                .any(|span| { span.content.contains("answer") && span.style.fg.is_some() })
        );
        assert!(!rich.to_string().contains("```rust"));
        let raw = render_body(&feed.entries[0], 40, true).unwrap();
        assert_eq!(raw.to_string(), feed.entries[0].body.trim_end_matches('\n'));
        for width in [1, 20, 80] {
            let resized = render_body(&feed.entries[0], width, false).unwrap();
            assert!(resized.to_string().contains("Example heading"));
        }
    }
}
