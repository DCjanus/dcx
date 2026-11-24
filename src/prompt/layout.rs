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

    pub(super) fn nerd(&self) -> bool {
        self.nerd
    }

    pub(super) fn python(&mut self, sample: &str, environment: &str) {
        let version = sample.strip_prefix("py:").unwrap_or_default();
        let environment = environment.strip_prefix("env:").unwrap_or_default();
        if sample.is_empty() && environment.is_empty() {
            return;
        }
        let mut label = String::from("🐍");
        if !version.is_empty() {
            label.push_str(&format!(" v{version}"));
        }
        if !environment.is_empty() {
            label.push_str(&format!(" ({environment})"));
        }
        self.item_with_prefix("via ", &label, "1;33", false);
    }

    pub(super) fn identity(&mut self, identity: &str) {
        let style = if identity == "root" { "1;31" } else { "1;33" };
        self.item(identity, style, false);
        self.remote_connector();
    }

    pub(super) fn hostname(&mut self, hostname: &str) {
        if !hostname.is_empty() {
            let label = if self.nerd {
                format!("🌐 {hostname}")
            } else {
                hostname.to_owned()
            };
            self.item(&label, "1;2;32", false);
            self.remote_connector();
        }
    }

    fn remote_connector(&mut self) {
        if self.nerd {
            self.item("in", "", false);
        }
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
            let flags = flags.split_whitespace().collect::<Vec<_>>();
            let branch = if self.nerd {
                match flags.iter().find_map(|flag| flag.strip_prefix("upstream:")) {
                    Some(upstream) => format!(" {branch}:{upstream}"),
                    None => format!(" {branch}"),
                }
            } else {
                branch.to_owned()
            };
            self.item_with_prefix("on ", &branch, "1;35", false);
            let mut markers = String::new();
            if self.nerd {
                for (status, marker) in [
                    ("conflict", '='),
                    ("stash", '$'),
                    ("deleted", '✘'),
                    ("renamed", '»'),
                    ("modified", '!'),
                    ("staged", '+'),
                    ("untracked", '?'),
                ] {
                    if flags.iter().any(|flag| git_status_name(flag) == status) {
                        markers.push(marker);
                    }
                }
                let ahead = flags.iter().any(|flag| {
                    flag.strip_prefix('+')
                        .and_then(|count| count.parse::<u32>().ok())
                        .is_some_and(|count| count > 0)
                });
                let behind = flags.iter().any(|flag| {
                    flag.strip_prefix('-')
                        .and_then(|count| count.parse::<u32>().ok())
                        .is_some_and(|count| count > 0)
                });
                match (ahead, behind) {
                    (true, true) => markers.push('⇕'),
                    (true, false) => markers.push('⇡'),
                    (false, true) => markers.push('⇣'),
                    _ => {}
                }
            }
            for flag in flags {
                if flag.starts_with("upstream:") {
                    continue;
                }
                if let Some(hash) = flag.strip_prefix("commit:") {
                    self.item(&format!("({hash})"), "1;32", false);
                    continue;
                }
                if self.nerd
                    && (flag.starts_with(['+', '-'])
                        || matches!(
                            git_status_name(flag),
                            "conflict"
                                | "stash"
                                | "deleted"
                                | "renamed"
                                | "modified"
                                | "staged"
                                | "untracked"
                        ))
                {
                    continue;
                }
                let operation = match git_status_name(flag) {
                    "rebase" => Some("REBASING"),
                    "merge" => Some("MERGING"),
                    "cherry-pick" => Some("CHERRY-PICKING"),
                    "revert" => Some("REVERTING"),
                    "bisect" => Some("BISECTING"),
                    "am" => Some("AM"),
                    _ => None,
                };
                if self.nerd
                    && let Some(operation) = operation
                {
                    let text = flag.split_once(':').map_or_else(
                        || operation.to_owned(),
                        |(_, step)| format!("{operation} {step}"),
                    );
                    self.item_with_affixes("(", &text, ")", "1;33", false);
                } else {
                    self.item(
                        flag,
                        if operation.is_some() { "1;33" } else { "1;31" },
                        false,
                    );
                }
            }
            if !markers.is_empty() {
                self.item(&format!("[{markers}]"), "1;31", false);
            }
        } else {
            let (label, value) = value.split_once(':').unwrap_or(("", value));
            let (plain, icon, color) = match label {
                "pkg" => ("pkg", "📦", "1;38;5;208"),
                "go" => ("go", "🐹", "1;36"),
                "rust" => ("rust", "🦀", "1;31"),
                "node" => ("node", "", "1;32"),
                "py" => ("py", "🐍", "1;33"),
                "env" => ("env", "󰈙", "1;33"),
                "conda" => ("conda", "🅒", "1;32"),
                "java" => ("java", "☕", "2;31"),
                "ruby" => ("ruby", "💎", "1;31"),
                _ => return,
            };
            let item = if self.nerd && value.is_empty() {
                icon.to_owned()
            } else if self.nerd {
                let version_prefix = if matches!(label, "env" | "conda") {
                    ""
                } else {
                    "v"
                };
                format!(
                    "{icon} {version_prefix}{}",
                    value
                        .strip_prefix('v')
                        .filter(|_| label == "pkg")
                        .unwrap_or(value)
                )
            } else {
                format!("{plain}:{value}")
            };
            let prefix = if self.nerd {
                match label {
                    "pkg" => "is ",
                    "env" => "",
                    _ => "via ",
                }
            } else {
                ""
            };
            self.item_with_prefix(prefix, &item, color, false);
        }
    }

    pub(super) fn jobs(&mut self, jobs: usize) {
        match jobs {
            0 => {}
            1 => self.item("1 job", "1;34", false),
            _ => self.item(&format!("{jobs} jobs"), "1;34", false),
        }
    }

    pub(super) fn duration(&mut self, duration: u64) {
        if duration >= 2000 {
            self.item_with_prefix(
                "took ",
                &if self.nerd {
                    duration_text(duration)
                } else {
                    format!("{:.1}s", duration as f64 / 1000.0)
                },
                "1;33",
                false,
            );
        }
    }

    pub(super) fn finish(mut self, status: u32, mode: Option<&str>, jobs: usize) -> String {
        if self.nerd {
            self.output.insert(0, '\n');
        }
        self.output.push('\n');
        if self.nerd && jobs > 0 {
            let label = if jobs == 1 {
                "✦".into()
            } else {
                format!("✦{jobs}")
            };
            self.styled(&label, "1;34");
            self.output.push(' ');
        }
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
        let (text, style) = if self.nerd {
            let style = if status != 0
                && !matches!(mode, Some("default" | "visual" | "replace" | "replace_one"))
            {
                "1;31"
            } else {
                color
            };
            (character.to_owned(), style)
        } else if status == 0 {
            (character.to_owned(), color)
        } else {
            (format!("[{status}] {character}"), "1;31")
        };
        self.styled(&text, style);
        self.output.push(' ');
        self.output
    }

    fn item(&mut self, text: &str, style: &str, keep_end: bool) {
        self.item_with_prefix("", text, style, keep_end);
    }

    fn item_with_prefix(&mut self, prefix: &str, text: &str, style: &str, keep_end: bool) {
        self.item_with_affixes(prefix, text, "", style, keep_end);
    }

    fn item_with_affixes(
        &mut self,
        prefix: &str,
        text: &str,
        suffix: &str,
        style: &str,
        keep_end: bool,
    ) {
        let text = format!("{prefix}{}{suffix}", clean(text));
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
        if let Some(text) = text
            .strip_prefix(prefix)
            .and_then(|text| text.strip_suffix(suffix))
        {
            self.output.push_str(prefix);
            self.styled(text, style);
            self.output.push_str(suffix);
        } else {
            self.styled(&text, style);
        }
        self.line_width += width;
    }

    fn styled(&mut self, text: &str, style: &str) {
        if self.color && !style.is_empty() {
            self.output.push_str("\x1b[");
            self.output.push_str(style);
            self.output.push('m');
        }
        self.output.push_str(text);
        if self.color && !style.is_empty() {
            self.output.push_str("\x1b[0m");
        }
    }
}

