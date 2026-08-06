use std::fs;
use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};

const PUBLIC_JWT: &str = concat!(
    "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.",
    "eyJhdWQiOlsiYXBpIiwid2ViIl0sImV4cCI6MTg5MzQ1NjAwMCwiaWF0IjoxODkzNDUyNDAwLCJpc3MiOiJodHRwczovL2lzc3Vlci5leGFtcGxlIiwic3ViIjoidXNlci0xMjMifQ.",
    "c2lnbmF0dXJl"
);

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
fn jwt_subcommand_decodes_header_claims_and_numeric_dates() {
    let directory = tempfile::tempdir().unwrap();
    let token_path = directory.path().join("token");
    fs::write(&token_path, PUBLIC_JWT).unwrap();

    let output = dcx(&directory.path().join("skills"))
        .args(["jwt", "inspect"])
        .arg(&token_path)
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("Signature"));
    assert!(stdout.contains("NOT VERIFIED"));
    assert!(stdout.contains("Algorithm"));
    assert!(stdout.contains("HS256"));
    assert!(stdout.contains("Issuer"));
    assert!(stdout.contains("https://issuer.example"));
    assert!(stdout.contains("Audience"));
    assert!(stdout.contains("api, web"));
    assert!(stdout.contains("Issued at"));
    assert!(stdout.contains("2029-12-31T23:00:00Z"));
    assert!(stdout.contains("Expires at"));
    assert!(stdout.contains("2030-01-01T00:00:00Z"));
    assert!(!stdout.contains("c2lnbmF0dXJl"));
}

#[test]
fn jwt_subcommand_reads_from_standard_input() {
    let directory = tempfile::tempdir().unwrap();
    let mut child = dcx(&directory.path().join("skills"))
        .args(["jwt", "inspect"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(PUBLIC_JWT.as_bytes())
        .unwrap();

    let output = child.wait_with_output().unwrap();

    assert!(output.status.success());
    assert!(
        String::from_utf8(output.stdout)
            .unwrap()
            .contains("https://issuer.example")
    );
}

#[test]
fn jwt_subcommand_does_not_echo_a_token_passed_as_a_path() {
    let directory = tempfile::tempdir().unwrap();
    let output = dcx(&directory.path().join("skills"))
        .args(["jwt", "inspect", PUBLIC_JWT])
        .output()
        .unwrap();

    assert!(!output.status.success());
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(stderr.contains("use standard input"));
    assert!(!stderr.contains(PUBLIC_JWT));
}

#[test]
fn jwt_subcommand_rejects_malformed_input() {
    let directory = tempfile::tempdir().unwrap();
    let mut child = dcx(&directory.path().join("skills"))
        .args(["jwt", "inspect"])
        .stdin(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child.stdin.take().unwrap().write_all(b"not-a-jwt").unwrap();

    let output = child.wait_with_output().unwrap();

    assert!(!output.status.success());
    assert!(
        String::from_utf8(output.stderr)
            .unwrap()
            .contains("expected 3 dot-separated segments")
    );
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
