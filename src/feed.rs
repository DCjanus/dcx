use std::fs::File;
use std::io::{self, IsTerminal, Read};
use std::path::Path;

use anyhow::{Context, bail};

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
    pub(crate) html: bool,
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
            let (body, html) = entry
                .content
                .and_then(|content| {
                    content
                        .body
                        .filter(|body| !body.trim().is_empty())
                        .map(|body| {
                            let mime = content.content_type.to_string();
                            (body, mime.contains("html"))
                        })
                })
                .or_else(|| {
                    entry.summary.map(|text| {
                        let mime = text.content_type.to_string();
                        (text.content, mime.contains("html"))
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
                html,
            }
        })
        .collect();
    Ok(Feed { title, entries })
}

pub(crate) fn body_text(entry: &Entry, width: u16) -> AnyResult<String> {
    if entry.body.is_empty() {
        return Ok("No content available.".into());
    }
    if entry.html {
        html2text::from_read(entry.body.as_bytes(), usize::from(width.max(10)))
            .context("failed to render feed HTML")
    } else {
        Ok(entry.body.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
