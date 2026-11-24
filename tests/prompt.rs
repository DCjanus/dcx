#![cfg(unix)]

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::thread;
use std::time::{Duration, Instant};

use tempfile::TempDir;

struct Fixture {
    root: TempDir,
    project: PathBuf,
    bin: PathBuf,
}

impl Fixture {
    fn new() -> Self {
        let root = tempfile::tempdir().unwrap();
        let project = root.path().join("project");
        let bin = root.path().join("bin");
        fs::create_dir(&project).unwrap();
        fs::create_dir(&bin).unwrap();
        Self { root, project, bin }
    }

    fn tool(&self, name: &str, script: &str) {
        let path = self.bin.join(name);
        fs::write(&path, format!("#!/bin/sh\n{script}\n")).unwrap();
        fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
    }

    fn render(&self, cwd: &Path, args: &[&str]) -> String {
        let output = Command::new(env!("CARGO_BIN_EXE_dcx"))
            .current_dir(cwd)
            .env_clear()
            .env("HOME", self.root.path().canonicalize().unwrap())
            .env("PATH", format!("{}:/usr/bin:/bin", self.bin.display()))
            .env("NO_COLOR", "1")
            .args([
                "prompt",
                "render",
                "--shell-pid=0",
                "--status=7",
                "--duration=2400",
                "--jobs=2",
            ])
            .args(args)
            .output()
            .unwrap();
        assert!(output.status.success(), "{:?}", output);
        assert!(output.stderr.is_empty(), "{:?}", output);
        String::from_utf8(output.stdout).unwrap()
    }
}

fn wait_until(mut condition: impl FnMut() -> bool) {
    let started = Instant::now();
    while !condition() {
        assert!(
            started.elapsed() < Duration::from_secs(8),
            "refresh did not complete"
        );
        thread::sleep(Duration::from_millis(60));
    }
}

#[test]
fn renders_before_a_slow_probe_and_reuses_the_result_across_processes() {
    let fixture = Fixture::new();
    let project = &fixture.project;
    fs::write(project.join(".python-version"), "3.15.2\n").unwrap();
    fixture.tool("python", "sleep 1; printf 'Python 3.15.2\n'");

    let started = Instant::now();
    let cold = fixture.render(project, &[]);
    assert!(started.elapsed() < Duration::from_secs(1), "{cold}");
    assert!(
        cold.starts_with("~/project 2 jobs took 2.4s\n[7] > "),
        "{cold}"
    );
    wait_until(|| fixture.render(project, &[]).contains("py:3.15.2"));
    let warm = fixture.render(project, &[]);
    assert!(warm.contains("py:3.15.2"));
    assert!(
        !fixture
            .render(fixture.root.path(), &[])
            .contains("py:3.15.2")
    );
}

#[test]
fn refreshes_git_and_reresolves_a_shim_when_project_configuration_changes() {
    let fixture = Fixture::new();
    let project = &fixture.project;
    fs::write(project.join(".python-version"), "3.13.1").unwrap();
    fs::write(project.join("git-head"), "first").unwrap();
    fixture.tool("python", "printf 'Python %s\n' \"$(cat .python-version)\"");
    fixture.tool("git", "printf '# branch.head %s\n' \"$(cat git-head)\"");

    wait_until(|| {
        let line = fixture.render(project, &[]);
        line.contains("on first") && line.contains("py:3.13.1")
    });
    fs::write(project.join(".python-version"), "3.15.2").unwrap();
    fs::write(project.join("git-head"), "second").unwrap();
    let before_refresh = fixture.render(project, &["--after-command"]);
    assert!(before_refresh.contains("on first"));
    assert!(before_refresh.contains("py:3.13.1"));
    wait_until(|| {
        let line = fixture.render(project, &[]);
        line.contains("on second") && line.contains("py:3.15.2")
    });
}

#[test]
fn retains_the_last_successful_sample_when_probes_time_out() {
    let fixture = Fixture::new();
    let project = &fixture.project;
    fs::write(project.join(".python-version"), "3.13.1").unwrap();
    fixture.tool("python", "printf 'Python 3.13.1\n'");
    fixture.tool("python3", "exit 1");
    wait_until(|| fixture.render(project, &[]).contains("py:3.13.1"));

    let cache_root = fixture.root.path().join(".cache/dcx/prompt-v1");
    let sample = fs::read_dir(cache_root)
        .unwrap()
        .next()
        .unwrap()
        .unwrap()
        .path()
        .join("python");
    let previous = fs::metadata(&sample).unwrap().modified().unwrap();
    fixture.tool("python", "exec sleep 5");
    fs::write(project.join(".python-version"), "3.15.2").unwrap();

    assert!(
        fixture
            .render(project, &["--after-command"])
            .contains("py:3.13.1")
    );
    wait_until(|| fs::metadata(&sample).unwrap().modified().unwrap() > previous);
    assert!(fixture.render(project, &[]).contains("py:3.13.1"));
}

#[test]
fn shortens_repository_paths_after_discovery() {
    let fixture = Fixture::new();
    let project = &fixture.project;
    let result = Command::new("git")
        .args(["init", "-qb", "main"])
        .current_dir(project)
        .status()
        .unwrap();
    assert!(result.success());

    let nested = project.join("docs");
    fs::create_dir(&nested).unwrap();
    wait_until(|| fixture.render(project, &[]).starts_with("project on main "));
    wait_until(|| {
        fixture
            .render(&nested, &[])
            .starts_with("project/docs on main ")
    });
}

#[test]
fn refreshes_after_a_command_when_an_older_git_probe_is_in_flight() {
    let fixture = Fixture::new();
    let project = &fixture.project;
    fs::write(project.join("head"), "before").unwrap();
    fixture.tool(
        "git",
        r#"head=$(cat head)
if [ "$head" = before ]; then
    touch before-read
    while [ ! -f release ]; do sleep 0.01; done
else
    touch after-read
fi
printf '# branch.head %s\n' "$head"
"#,
    );

    fixture.render(project, &[]);
    wait_until(|| project.join("before-read").exists());
    fs::write(project.join("head"), "after").unwrap();
    fixture.render(project, &["--after-command"]);
    fs::write(project.join("release"), "").unwrap();

    wait_until(|| project.join("after-read").exists());
    wait_until(|| fixture.render(project, &[]).contains("on after"));
}