fn duration_text(duration: u64) -> String {
    let mut remaining = duration / 1000;
    let mut text = String::new();
    for (unit, divisor) in [('d', 86_400), ('h', 3_600), ('m', 60), ('s', 1)] {
        let count = remaining / divisor;
        if count > 0 || !text.is_empty() || divisor == 1 {
            text.push_str(&format!("{count}{unit}"));
        }
        remaining %= divisor;
    }
    text
}

fn git_status_name(flag: &str) -> &str {
    flag.split_once(':').map_or(flag, |(name, _)| name)
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
    fn renders_nerd_git_markers_in_bold_red() {
        let mut layout = Layout::new(80);
        layout.nerd = true;
        layout.color = true;
        layout.module("git", "git:main modified untracked");
        assert!(layout.finish(0, None, 0).contains("\x1b[1;31m[!?]\x1b[0m"));
    }

    #[test]
    fn groups_stash_with_other_nerd_git_markers() {
        let mut layout = Layout::new(80);
        layout.nerd = true;
        layout.color = false;
        layout.module("git", "git:main staged modified untracked conflict stash:2");
        assert_eq!(layout.finish(0, None, 0), "\non  main [=$!+?]\n❯ ");
    }

    #[test]
    fn truncates_wide_text_at_character_boundaries() {
        assert_eq!(fit("example/中文路径", 9, true), "...文路径");
        assert_eq!(fit("分支分支分支", 8, false), "分支...");
    }
}
