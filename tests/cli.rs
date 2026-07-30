use std::fs;
use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};

fn dcx(skills_directory: &Path) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_dcx"));
    command.env("DCX_SKILLS_DIR", skills_directory);
    command
}

#[test]
fn sort_subcommand_sorts_trims_and_deduplicates() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("input.txt");
    fs::write(&path, " banana  \npear\n  apple\nbanana\n").unwrap();

    let status = dcx(&directory.path().join("skills"))
        .args(["sort", "--uniq", "--trim-whitespace"])
        .arg(&path)
        .status()
        .unwrap();

    assert!(status.success());
    assert_eq!(fs::read_to_string(path).unwrap(), "apple\nbanana\npear\n");
}

#[test]
fn uniq_subcommand_deduplicates_in_first_seen_order() {
    let directory = tempfile::tempdir().unwrap();
    let mut child = dcx(&directory.path().join("skills"))
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

#[test]
fn installs_a_dynamic_bash_completion_registration() {
    let directory = tempfile::tempdir().unwrap();
    let data_home = directory.path().join("data");

    let output = dcx(&directory.path().join("skills"))
        .env("HOME", directory.path())
        .env("XDG_DATA_HOME", &data_home)
        .args(["--install-completion", "bash"])
        .output()
        .unwrap();

    assert!(output.status.success());
    let destination = data_home.join("bash-completion/completions/dcx");
    let script = fs::read_to_string(&destination).unwrap();
    assert!(script.contains("COMPLETE=\"bash\""));
    assert!(script.contains("builtin type -P dcx"));
    assert!(
        String::from_utf8(output.stdout)
            .unwrap()
            .starts_with("installed bash completion to ")
    );
}

#[test]
fn prints_dynamic_completion_registration_from_the_environment_protocol() {
    let directory = tempfile::tempdir().unwrap();
    let output = dcx(&directory.path().join("skills"))
        .env("COMPLETE", "bash")
        .output()
        .unwrap();

    assert!(output.status.success());
    let registration = String::from_utf8(output.stdout).unwrap();
    assert!(registration.contains("COMPLETE=\"bash\""));
    assert!(registration.contains("dcx"));
}
