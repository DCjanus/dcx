use std::env;
use std::fs::{self, File};
use std::io::{BufRead, BufReader, Read};
use std::path::Path;
use std::process::Command;

use super::{MAX_SAMPLE, ProbeStderr, capture, capture_output, clean, fingerprint};

#[derive(Clone, Copy)]
pub(super) struct Probe {
    pub name: &'static str,
    collect: fn(&Path, SourceKinds) -> Option<String>,
}

pub(super) const ALL: [Probe; 10] = [
    Probe {
        name: "git",
        collect: |cwd, _| git(cwd),
    },
    Probe {
        name: "package",
        collect: |cwd, _| package(cwd),
    },
    Probe {
        name: "go",
        collect: go,
    },
    Probe {
        name: "java",
        collect: java,
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
        collect: |cwd, _| environment(cwd),
    },
    Probe {
        name: "ruby",
        collect: ruby,
    },
    Probe {
        name: "rust",
        collect: rust,
    },
    Probe {
        name: "conda",
        collect: |cwd, _| conda(cwd),
    },
];

pub(super) fn collect(probe: &Probe, cwd: &Path, kinds: SourceKinds) -> Option<String> {
    (probe.collect)(cwd, kinds)
}

const GO_FILES: &[&str] = &[
    "go.mod",
    "go.sum",
    "go.work",
    "glide.yaml",
    "Gopkg.yml",
    "Gopkg.lock",
    ".go-version",
];
const JAVA_FILES: &[&str] = &[
    "pom.xml",
    "build.gradle.kts",
    "build.sbt",
    ".java-version",
    "deps.edn",
    "project.clj",
    "build.boot",
    ".sdkmanrc",
];
const NODE_FILES: &[&str] = &["package.json", ".node-version", ".nvmrc"];
const NODE_EXCLUSIONS: &[&str] = &[
    "bunfig.toml",
    "bun.lock",
    "bun.lockb",
    "deno.json",
    "deno.jsonc",
    "deno.lock",
];
const PYTHON_FILES: &[&str] = &[
    "requirements.txt",
    ".python-version",
    "pyproject.toml",
    "Pipfile",
    "tox.ini",
    "setup.py",
    "__init__.py",
];
const RUST_FILES: &[&str] = &["Cargo.toml"];
const RUBY_FILES: &[&str] = &["Gemfile", ".ruby-version"];
fn nerd() -> bool {
    env::var_os("DCX_PROMPT_STYLE").is_some_and(|style| style == "nerd")
}

#[derive(Clone, Copy, Default, Hash)]
pub(super) struct SourceKinds {
    go: bool,
    java: bool,
    node: bool,
    python: bool,
    rust: bool,
    ruby: bool,
}

pub(super) fn source_kinds(cwd: &Path) -> SourceKinds {
    let mut kinds = SourceKinds::default();
    if let Ok(entries) = std::fs::read_dir(cwd) {
        for entry in entries.flatten() {
            let name = entry.file_name();
            if entry.file_type().is_ok_and(|kind| kind.is_dir()) {
                continue;
            }
            match Path::new(&name)
                .extension()
                .and_then(|extension| extension.to_str())
            {
                Some("go") => kinds.go = true,
                Some("java" | "class" | "jar" | "gradle" | "clj" | "cljc") => kinds.java = true,
                Some("js" | "mjs" | "cjs" | "ts" | "mts" | "cts") => kinds.node = true,
                Some("py" | "ipynb") => kinds.python = true,
                Some("rs") => kinds.rust = true,
                Some("rb") => kinds.ruby = true,
                _ => {}
            }
        }
    }
    kinds
}

fn detected(cwd: &Path, files: &[&str], has_source: bool) -> bool {
    if nerd() {
        files.iter().any(|file| cwd.join(file).is_file()) || has_source
    } else {
        project_file(cwd, files).is_some() || has_source
    }
}

