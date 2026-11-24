use std::env;
use std::fs::File;
use std::io::{BufRead, BufReader, Read};
use std::path::Path;
use std::process::Command;

use super::{MAX_SAMPLE, capture, clean, fingerprint};

#[derive(Clone, Copy)]
pub(super) struct Probe {
    pub name: &'static str,
    collect: fn(&Path) -> Option<String>,
}

pub(super) const ALL: [Probe; 9] = [
    Probe {
        name: "git",
        collect: git,
    },
    Probe {
        name: "package",
        collect: package,
    },
    Probe {
        name: "go",
        collect: go,
    },
    Probe {
        name: "rust",
        collect: rust,
    },
    Probe {
        name: "node",
        collect: node,
    },
    Probe {
        name: "python",
        collect: python,
    },
    Probe {
        name: "environment",
        collect: environment,
    },
    Probe {
        name: "java",
        collect: java,
    },
    Probe {
        name: "ruby",
        collect: ruby,
    },
];

pub(super) fn collect(probe: &Probe, cwd: &Path) -> Option<String> {
    (probe.collect)(cwd)
}

pub(super) fn selection_fingerprint(cwd: &Path) -> u64 {
    // 同一个 shim 文件可以在项目配置修改后指向另一个版本。
    let mut selectors = Vec::new();
    for ancestor in cwd.ancestors() {
        for name in [
            "mise.toml",
            ".mise.toml",
            "mise.local.toml",
            ".mise.local.toml",
            ".tool-versions",
            ".python-version",
            ".node-version",
            ".nvmrc",
            ".go-version",
            "go.mod",
            "go.work",
            "rust-toolchain",
            "rust-toolchain.toml",
            ".java-version",
            ".ruby-version",
            "package.json",
            "Cargo.toml",
            "pyproject.toml",
        ] {
            let path = ancestor.join(name);
            if let Ok(metadata) = path.metadata() {
                selectors.push((path, metadata.len(), metadata.modified().ok()));
            }
        }
    }
    fingerprint(&selectors)
}

fn project_file<'a>(cwd: &'a Path, names: &[&str]) -> Option<&'a Path> {
    for directory in cwd.ancestors() {
        if names.iter().any(|name| directory.join(name).is_file()) {
            return Some(directory);
        }
        if directory.join(".git").exists() {
            break;
        }
    }
    None
}

fn git(cwd: &Path) -> Option<String> {
    let (status, output) = capture(Command::new("git").env("LC_ALL", "C").args([
        "--no-optional-locks",
        "-c",
        "core.quotePath=true",
        "status",
        "--porcelain=v2",
        "--branch",
        "--show-stash",
    ]))?;
    if !status.success() {
        // Git 返回 128 时没有可用的仓库状态，例如切出仓库或移除 .git。
        return (status.code() == Some(128)).then(String::new);
    }
    let mut summary = git_summary(BufReader::new(output))?;
    if let Some(operation) = gix::discover_with_environment_overrides(cwd)
        .ok()
        .and_then(|repository| repository.state())
    {
        use gix::state::InProgress::*;
        summary.push(' ');
        summary.push_str(match operation {
            Rebase | RebaseInteractive | ApplyMailboxRebase => "rebase",
            Merge => "merge",
            CherryPick | CherryPickSequence => "cherry-pick",
            Revert | RevertSequence => "revert",
            Bisect => "bisect",
            ApplyMailbox => "am",
        });
    }
    Some(summary)
}

fn git_summary(output: impl BufRead) -> Option<String> {
    let mut branch = None;
    let mut oid = None;
    let mut upstream = None;
    let mut stashes = None;
    let mut staged = false;
    let mut modified = false;
    let mut untracked = false;
    let mut conflicted = false;
    for line in output.lines() {
        let line = line.ok()?;
        if let Some(value) = line.strip_prefix("# branch.head ") {
            branch = Some(value.to_owned());
        } else if let Some(value) = line.strip_prefix("# branch.oid ") {
            oid = Some(value.to_owned());
        } else if let Some(value) = line.strip_prefix("# branch.ab ") {
            upstream = Some(value.to_owned());
        } else if let Some(value) = line.strip_prefix("# stash ") {
            stashes = value.parse::<usize>().ok().filter(|count| *count > 0);
        } else {
            match line.as_bytes() {
                [b'1' | b'2', b' ', index, worktree, ..] => {
                    staged |= *index != b'.';
                    modified |= *worktree != b'.';
                }
                [b'?', b' ', ..] => untracked = true,
                [b'u', b' ', ..] => conflicted = true,
                _ => {}
            }
        }
    }
    let Some(branch) = branch else {
        return Some(String::new());
    };
    let head = if branch == "(detached)" {
        oid.as_deref()
            .and_then(|oid| oid.get(..7))
            .unwrap_or("detached")
    } else {
        &branch
    };
    let mut parts = vec![format!("git:{head}")];
    if let Some(upstream) = upstream.filter(|value| *value != "+0 -0") {
        parts.push(upstream);
    }
    if staged {
        parts.push("staged".into());
    }
    if modified {
        parts.push("modified".into());
    }
    if untracked {
        parts.push("untracked".into());
    }
    if conflicted {
        parts.push("conflict".into());
    }
    if let Some(count) = stashes {
        parts.push(format!("stash:{count}"));
    }
    Some(parts.join(" "))
}

