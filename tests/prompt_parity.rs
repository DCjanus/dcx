#![cfg(unix)]

use std::ffi::OsStr;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::process::Command;

struct Fixture {
    home: tempfile::TempDir,
    cwd: PathBuf,
}

impl Fixture {
    fn new() -> Self {
        let home = tempfile::tempdir().unwrap();
        let cwd = home.path().canonicalize().unwrap().join("project");
        fs::create_dir(&cwd).unwrap();
        fs::create_dir(cwd.join("bin")).unwrap();
        let fixture = Self { home, cwd };
        fixture.write(
            "starship.toml",
            "[username]\ndisabled = true\n[maven]\ndisabled = true\n",
        );
        fixture
    }

    fn command(&self, executable: impl AsRef<OsStr>) -> Command {
        let mut command = Command::new(executable);
        command
            .current_dir(&self.cwd)
            .env_clear()
            .env("HOME", self.cwd.parent().unwrap())
            .env(
                "PATH",
                format!("{}:/usr/bin:/bin", self.cwd.join("bin").display()),
            )
            .env("XDG_CACHE_HOME", self.home.path().join("cache"))
            .env("TERM", "xterm-256color")
            .env("STARSHIP_SHELL", "fish")
            .env("STARSHIP_CONFIG", self.cwd.join("starship.toml"))
            .env("DCX_PROMPT_STYLE", "nerd")
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .env("GIT_AUTHOR_NAME", "Fixture")
            .env("GIT_AUTHOR_EMAIL", "fixture@example.invalid")
            .env("GIT_COMMITTER_NAME", "Fixture")
            .env("GIT_COMMITTER_EMAIL", "fixture@example.invalid");
        command
    }

    fn write(&self, path: &str, text: &str) {
        fs::write(self.cwd.join(path), text).unwrap();
    }

    fn tool(&self, name: &str, script: &str) {
        let path = format!("bin/{name}");
        self.write(&path, &format!("#!/bin/sh\n{script}\n"));
        fs::set_permissions(self.cwd.join(path), fs::Permissions::from_mode(0o755)).unwrap();
    }

    fn git(&self, args: &[&str]) -> String {
        output(self.command("git").args(args))
    }

    fn init_git(&self) {
        self.git(&["init", "-qb", "main"]);
        self.write("tracked", "seed\n");
        self.write(".gitignore", "/bin/\n/starship.toml\n");
        self.git(&["add", "tracked", ".gitignore"]);
        self.git(&["commit", "-qm", "seed"]);
    }

    fn render(&self, args: &[&str], environment: &[(&str, &str)]) -> String {
        let bin = env!("CARGO_BIN_EXE_dcx");
        output(self.command(bin).envs(environment.iter().copied()).args([
            "prompt",
            "refresh",
            "--shell-pid=0",
            "--shown=0",
            "--after-command",
            "18446744073709551615",
        ]));
        output(
            self.command(bin)
                .envs(environment.iter().copied())
                .args([
                    "prompt",
                    "render",
                    "--shell-pid=0",
                    "--hostname=devbox.example",
                ])
                .args(args),
        )
    }

    fn oracle(&self, args: &[&str], environment: &[(&str, &str)]) -> Option<String> {
        let binary = std::env::var_os("DCX_TEST_STARSHIP")?;
        Some(output(
            self.command(binary)
                .envs(environment.iter().copied())
                .args(args),
        ))
    }
}

fn output(command: &mut Command) -> String {
    let result = command.output().unwrap();
    assert!(
        result.status.success(),
        "{command:?}: {}",
        String::from_utf8_lossy(&result.stderr)
    );
    String::from_utf8(result.stdout).unwrap()
}

fn status(fixture: &Fixture, expected: &str) {
    let colors = fixture.render(&["--status=0", "--duration=0", "--jobs=0"], &[]);
    let marker = format!("\x1b[1;31m{expected}\x1b[0m");
    assert!(colors.contains(&marker), "{colors:?}");
    if let Some(oracle) = fixture.oracle(&["module", "git_status"], &[]) {
        assert_eq!(oracle.trim_end(), marker);
    }
}