pub(super) fn selection_fingerprint(cwd: &Path, kinds: SourceKinds) -> u64 {
    // 同一个 shim 文件可以在项目配置修改后指向另一个版本。
    let mut selectors = Vec::new();
    for ancestor in cwd.ancestors() {
        let git = ancestor.join(".git");
        if git.exists() {
            selectors.push((git, 0, None));
        }
        for name in [
            "mise.toml",
            ".mise.toml",
            "mise.local.toml",
            ".mise.local.toml",
            ".tool-versions",
            "rust-toolchain",
            "rust-toolchain.toml",
            "uv.lock",
        ]
        .into_iter()
        .chain(
            [
                GO_FILES,
                JAVA_FILES,
                NODE_FILES,
                NODE_EXCLUSIONS,
                PYTHON_FILES,
                RUST_FILES,
                RUBY_FILES,
            ]
            .into_iter()
            .flatten()
            .copied(),
        ) {
            let path = ancestor.join(name);
            if let Ok(metadata) = path.metadata() {
                selectors.push((path, metadata.len(), metadata.modified().ok()));
            }
        }
    }
    fingerprint(&(
        selectors,
        kinds,
        cwd.join("node_modules").is_dir(),
        cwd.join("Godeps").is_dir(),
    ))
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
        "-c",
        "status.renames=true",
        "status",
        "--porcelain=v2",
        "--branch",
        "--show-stash",
    ]))?;
    if !status.success() {
        // Git 返回 128 时没有可用的仓库状态，例如切出仓库或移除 .git。
        return (status.code() == Some(128)).then(String::new);
    }
    let repository = gix::discover_with_environment_overrides(cwd).ok();
    let upstream = repository.as_ref().and_then(|repository| {
        let head = repository.head_name().ok()??;
        let upstream = repository
            .branch_remote_ref_name(head.as_ref(), gix::remote::Direction::Fetch)?
            .ok()?;
        (upstream.shorten() != head.shorten()).then(|| upstream.shorten().to_string())
    });
    let mut summary = git_summary(BufReader::new(output), upstream.as_deref())?;
    if let Some(repository) = &repository
        && let Some(operation) = repository.state()
    {
        use gix::state::InProgress::*;
        summary.push(' ');
        let label = match operation {
            Rebase | RebaseInteractive | ApplyMailboxRebase => "rebase",
            Merge => "merge",
            CherryPick | CherryPickSequence => "cherry-pick",
            Revert | RevertSequence => "revert",
            Bisect => "bisect",
            ApplyMailbox => "am",
        };
        summary.push_str(label);
        if matches!(label, "rebase" | "am")
            && let Some((current, total)) = rebase_progress(repository.git_dir())
        {
            summary.push_str(&format!(":{current}/{total}"));
        }
    }
    if let Some(depth) = repository
        .as_ref()
        .and_then(|repository| repository.workdir())
        .filter(|root| !env::var_os("HOME").is_some_and(|home| *root == Path::new(&home)))
        .and_then(|root| repository_depth(cwd, root))
    {
        summary = format!("{depth}\n{summary}");
    }
    Some(summary)
}

fn rebase_progress(git_dir: &Path) -> Option<(u32, u32)> {
    let read = |name| {
        let mut value = String::new();
        File::open(git_dir.join(name))
            .ok()?
            .take(32)
            .read_to_string(&mut value)
            .ok()?;
        value.trim().parse().ok()
    };
    let merge_progress = || Some((read("rebase-merge/msgnum")?, read("rebase-merge/end")?));
    merge_progress().or_else(|| Some((read("rebase-apply/next")?, read("rebase-apply/last")?)))
}

fn repository_depth(cwd: &Path, root: &Path) -> Option<usize> {
    let logical = env::var_os("PWD");
    let logical = logical
        .as_deref()
        .map(Path::new)
        .filter(|path| path.is_absolute())
        .unwrap_or(cwd);
    if logical == cwd {
        return cwd
            .strip_prefix(root)
            .ok()
            .map(|relative| relative.components().count());
    }
    // 软链接可能直接指向仓库子目录，逻辑路径的祖先不一定经过仓库根。
    let root = root.canonicalize().ok()?;
    logical
        .ancestors()
        .position(|ancestor| ancestor.canonicalize().is_ok_and(|path| path == root))
}

fn git_summary(output: impl BufRead, upstream_name: Option<&str>) -> Option<String> {
    let mut branch = None;
    let mut oid = None;
    let mut upstream = None;
    let mut stashes = None;
    let mut staged = false;
    let mut modified = false;
    let mut untracked = false;
    let mut conflicted = false;
    let mut deleted = false;
    let mut renamed = false;
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
                    staged |= matches!(index, b'A' | b'M' | b'T');
                    modified |= matches!(worktree, b'A' | b'M');
                    deleted |= *index == b'D' || *worktree == b'D';
                    renamed |= *index == b'R' || *worktree == b'R';
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
        "HEAD"
    } else {
        &branch
    };
    let mut parts = vec![format!("git:{head}")];
    if let Some(upstream_name) = upstream_name.filter(|_| head != "HEAD") {
        parts.push(format!("upstream:{upstream_name}"));
    }
    if branch == "(detached)"
        && let Some(hash) = oid.as_deref().and_then(|oid| oid.get(..7))
    {
        parts.push(format!("commit:{hash}"));
    }
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
    if deleted {
        parts.push("deleted".into());
    }
    if renamed {
        parts.push("renamed".into());
    }
    if let Some(count) = stashes {
        parts.push(format!("stash:{count}"));
    }
    Some(parts.join(" "))
}

