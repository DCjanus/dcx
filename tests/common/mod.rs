#![allow(dead_code)]

use std::path::Path;
use std::process::{Command, Output};

pub fn dtools(skills_directory: &Path) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_dtools"));
    command.env("DTOOLS_SKILLS_DIR", skills_directory);
    command
}

pub fn git(directory: &Path, arguments: &[&str]) -> Output {
    let output = Command::new("git")
        .arg("-C")
        .arg(directory)
        .args(arguments)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "git {arguments:?} failed:\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    output
}

pub fn init_repository(directory: &Path) {
    git(directory, &["init", "--quiet", "--initial-branch=main"]);
    git(directory, &["config", "user.name", "Test User"]);
    git(directory, &["config", "user.email", "test@example.com"]);
    std::fs::write(directory.join("file.txt"), "base\n").unwrap();
    git(directory, &["add", "file.txt"]);
    git(directory, &["commit", "--quiet", "-m", "base"]);
}