#[test]
fn git_stash_counts_and_worktree_states_match_starship() {
    let fixture = Fixture::new();
    fixture.init_git();
    let prompt = fixture.render(&["--status=0", "--duration=0", "--jobs=0"], &[]);
    assert!(prompt.contains(" on \x1b[1;35m main\x1b[0m\n"));
    for text in ["first\n", "second\n"] {
        fixture.write("tracked", text);
        fixture.git(&["stash", "push", "-q"]);
        status(&fixture, "[$]");
    }
    fixture.write("tracked", "modified\n");
    status(&fixture, "[$!]");
    fixture.git(&["add", "tracked"]);
    fixture.write("untracked", "new\n");
    status(&fixture, "[$+?]");
    fs::remove_file(fixture.cwd.join("tracked")).unwrap();
    status(&fixture, "[$✘+?]");
    fixture.git(&["restore", "--staged", "--worktree", "tracked"]);
    fixture.git(&["config", "status.renames", "false"]);
    fixture.git(&["mv", "tracked", "renamed"]);
    status(&fixture, "[$»?]");
    fixture.write("renamed", "renamed and modified\n");
    status(&fixture, "[$»!?]");
    fixture.git(&["restore", "--staged", "--worktree", "."]);
    fixture.git(&["rm", "tracked"]);
    status(&fixture, "[$✘?]");
    fixture.git(&["stash", "clear"]);
    status(&fixture, "[✘?]");
}

#[test]
fn git_ahead_behind_diverged_and_detached_match_starship() {
    let fixture = Fixture::new();
    fixture.init_git();
    fixture.git(&["branch", "--track", "other", "main"]);
    fixture.git(&["branch", "--set-upstream-to=other", "main"]);
    fixture.git(&["commit", "-qm", "ahead", "--allow-empty"]);
    status(&fixture, "[⇡]");
    let result = fixture.render(&["--status=0", "--duration=0", "--jobs=0"], &[]);
    assert!(result.contains(" main:other"), "{result:?}");
    fixture.git(&["switch", "-q", "other"]);
    fixture.git(&["commit", "-qm", "other", "--allow-empty"]);
    fixture.git(&["switch", "-q", "main"]);
    status(&fixture, "[⇕]");
    fixture.git(&["reset", "--hard", "HEAD^"]);
    status(&fixture, "[⇣]");
    let hash = fixture.git(&["rev-parse", "--short=7", "HEAD"]);
    fixture.git(&["switch", "-q", "--detach"]);
    let result = fixture.render(&["--status=0", "--duration=0", "--jobs=0"], &[]);
    assert!(
        result.contains(&format!(
            "on \x1b[1;35m HEAD\x1b[0m \x1b[1;32m({})\x1b[0m",
            hash.trim()
        )),
        "{result:?}"
    );
}

#[test]
fn git_merge_conflict_shows_operation_and_conflict_marker() {
    let fixture = Fixture::new();
    fixture.init_git();
    fixture.git(&["switch", "-qc", "other"]);
    fixture.write("tracked", "other\n");
    fixture.git(&["commit", "-qam", "other"]);
    fixture.git(&["switch", "-q", "main"]);
    fixture.write("tracked", "main\n");
    fixture.git(&["commit", "-qam", "main"]);
    let merge = fixture
        .command("git")
        .args(["merge", "other"])
        .output()
        .unwrap();
    assert_eq!(merge.status.code(), Some(1));
    status(&fixture, "[=]");
    let result = fixture.render(&["--status=0", "--duration=0", "--jobs=0"], &[]);
    assert!(result.contains("(\x1b[1;33mMERGING\x1b[0m)"), "{result:?}");
}