fn package(cwd: &Path) -> Option<String> {
    let directory = if nerd() {
        cwd
    } else {
        let Some(directory) = project_file(cwd, &["package.json", "Cargo.toml", "pyproject.toml"])
        else {
            return Some(String::new());
        };
        directory
    };
    let version = cargo_package_version(directory)
        .or_else(|| package_json(&directory.join("package.json")))
        .or_else(|| python_package_version(directory));
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
    if json.get("private").and_then(serde_json::Value::as_bool) == Some(true) {
        return None;
    }
    Some(json.get("version")?.as_str()?.to_owned())
}

fn python_package_version(directory: &Path) -> Option<String> {
    let manifest = read_toml(&directory.join("pyproject.toml"))?;
    manifest
        .get("project")
        .and_then(|project| project.get("version"))
        .and_then(toml::Value::as_str)
        .or_else(|| {
            manifest
                .get("tool")?
                .get("poetry")?
                .get("version")?
                .as_str()
        })
        .map(str::to_owned)
}

fn cargo_package_version(directory: &Path) -> Option<String> {
    let manifest = read_toml(&directory.join("Cargo.toml"))?;
    let version = manifest
        .get("package")
        .and_then(|package| package.get("version"));
    if let Some(version) = version.and_then(toml::Value::as_str) {
        return Some(version.to_owned());
    }
    let inherited = version
        .and_then(|version| version.get("workspace"))
        .and_then(toml::Value::as_bool)
        == Some(true);
    let workspace = if manifest.contains_key("workspace") || !inherited {
        manifest
    } else {
        let path = directory
            .ancestors()
            .skip(1)
            .map(|path| path.join("Cargo.toml"))
            .find(|path| path.is_file())?;
        read_toml(&path)?
    };
    Some(
        workspace
            .get("workspace")?
            .get("package")?
            .get("version")?
            .as_str()?
            .to_owned(),
    )
}

fn read_toml(path: &Path) -> Option<toml::Table> {
    if !path.is_file() {
        return None;
    }
    fs::read_to_string(path).ok()?.parse().ok()
}

fn go(cwd: &Path, kinds: SourceKinds) -> Option<String> {
    version_in_project(
        detected(cwd, GO_FILES, kinds.go) || cwd.join("Godeps").is_dir(),
        "go",
        "go",
        &["version"],
    )
}

fn rust(cwd: &Path, kinds: SourceKinds) -> Option<String> {
    let matches = detected(cwd, RUST_FILES, kinds.rust)
        || (!nerd() && project_file(cwd, &["rust-toolchain", "rust-toolchain.toml"]).is_some());
    version_in_project(matches, "rust", "rustc", &["--version"])
}

fn node(cwd: &Path, kinds: SourceKinds) -> Option<String> {
    let excluded = nerd() && NODE_EXCLUSIONS.iter().any(|name| cwd.join(name).is_file());
    version_in_project(
        !excluded && (detected(cwd, NODE_FILES, kinds.node) || cwd.join("node_modules").is_dir()),
        "node",
        "node",
        &["--version"],
    )
}

fn java(cwd: &Path, kinds: SourceKinds) -> Option<String> {
    if !detected(cwd, JAVA_FILES, kinds.java)
        && (nerd() || project_file(cwd, &["build.gradle"]).is_none())
    {
        return Some(String::new());
    }
    let binary = env::var_os("JAVA_HOME")
        .map(|home| Path::new(&home).join("bin/java"))
        .unwrap_or_else(|| "java".into());
    if let Some((status, output)) = capture_output(
        Command::new(&binary).arg("-Xinternalversion"),
        ProbeStderr::Merge,
    ) && status.success()
    {
        let mut text = String::new();
        output.take(MAX_SAMPLE).read_to_string(&mut text).ok()?;
        if let Some(details) = text.split_once(", built on").map(|(details, _)| details) {
            let raw = details
                .rsplit_once("JRE")
                .and_then(|(_, jre)| jre.rsplit_once('('))
                .or_else(|| details.split_once("OpenJ9 "));
            if let Some((_, raw)) = raw {
                let version = raw
                    .split(|character: char| !character.is_ascii_digit() && character != '.')
                    .next()
                    .unwrap_or_default()
                    .split('.')
                    .take(3)
                    .collect::<Vec<_>>()
                    .join(".");
                if !version.is_empty() {
                    return Some(format!("java:{version}"));
                }
            }
        }
    }
    version("java", binary.to_str()?, &["--version"])
        .map(|sample| with_detected_icon("java", sample))
}

