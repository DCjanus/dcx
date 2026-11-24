use std::env;
use std::path::{Component, Path, PathBuf};

use ratatui::text::Span;

use super::clean;

pub(super) struct Layout {
    width: usize,
    line_width: usize,
    output: String,
    color: bool,
    nerd: bool,
}

impl Layout {
    pub(super) fn new(width: usize) -> Self {
        Self {
            width: width.saturating_sub(1).max(8),
            line_width: 0,
            output: String::new(),
            color: env::var_os("NO_COLOR").is_none()
                && !env::var_os("TERM").is_some_and(|term| term == "dumb"),
            nerd: env::var_os("DCX_PROMPT_STYLE").is_some_and(|style| style == "nerd"),
        }
    }

    pub(super) fn identity(&mut self, identity: &str) {
        self.item(identity, "1;33", false);
    }

    pub(super) fn directory(&mut self, cwd: &Path, git_depth: Option<usize>) {
        let directory = display_directory(cwd, git_depth);
        if display_width(&directory) > self.width {
            let prefix = if directory.starts_with("~/") {
                "~/.../"
            } else {
                ".../"
            };
            for (index, _) in directory.match_indices('/') {
                let tail = &directory[index + 1..];
                if !tail.is_empty() && display_width(tail) + prefix.len() <= self.width {
                    self.item(&format!("{prefix}{tail}"), "1;36", true);
                    return;
                }
            }
        }
        self.item(&directory, "1;36", true);
    }

    pub(super) fn module(&mut self, name: &str, value: &str) {
        if name == "git" {
            let value = value.split_once('\n').map_or(value, |(_, summary)| summary);
            let git = value.strip_prefix("git:").unwrap_or(value);
            let (branch, flags) = git.split_once(' ').unwrap_or((git, ""));
            let prefix = if self.nerd { "" } else { "on" };
            self.item(&format!("{prefix} {branch}"), "1;35", false);
            let mut markers = String::new();
            for flag in flags.split_whitespace() {
                let marker = match flag {
                    "staged" => "+",
                    "modified" => "!",
                    "untracked" => "?",
                    "conflict" => "=",
                    _ => "",
                };
                if self.nerd && !marker.is_empty() {
                    markers.push_str(marker);
                } else {
                    self.item(flag, "33", false);
                }
            }
            if !markers.is_empty() {
                self.item(&format!("[{markers}]"), "1;33", false);
            }
        } else {
            let (label, value) = value.split_once(':').unwrap_or(("", value));
            let (plain, icon, color) = match label {
                "pkg" => ("pkg", "📦", "33"),
                "go" => ("go", "🐹", "36"),
                "rust" => ("rust", "🦀", "31"),
                "node" => ("node", "", "32"),
                "py" => ("py", "🐍", "33"),
                "env" => ("env", "󰈙", "33"),
                "java" => ("java", "☕", "31"),
                "ruby" => ("ruby", "💎", "31"),
                _ => return,
            };
            let item = if self.nerd {
                let version_prefix = if label == "env" { "" } else { "v" };
                format!("{icon} {version_prefix}{value}")
            } else {
                format!("{plain}:{value}")
            };
            let color = if self.nerd && label == "pkg" {
                "1;38;5;208"
            } else {
                color
            };
            self.item(&item, color, false);
        }
    }

    pub(super) fn jobs(&mut self, jobs: usize) {
        match jobs {
            0 => {}
            1 => self.item("1 job", "34", false),
            _ => self.item(&format!("{jobs} jobs"), "34", false),
        }
    }

    pub(super) fn duration(&mut self, duration: u64) {
        if duration >= 2000 {
            self.item(
                &format!("took {:.1}s", duration as f64 / 1000.0),
                "33",
                false,
            );
        }
    }

    pub(super) fn finish(mut self, status: u32, mode: Option<&str>) -> String {
        self.output.push('\n');
        let (character, color) = match mode {
            Some("visual") if self.nerd => ("❮", "1;33"),
            Some("replace" | "replace_one") if self.nerd => ("❮", "1;35"),
            Some("default") => {
                if self.nerd {
                    ("❮", "1;32")
                } else {
                    ("<", "1;32")
                }
            }
            Some("visual") => ("v", "1;33"),
            Some("replace" | "replace_one") => ("r", "1;35"),
            _ => {
                if self.nerd {
                    ("❯", "1;32")
                } else {
                    (">", "1;32")
                }
            }
        };
        let (text, style) = if status == 0 {
            (character.to_owned(), color)
        } else {
            (format!("[{status}] {character}"), "1;31")
        };
        self.styled(&text, style);
        self.output.push(' ');
        self.output
    }

    fn item(&mut self, text: &str, style: &str, keep_end: bool) {
        let text = clean(text);
        let text = fit(&text, self.width, keep_end);
        let width = display_width(&text);
        if width == 0 {
            return;
        }
        if self.line_width > 0 {
            if self.line_width + 1 + width > self.width {
                self.output.push('\n');
                self.line_width = 0;
            } else {
                self.output.push(' ');
                self.line_width += 1;
            }
        }
        self.styled(&text, style);
        self.line_width += width;
    }

    fn styled(&mut self, text: &str, style: &str) {
        if self.color {
            self.output.push_str("\x1b[");
            self.output.push_str(style);
            self.output.push('m');
        }
        self.output.push_str(text);
        if self.color {
            self.output.push_str("\x1b[0m");
        }
    }
}

fn display_directory(cwd: &Path, git_depth: Option<usize>) -> String {
    let logical = env::var_os("PWD")
        .map(PathBuf::from)
        .filter(|path| path.is_absolute());
    let directory = logical.as_deref().unwrap_or(cwd);
    if let Some(depth) = git_depth {
        let tail = directory
            .components()
            .rev()
            .take(depth.saturating_add(1).min(3))
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect::<PathBuf>();
        return tail.to_string_lossy().into_owned();
    }
    let home = env::var_os("HOME").and_then(|home| directory.strip_prefix(home).ok());
    let path = home.unwrap_or(directory);
    let components = path
        .components()
        .filter_map(|component| match component {
            Component::Normal(name) => Some(name),
            _ => None,
        })
        .collect::<Vec<_>>();
    if components.len() + usize::from(home.is_some()) <= 3 {
        if let Some(relative) = home {
            return if relative.as_os_str().is_empty() {
                "~".into()
            } else {
                Path::new("~").join(relative).to_string_lossy().into_owned()
            };
        }
        return directory.to_string_lossy().into_owned();
    }

    components[components.len().saturating_sub(3)..]
        .iter()
        .collect::<PathBuf>()
        .to_string_lossy()
        .into_owned()
}

fn display_width(text: &str) -> usize {
    Span::raw(text).width()
}

fn fit(text: &str, width: usize, keep_end: bool) -> String {
    if display_width(text) <= width {
        return text.to_owned();
    }
    let budget = width - 3;
    if keep_end {
        let mut start = text.len();
        for (index, _) in text.char_indices().rev() {
            if display_width(&text[index..]) > budget {
                break;
            }
            start = index;
        }
        format!("...{}", &text[start..])
    } else {
        let mut end = 0;
        for (index, character) in text.char_indices() {
            let next = index + character.len_utf8();
            if display_width(&text[..next]) > budget {
                break;
            }
            end = next;
        }
        format!("{}...", &text[..end])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn truncates_wide_text_at_character_boundaries() {
        assert_eq!(fit("example/中文路径", 9, true), "...文路径");
        assert_eq!(fit("分支分支分支", 8, false), "分支...");
    }
}
