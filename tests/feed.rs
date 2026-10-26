mod common;

use common::dcx;
use std::io::Write;
use std::process::Stdio;

#[test]
fn feed_help_exposes_file_and_stdin_input() {
    let root = tempfile::tempdir().unwrap();
    let output = dcx(&root.path().join("skills"))
        .args(["feed", "--help"])
        .output()
        .unwrap();
    assert!(output.status.success());
    let help = String::from_utf8(output.stdout).unwrap();
    assert!(help.contains("<PATH>"));
    assert!(help.contains("standard input"));
}

#[test]
fn feed_parses_stdin_then_requires_a_terminal() {
    let root = tempfile::tempdir().unwrap();
    let mut child = dcx(&root.path().join("skills"))
        .args(["feed", "-"])
        .stdin(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(include_bytes!("fixtures/sample.atom"))
        .unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("requires an interactive terminal"));
}

#[test]
fn feed_reports_missing_files_and_invalid_input() {
    let root = tempfile::tempdir().unwrap();
    let missing = dcx(&root.path().join("skills"))
        .arg("feed")
        .arg(root.path().join("missing.atom"))
        .output()
        .unwrap();
    assert!(!missing.status.success());
    assert!(String::from_utf8_lossy(&missing.stderr).contains("failed to open"));
    let path = root.path().join("invalid.atom");
    std::fs::write(&path, "invalid feed").unwrap();
    let invalid = dcx(&root.path().join("skills"))
        .arg("feed")
        .arg(path)
        .output()
        .unwrap();
    assert!(!invalid.status.success());
    assert!(String::from_utf8_lossy(&invalid.stderr).contains("failed to parse"));
}

#[test]
fn removed_cargo_subcommand_is_unavailable() {
    let root = tempfile::tempdir().unwrap();
    let output = dcx(&root.path().join("skills"))
        .args(["cargo", "cleanup", "--help"])
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("unrecognized subcommand 'cargo'"));
}
