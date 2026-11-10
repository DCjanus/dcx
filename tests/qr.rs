mod common;

use std::io::Write;
use std::process::Stdio;

#[test]
fn stdin_is_explicit_and_conflicts_with_text() {
    let directory = tempfile::tempdir().unwrap();
    for arguments in [vec!["qr"], vec!["qr", "hello", "--stdin"]] {
        let output = common::dcx(directory.path())
            .args(arguments)
            .output()
            .unwrap();
        assert_eq!(output.status.code(), Some(2));
        assert!(output.stdout.is_empty());
    }
}

#[test]
fn png_roundtrips_unicode_and_whitespace_from_stdin() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("qr.png");
    let text = " 你好 https://example.com\nsecond line\n";
    let mut child = common::dcx(directory.path())
        .args(["qr", "--stdin", "-o"])
        .arg(&path)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(text.as_bytes())
        .unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(output.status.success(), "{:?}", output);
    assert!(output.stdout.is_empty());
    assert!(output.stderr.is_empty());
    let image = image::open(path).unwrap().to_luma8();
    let mut prepared = rqrr::PreparedImage::prepare(image);
    let grids = prepared.detect_grids();
    assert_eq!(grids.len(), 1);
    assert_eq!(grids[0].decode().unwrap().1, text);
}

#[test]
fn svg_file_and_plain_terminal_output() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("qr.svg");
    let output = common::dcx(directory.path())
        .args(["qr", "hello", "-o"])
        .arg(&path)
        .output()
        .unwrap();
    assert!(output.status.success());
    assert!(output.stdout.is_empty());
    let svg = std::fs::read_to_string(path).unwrap();
    assert!(svg.contains("<svg"));
    assert!(svg.contains("</svg>"));
    let output = common::dcx(directory.path())
        .args(["qr", "hello"])
        .output()
        .unwrap();
    assert!(output.status.success());
    let text = String::from_utf8(output.stdout).unwrap();
    assert!(text.contains('█'));
    assert!(!text.contains('\x1b'));
}

#[test]
fn rejects_empty_oversized_and_unsupported_output_without_creating_files() {
    let directory = tempfile::tempdir().unwrap();
    for (text, filename, message) in [
        ("".to_owned(), "empty.png", "must not be empty"),
        ("x".repeat(10000), "large.png", "failed to encode QR code"),
        ("hello".to_owned(), "qr.jpg", ".png or .svg"),
    ] {
        let path = directory.path().join(filename);
        let output = common::dcx(directory.path())
            .args(["qr", &text, "-o"])
            .arg(&path)
            .output()
            .unwrap();
        assert!(!output.status.success());
        assert!(String::from_utf8_lossy(&output.stderr).contains(message));
        assert!(!path.exists());
    }
}