#[test]
fn nerd_tool_versions_have_starship_connectors_colors_and_order() {
    let fixture = Fixture::new();
    for (path, contents) in [
        ("Cargo.toml", "[package]\nversion = \"1.2.3\"\n"),
        ("package.json", "{\"version\":\"9.8.7\"}"),
        ("pyproject.toml", "[project]\nversion = \"3.2.1\"\n"),
        ("go.mod", "module example.invalid/foo\n"),
        ("pom.xml", "<project/>"),
        ("Gemfile", ""),
    ] {
        fixture.write(path, contents);
    }
    for (tool, version) in [
        ("go", "go version go1.27.4 darwin/arm64"),
        ("node", "v24.1.2"),
        ("python", "Python 3.15.2"),
        ("ruby", "ruby 3.4.8p123"),
        ("rustc", "rustc 1.100.0"),
    ] {
        fixture.tool(tool, &format!("printf '%s\\n' '{version}'"));
    }
    fixture.tool("java", r#"if [ "$1" = "-Xinternalversion" ]; then
    printf '%s\n' 'OpenJDK Runtime Environment (26.0.1.4+8) for linux-amd64 JRE (26.0.1.4+8), built on Jan 01 2026 01:00:00' >&2
else
    printf '%s\n' 'openjdk 26.0.1'
fi"#);
    let expected = [
        "is \x1b[1;38;5;208m📦 v1.2.3",
        "via \x1b[1;36m🐹 v1.27.4",
        "via \x1b[2;31m☕ v26.0.1",
        "via \x1b[1;32m v24.1.2",
        "via \x1b[1;33m🐍 v3.15.2 (venv)",
        "via \x1b[1;31m💎 v3.4.8",
        "via \x1b[1;31m🦀 v1.100.0",
        "via \x1b[1;32m🅒 science",
    ];
    let environment = [
        ("VIRTUAL_ENV", "/fixture/venv"),
        ("CONDA_DEFAULT_ENV", "science"),
        ("CONDA_SHLVL", "1"),
    ];
    let args = ["--status=0", "--duration=0", "--jobs=0", "--width=240"];
    let result = fixture.render(&args, &environment);
    assert_in_order(&result, &expected);
    if let Some(oracle) = fixture.oracle(&["prompt", "--terminal-width=240"], &environment) {
        assert_in_order(&oracle, &expected);
    }
    let base = fixture.render(&args, &[("CONDA_DEFAULT_ENV", "base")]);
    assert!(!base.contains("🅒"), "{base:?}");
}

fn assert_in_order(mut text: &str, expected: &[&str]) {
    for part in expected {
        let (_, tail) = text
            .split_once(part)
            .unwrap_or_else(|| panic!("Missing {part:?} in {text:?}"));
        text = tail;
    }
}

#[test]
fn duration_and_fish_modes_match_starship() {
    let fixture = Fixture::new();
    for (duration, expected) in [
        (1999, ""),
        (2000, "2s"),
        (2500, "2s"),
        (59999, "59s"),
        (60000, "1m0s"),
        (3661000, "1h1m1s"),
        (90061000, "1d1h1m1s"),
    ] {
        let arg = format!("--duration={duration}");
        let prompt = fixture.render(&["--status=0", "--jobs=2", &arg], &[]);
        assert!(
            prompt.ends_with("\n\x1b[1;34m✦2\x1b[0m \x1b[1;32m❯\x1b[0m "),
            "{prompt:?}"
        );
        verify_duration(&prompt, expected);
        let arg = format!("--cmd-duration={duration}");
        if let Some(oracle) = fixture.oracle(&["prompt", "--jobs=2", &arg], &[]) {
            verify_duration(&oracle, expected);
            assert!(oracle.ends_with("\n\x1b[1;34m✦2\x1b[0m \x1b[1;32m❯\x1b[0m "));
        }
    }
    for (mode, color, character) in [
        ("insert", 31, '❯'),
        ("default", 32, '❮'),
        ("visual", 33, '❮'),
        ("replace", 35, '❮'),
        ("replace_one", 35, '❮'),
    ] {
        let arg = format!("--mode={mode}");
        let prompt = fixture.render(&["--status=7", "--jobs=1", "--duration=0", &arg], &[]);
        let tail = format!("\n\x1b[1;34m✦\x1b[0m \x1b[1;{color}m{character}\x1b[0m ");
        assert!(prompt.ends_with(&tail), "{prompt:?}");
        let arg = format!("--keymap={mode}");
        if let Some(oracle) = fixture.oracle(&["prompt", "--status=7", "--jobs=1", &arg], &[]) {
            assert!(oracle.ends_with(&tail), "{oracle:?}");
        }
    }
}

fn verify_duration(prompt: &str, expected: &str) {
    if expected.is_empty() {
        assert!(!prompt.contains("took "), "{prompt:?}");
    } else {
        assert!(
            prompt.contains(&format!("took \x1b[1;33m{expected}\x1b[0m")),
            "{prompt:?}"
        );
    }
}

#[test]
fn ssh_only_hostname_matches_starship_style() {
    let fixture = Fixture::new();
    let args = ["--status=0", "--duration=0", "--jobs=0"];
    assert!(!fixture.render(&args, &[]).contains("🌐"));
    for variable in ["SSH_CONNECTION", "SSH_CLIENT", "SSH_TTY"] {
        let result = fixture.render(&args, &[(variable, "fixture")]);
        assert!(
            result.contains("\x1b[1;2;32m🌐 devbox\x1b[0m in "),
            "{result:?}"
        );
        if let Some(oracle) = fixture.oracle(&["module", "hostname"], &[(variable, "fixture")]) {
            assert!(
                oracle.starts_with("\x1b[1;2;32m🌐 ") && oracle.ends_with("\x1b[0m in "),
                "{oracle:?}"
            );
        }
    }
}

#[test]
fn full_nerd_prompt_matches_default_starship_in_dirty_repository() {
    let fixture = Fixture::new();
    fixture.init_git();
    fixture.write("Cargo.toml", "[package]\nversion = \"1.2.3\"\n");
    fixture.tool("rustc", "printf 'rustc 1.100.0\n'");
    fixture.git(&["add", "Cargo.toml"]);
    fixture.git(&["commit", "-qm", "manifest"]);
    fixture.write("tracked", "modified\n");
    fixture.write("untracked", "file\n");
    let args = ["--status=1", "--duration=2456", "--jobs=2", "--width=120"];
    let result = visible_lines(&fixture.render(&args, &[]));
    assert_eq!(
        result,
        "\nproject on  main [!?] is 📦 v1.2.3 via 🦀 v1.100.0 took 2s\n✦2 ❯"
    );
    if let Some(oracle) = fixture.oracle(
        &[
            "prompt",
            "--status=1",
            "--cmd-duration=2456",
            "--jobs=2",
            "--terminal-width=120",
        ],
        &[],
    ) {
        assert_eq!(result, visible_lines(&oracle));
    }
}

fn visible_lines(prompt: &str) -> String {
    let mut text = String::new();
    let mut characters = prompt.chars();
    while let Some(character) = characters.next() {
        if character == '\x1b' && characters.next() == Some('[') {
            for code in characters.by_ref() {
                if ('@'..='~').contains(&code) {
                    break;
                }
            }
        } else {
            text.push(character);
        }
    }
    text.lines()
        .map(str::trim_end)
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn private_javascript_package_is_hidden_but_node_is_present() {
    let fixture = Fixture::new();
    fixture.write("package.json", "{\"private\":true,\"version\":\"1.2.3\"}");
    fixture.tool("node", "printf 'v24.1.2\n'");
    let prompt = fixture.render(&["--status=0", "--duration=0", "--jobs=0"], &[]);
    assert!(!prompt.contains("📦"), "{prompt:?}");
    assert!(prompt.contains("via \x1b[1;32m v24.1.2"), "{prompt:?}");
    if let Some(oracle) = fixture.oracle(&["prompt"], &[]) {
        assert!(!oracle.contains("📦"), "{oracle:?}");
        assert!(oracle.contains("via \x1b[1;32m v24.1.2"), "{oracle:?}");
    }
}

#[test]
fn no_color_and_ascii_fallback_preserve_prompt_contents() {
    let fixture = Fixture::new();
    fixture.init_git();
    fixture.write("untracked", "file\n");
    let args = ["--status=7", "--duration=2400", "--jobs=1"];
    let uncolored = fixture.render(&args, &[("NO_COLOR", "1")]);
    assert_eq!(uncolored, "\nproject on  main [?] took 2s\n✦ ❯ ");
    let ascii = fixture.render(&args, &[("NO_COLOR", "1"), ("DCX_PROMPT_STYLE", "ascii")]);
    assert_eq!(ascii, "project on main untracked 1 job took 2.4s\n[7] > ");
}

#[test]
fn narrow_terminal_keeps_connectors_with_their_module() {
    let fixture = Fixture::new();
    fixture.init_git();
    fixture.write("Cargo.toml", "[package]\nversion = \"1.2.3\"\n");
    fixture.tool("rustc", "printf 'rustc 1.100.0\n'");
    let args = ["--status=0", "--duration=0", "--jobs=0", "--width=23"];
    let result = fixture.render(&args, &[("NO_COLOR", "1")]);
    assert_eq!(
        result,
        "\nproject on  main [?]\nis 📦 v1.2.3\nvia 🦀 v1.100.0\n❯ "
    );
}

#[test]
fn source_files_without_manifests_activate_language_modules() {
    let fixture = Fixture::new();
    let args = ["--status=0", "--duration=0", "--jobs=0", "--width=240"];
    for (file, tool, reply, expected) in [
        (
            "main.go",
            "go",
            "go version go1.27.4 linux/amd64",
            "🐹 v1.27.4",
        ),
        ("index.mts", "node", "v24.1.2", " v24.1.2"),
        ("script.py", "python", "Python 3.15.2", "🐍 v3.15.2"),
        ("script.rb", "ruby", "ruby 3.4.8", "💎 v3.4.8"),
        ("main.rs", "rustc", "rustc 1.100.0", "🦀 v1.100.0"),
    ] {
        fixture.tool(tool, &format!("printf '%s\\n' '{reply}'"));
        assert!(!fixture.render(&args, &[]).contains(expected));
        fixture.write(file, "");
        assert!(fixture.render(&args, &[]).contains(expected));
        if let Some(oracle) = fixture.oracle(&["prompt", "--terminal-width=240"], &[]) {
            assert!(oracle.contains(expected), "{oracle:?}");
        }
    }
    fixture.write("deno.json", "{}");
    assert!(!fixture.render(&args, &[]).contains(""));
    if let Some(oracle) = fixture.oracle(&["prompt"], &[]) {
        assert!(!oracle.contains(""), "{oracle:?}");
    }
}

#[test]
fn conda_path_uses_basename_and_does_not_enable_python() {
    let fixture = Fixture::new();
    fixture.tool("python", "printf 'Python 3.15.2\n'");
    let args = ["--status=0", "--duration=0", "--jobs=0"];
    let conda = [
        ("CONDA_DEFAULT_ENV", "/fixture/conda/science"),
        ("CONDA_PREFIX", "/fixture/conda/science"),
        ("CONDA_SHLVL", "1"),
    ];
    let result = fixture.render(&args, &conda);
    assert!(result.contains("via \x1b[1;32m🅒 science"), "{result:?}");
    assert!(!result.contains("🐍"), "{result:?}");
    if let Some(oracle) = fixture.oracle(&["prompt"], &conda) {
        assert!(
            oracle.contains("via \x1b[1;32m🅒 science") && !oracle.contains("🐍"),
            "{oracle:?}"
        );
    }
    let mut pixi = conda.to_vec();
    pixi.push(("PIXI_ENVIRONMENT_NAME", "default"));
    assert!(!fixture.render(&args, &pixi).contains("🅒"));
}

#[test]
fn rebase_reports_current_and_total_commits() {
    let fixture = Fixture::new();
    fixture.init_git();
    fixture.git(&["switch", "-qc", "feature"]);
    fixture.write("tracked", "feature\n");
    fixture.git(&["commit", "-qam", "feature"]);
    fixture.write("extra", "second commit\n");
    fixture.git(&["add", "extra"]);
    fixture.git(&["commit", "-qm", "extra"]);
    fixture.git(&["switch", "-q", "main"]);
    fixture.write("tracked", "main\n");
    fixture.git(&["commit", "-qam", "main"]);
    fixture.git(&["switch", "-q", "feature"]);
    let rebase = fixture
        .command("git")
        .args(["rebase", "main"])
        .output()
        .unwrap();
    assert_eq!(rebase.status.code(), Some(1));
    let result = fixture.render(&["--status=0", "--duration=0", "--jobs=0"], &[]);
    let operation = "(\x1b[1;33mREBASING 1/2\x1b[0m)";
    assert!(result.contains(operation), "{result:?}");
    if let Some(oracle) = fixture.oracle(&["module", "git_state"], &[]) {
        assert_eq!(oracle.trim_end(), operation);
    }
}

#[test]
fn cargo_workspace_and_members_show_inherited_package_version() {
    let mut fixture = Fixture::new();
    fixture.write(
        "Cargo.toml",
        "[workspace]\nmembers = [\"member\"]\n[workspace.package]\nversion = \"0.8.1\"\n",
    );
    let workspace = fixture.cwd.clone();
    let config = fs::read_to_string(workspace.join("starship.toml")).unwrap();
    let member = workspace.join("member");
    fs::create_dir(&member).unwrap();
    for (cwd, manifest) in [
        (&workspace, ""),
        (
            &member,
            "[package]\nname = \"member\"\nversion.workspace = true # inherit\n",
        ),
        (
            &member,
            "[package]\nname = \"member\"\nversion = { workspace = true }\n",
        ),
    ] {
        fixture.cwd = cwd.clone();
        if !manifest.is_empty() {
            fixture.write("Cargo.toml", manifest);
            fixture.write("starship.toml", &config);
        }
        let prompt = fixture.render(&["--status=0", "--duration=0", "--jobs=0"], &[]);
        let expected = "is \x1b[1;38;5;208m📦 v0.8.1";
        assert!(prompt.contains(expected), "{prompt:?}");
        if let Some(oracle) = fixture.oracle(&["module", "package"], &[]) {
            assert!(oracle.contains(expected), "{oracle:?}");
        }
    }
}

#[test]
fn parses_valid_toml_and_poetry_package_versions() {
    let long_manifest = format!(
        "[package]\nversion = \"5.6.7\"\ndescription = \"{}\"\n",
        "a".repeat(70_000),
    );
    for (path, manifest, version) in [
        ("Cargo.toml", long_manifest.as_str(), "5.6.7"),
        (
            "Cargo.toml",
            "[package] # metadata\nversion = \"5.6.7\"\n",
            "5.6.7",
        ),
        (
            "Cargo.toml",
            "package = { name = \"project\", version = \"5.6.7\" }\n",
            "5.6.7",
        ),
        (
            "pyproject.toml",
            "[tool.poetry]\nversion = \"5.6.7\"\n",
            "5.6.7",
        ),
        (
            "pyproject.toml",
            "[project]\nversion = \"1.2.3\"\n[tool.poetry]\nversion = \"5.6.7\"\n",
            "1.2.3",
        ),
        ("package.json", "{\"version\":\" v5.6.7 \"}", "5.6.7"),
    ] {
        let fixture = Fixture::new();
        fixture.write(path, manifest);
        let expected = format!("is \x1b[1;38;5;208m📦 v{version}\x1b[0m");
        let prompt = fixture.render(&["--status=0", "--duration=0", "--jobs=0"], &[]);
        assert!(prompt.contains(&expected), "{prompt:?}");
        if let Some(oracle) = fixture.oracle(&["module", "package"], &[]) {
            assert!(oracle.contains(&expected), "{oracle:?}");
        }
    }
}

#[test]
fn python_version_accepts_stderr_from_the_selected_interpreter() {
    let fixture = Fixture::new();
    fixture.write("script.py", "");
    fixture.tool("python", "printf 'Python 2.7.18\n' >&2");
    fixture.tool("python3", "printf 'Python 3.15.2\n'");
    let expected = "via \x1b[1;33m🐍 v2.7.18";
    let prompt = fixture.render(&["--status=0", "--duration=0", "--jobs=0"], &[]);
    assert!(prompt.contains(expected), "{prompt:?}");
    if let Some(oracle) = fixture.oracle(&["module", "python"], &[]) {
        assert!(oracle.contains(expected), "{oracle:?}");
    }

    fixture.tool(
        "python",
        "printf 'Python 2.7.18\n' >&2; printf 'Python 3.4.5\n'",
    );
    let expected = "via \x1b[1;33m🐍 v3.4.5";
    let prompt = fixture.render(&["--status=0", "--duration=0", "--jobs=0"], &[]);
    assert!(prompt.contains(expected), "{prompt:?}");
    if let Some(oracle) = fixture.oracle(&["module", "python"], &[]) {
        assert!(oracle.contains(expected), "{oracle:?}");
    }
}
