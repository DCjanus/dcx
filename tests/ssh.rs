mod common;

use std::fs;
use std::io::Write;
use std::path::Path;
use std::process::{Command, Output, Stdio};

fn ssh_keygen(arguments: &[&str]) -> Output {
    let output = Command::new("ssh-keygen")
        .args(arguments)
        .stdin(Stdio::null())
        .output()
        .expect("SSH integration tests require ssh-keygen on PATH");
    assert!(
        output.status.success(),
        "ssh-keygen failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    output
}

fn generate_key(path: &Path, algorithm: &str, bits: &str, password: &str) {
    ssh_keygen(&[
        "-q",
        "-t",
        algorithm,
        "-b",
        bits,
        "-a",
        "1",
        "-N",
        password,
        "-C",
        "test comment",
        "-f",
        path.to_str().unwrap(),
    ]);
}

fn assert_public_key(output: &Output, expected: &[u8]) {
    assert!(
        output.status.success(),
        "dcx failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(output.stderr.is_empty());
    assert_eq!(output.stdout, expected);
}

#[test]
fn public_keys_match_ssh_keygen_without_external_commands_at_runtime() {
    let directory = tempfile::tempdir().unwrap();
    for (algorithm, bits) in [
        ("ed25519", "256"),
        ("rsa", "2048"),
        ("ecdsa", "256"),
        ("ecdsa", "384"),
        ("ecdsa", "521"),
    ] {
        for password in ["", "integration-test-passphrase"] {
            let case = directory.path().join(format!(
                "{algorithm}-{bits}-{}",
                if password.is_empty() {
                    "plain"
                } else {
                    "encrypted"
                }
            ));
            fs::create_dir(&case).unwrap();
            let path = case.join("private key");
            generate_key(&path, algorithm, bits, password);
            let reference = ssh_keygen(&["-y", "-P", password, "-f", path.to_str().unwrap()]);
            // ssh-keygen 版本可能保留注释；对照算法和 Base64 公钥数据。
            let reference = String::from_utf8(reference.stdout).unwrap();
            let fields: Vec<_> = reference.split_whitespace().collect();
            let expected = format!("{} {}\n", fields[0], fields[1]);
            let original = fs::read(&path).unwrap();
            // 删除旁路公钥，并清空 PATH，验证实际运行不依赖 .pub 或外部命令。
            fs::remove_file(path.with_extension("pub")).unwrap();
            let output = common::dcx(&case.join("skills"))
                .args(["ssh", "public-key"])
                .arg(&path)
                .env("PATH", "")
                .stdin(Stdio::null())
                .output()
                .unwrap();
            assert_public_key(&output, expected.as_bytes());
            assert_eq!(fs::read(&path).unwrap(), original);
            assert!(!path.with_extension("pub").exists());

            for argument in [None, Some("-")] {
                let mut command = common::dcx(&case.join("skills"));
                command.args(["ssh", "public-key"]);
                if let Some(argument) = argument {
                    command.arg(argument);
                }
                let mut child = command
                    .env("PATH", "")
                    .stdin(Stdio::piped())
                    .stdout(Stdio::piped())
                    .stderr(Stdio::piped())
                    .spawn()
                    .unwrap();
                child.stdin.take().unwrap().write_all(&original).unwrap();
                assert_public_key(&child.wait_with_output().unwrap(), expected.as_bytes());
            }
        }
    }
}

#[test]
fn invalid_or_unsupported_inputs_fail_without_echoing_input() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("key");
    for input in [
        "",
        "private-secret-invalid-input",
        "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAI invalid-public-key",
        "-----BEGIN RSA PRIVATE KEY-----\nprivate-secret\n-----END RSA PRIVATE KEY-----",
        "-----BEGIN OPENSSH PRIVATE KEY-----\nprivate-secret\n-----END OPENSSH PRIVATE KEY-----",
    ] {
        fs::write(&path, input).unwrap();
        let output = common::dcx(&directory.path().join("skills"))
            .args(["ssh", "public-key"])
            .arg(&path)
            .output()
            .unwrap();
        assert!(!output.status.success());
        assert!(output.stdout.is_empty());
        let error = String::from_utf8(output.stderr).unwrap();
        assert!(error.contains("invalid OpenSSH private key"), "{error}");
        assert!(!error.contains("private-secret"));
    }
}

#[test]
fn missing_file_fails_without_stdout() {
    let directory = tempfile::tempdir().unwrap();
    let output = common::dcx(&directory.path().join("skills"))
        .args(["ssh", "public-key"])
        .arg(directory.path().join("missing"))
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    assert!(
        String::from_utf8(output.stderr)
            .unwrap()
            .contains("failed to open")
    );
}