fn package(cwd: &Path) -> Option<String> {
    let Some(directory) = project_file(cwd, &["package.json", "Cargo.toml", "pyproject.toml"])
    else {
        return Some(String::new());
    };
    let version = package_json(&directory.join("package.json"))
        .or_else(|| package_toml(&directory.join("Cargo.toml"), "[package]"))
        .or_else(|| package_toml(&directory.join("pyproject.toml"), "[project]"));
    Some(
        version
            .map(|version| format!("pkg:{version}"))
            .unwrap_or_default(),
    )
}

fn package_json(path: &Path) -> Option<String> {
    if !path.is_file() {
        return None;
    }
    let mut raw = Vec::new();
    File::open(path)
        .ok()?
        .take(64 * 1024)
        .read_to_end(&mut raw)
        .ok()?;
    let json: serde_json::Value = serde_json::from_slice(&raw).ok()?;
    Some(json.get("version")?.as_str()?.to_owned())
}

fn package_toml(path: &Path, section: &str) -> Option<String> {
    if !path.is_file() {
        return None;
    }
    // 仅读取常见的单行版本声明，动态版本由各项目的工具处理。
    let mut in_section = false;
    for line in BufReader::new(File::open(path).ok()?.take(64 * 1024)).lines() {
        let line = line.ok()?;
        let line = line.trim();
        if line.starts_with('[') {
            in_section = line == section;
        } else if in_section
            && let Some((key, value)) = line.split_once('=')
            && key.trim() == "version"
        {
            let value = value.trim();
            let quote = value
                .chars()
                .next()
                .filter(|quote| matches!(quote, '"' | '\''))?;
            return Some(value[1..].split_once(quote)?.0.to_owned());
        }
    }
    None
}

fn go(cwd: &Path) -> Option<String> {
    version_in_project(
        cwd,
        &["go.mod", "go.work", ".go-version"],
        "go",
        "go",
        &["version"],
    )
}

fn rust(cwd: &Path) -> Option<String> {
    version_in_project(
        cwd,
        &["Cargo.toml", "rust-toolchain", "rust-toolchain.toml"],
        "rust",
        "rustc",
        &["--version"],
    )
}

fn node(cwd: &Path) -> Option<String> {
    version_in_project(
        cwd,
        &["package.json", ".node-version", ".nvmrc"],
        "node",
        "node",
        &["--version"],
    )
}

fn java(cwd: &Path) -> Option<String> {
    version_in_project(
        cwd,
        &[
            "pom.xml",
            "build.gradle",
            "build.gradle.kts",
            ".java-version",
        ],
        "java",
        "java",
        &["--version"],
    )
}

fn ruby(cwd: &Path) -> Option<String> {
    version_in_project(
        cwd,
        &["Gemfile", ".ruby-version"],
        "ruby",
        "ruby",
        &["--version"],
    )
}

fn environment(_: &Path) -> Option<String> {
    let name = env::var_os("VIRTUAL_ENV")
        .and_then(|path| {
            Path::new(&path)
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
        })
        .or_else(|| env::var("CONDA_DEFAULT_ENV").ok());
    Some(
        name.map(|name| format!("env:{}", clean(&name)))
            .unwrap_or_default(),
    )
}

fn python(cwd: &Path) -> Option<String> {
    if project_file(
        cwd,
        &[
            ".python-version",
            "pyproject.toml",
            "requirements.txt",
            "Pipfile",
            "uv.lock",
        ],
    )
    .is_none()
        && env::var_os("VIRTUAL_ENV").is_none()
        && env::var_os("CONDA_PREFIX").is_none()
    {
        return Some(String::new());
    }
    let mut failed = false;
    for bin in ["python", "python3"] {
        match version("py", bin, &["--version"]) {
            Some(result) if result.is_empty() => continue,
            Some(result) => return Some(result),
            None => failed = true,
        }
    }
    (!failed).then(String::new)
}

fn version_in_project(
    cwd: &Path,
    files: &[&str],
    label: &str,
    bin: &str,
    args: &[&str],
) -> Option<String> {
    if project_file(cwd, files).is_none() {
        return Some(String::new());
    }
    version(label, bin, args)
}

fn version(label: &str, bin: &str, args: &[&str]) -> Option<String> {
    // 在实际的 PATH 和工作目录里运行，交由 shim 或 wrapper 选择版本。
    let Some(path) = env::var_os("PATH") else {
        return Some(String::new());
    };
    if !env::split_paths(&path).any(|directory| directory.join(bin).is_file()) {
        return Some(String::new());
    }
    let (status, output) = capture(Command::new(bin).args(args))?;
    let mut text = String::new();
    output.take(MAX_SAMPLE).read_to_string(&mut text).ok()?;
    let reported = status
        .success()
        .then(|| {
            text.lines()
                .next()
                .unwrap_or_default()
                .split_whitespace()
                .find_map(|word| {
                    let version = word.trim_start_matches("go").trim_start_matches('v');
                    version
                        .as_bytes()
                        .first()
                        .is_some_and(u8::is_ascii_digit)
                        .then_some(version)
                })
        })
        .flatten();
    reported.map(|version| format!("{label}:{}", clean(version)))
}
