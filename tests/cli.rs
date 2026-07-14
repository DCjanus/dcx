use std::fs;
use std::io::Write;
use std::process::{Command, Stdio};

fn dtools() -> Command {
    Command::new(env!("CARGO_BIN_EXE_dtools"))
}

#[test]
fn sort_subcommand_sorts_trims_and_deduplicates() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("input.txt");
    fs::write(&path, " banana  \npear\n  apple\nbanana\n").unwrap();

    let status = dtools()
        .args(["sort", "--uniq", "--trim-whitespace"])
        .arg(&path)
        .status()
        .unwrap();

    assert!(status.success());
    assert_eq!(fs::read_to_string(path).unwrap(), "apple\nbanana\npear\n");
}

#[test]
fn uniq_subcommand_deduplicates_in_first_seen_order() {
    let mut child = dtools()
        .arg("uniq")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(b"pear\napple\npear\n")
        .unwrap();

    let output = child.wait_with_output().unwrap();

    assert!(output.status.success());
    assert_eq!(String::from_utf8(output.stdout).unwrap(), "pear\napple\n");
}