fn ruby(cwd: &Path, kinds: SourceKinds) -> Option<String> {
    let matches = detected(cwd, RUBY_FILES, kinds.ruby)
        || ["RUBY_VERSION", "RBENV_VERSION"]
            .iter()
            .any(|name| env::var_os(name).is_some());
    version_in_project(matches, "ruby", "ruby", &["--version"])
}

fn environment(_: &Path) -> Option<String> {
    let name = env::var_os("VIRTUAL_ENV").and_then(|path| {
        Path::new(&path)
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
    });
    Some(
        name.map(|name| format!("env:{}", clean(&name)))
            .unwrap_or_default(),
    )
}

fn conda(_: &Path) -> Option<String> {
    let name = env::var_os("CONDA_DEFAULT_ENV")
        .filter(|_| env::var_os("PIXI_ENVIRONMENT_NAME").is_none())
        .and_then(|name| {
            Path::new(&name)
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
        })
        .filter(|name| name != "base");
    Some(name.map_or_else(String::new, |name| format!("conda:{}", clean(&name))))
}

fn python(cwd: &Path, kinds: SourceKinds) -> Option<String> {
    let matches = detected(cwd, PYTHON_FILES, kinds.python)
        || env::var_os("VIRTUAL_ENV").is_some()
        || (!nerd()
            && (project_file(cwd, &["uv.lock"]).is_some()
                || env::var_os("CONDA_PREFIX").is_some()));
    if !matches {
        return Some(String::new());
    }
    let mut failed = false;
    for bin in ["python", "python3", "python2"] {
        match version("py", bin, &["--version"]) {
            Some(result) if result.is_empty() => continue,
            Some(result) => return Some(result),
            None => failed = true,
        }
    }
    (!failed).then(|| with_detected_icon("py", String::new()))
}

fn version_in_project(matched: bool, label: &str, bin: &str, args: &[&str]) -> Option<String> {
    if !matched {
        return Some(String::new());
    }
    version(label, bin, args).map(|sample| with_detected_icon(label, sample))
}

fn with_detected_icon(label: &str, sample: String) -> String {
    if sample.is_empty() && nerd() {
        format!("{label}:")
    } else {
        sample
    }
}

fn version(label: &str, bin: &str, args: &[&str]) -> Option<String> {
    // 在实际的 PATH 和工作目录里运行，交由 shim 或 wrapper 选择版本。
    let Some(path) = env::var_os("PATH") else {
        return Some(String::new());
    };
    if !Path::new(bin).is_absolute()
        && !env::split_paths(&path).any(|directory| directory.join(bin).is_file())
    {
        return Some(String::new());
    }
    let stderr = if label == "py" {
        Some(tempfile::tempfile().ok()?)
    } else {
        None
    };
    let (status, output) = capture_output(
        Command::new(bin).args(args),
        stderr
            .as_ref()
            .map_or(ProbeStderr::Discard, ProbeStderr::Separate),
    )?;
    let mut text = String::new();
    output.take(MAX_SAMPLE).read_to_string(&mut text).ok()?;
    if text.trim().is_empty()
        && let Some(stderr) = stderr
    {
        stderr.take(MAX_SAMPLE).read_to_string(&mut text).ok()?;
    }
    let reported = status
        .success()
        .then(|| {
            let mut lines = text.lines();
            let first = if label == "py" {
                lines.find(|line| line.starts_with("Python "))
            } else {
                lines.next()
            };
            first
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
    if !status.success() {
        return None;
    }
    let Some(version) = reported else {
        return Some(String::new());
    };
    let version = if label == "ruby" {
        version.split_once('p').map_or(version, |(base, _)| base)
    } else {
        version
    };
    Some(format!("{label}:{}", clean(version)))
}
