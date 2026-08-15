mod common;

use std::fs;

use common::dcx;

#[test]
fn cleanup_removes_stale_cache_groups_but_keeps_final_binary() {
    let root = tempfile::tempdir().unwrap();
    let project = root.path().join("project");
    let skills = root.path().join("skills");
    let profile = project.join("target/debug");
    let fingerprint = profile
        .join(".fingerprint")
        .join("old-crate-0123456789abcdef");
    let dependency = profile.join("deps/libold_crate-0123456789abcdef.rlib");
    let incremental = profile.join("incremental/old_crate-0123456789abcdef");
    let final_binary = profile.join("application");
    fs::create_dir_all(project.join("src")).unwrap();
    fs::write(
        project.join("Cargo.toml"),
        "[package]\nname = \"fixture\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
    )
    .unwrap();
    fs::write(project.join("src/main.rs"), "fn main() {}\n").unwrap();
    fs::create_dir_all(&fingerprint).unwrap();
    fs::write(fingerprint.join("bin-old_crate.json"), r#"{"rustc":42}"#).unwrap();
    fs::create_dir_all(dependency.parent().unwrap()).unwrap();
    fs::write(&dependency, b"dependency cache").unwrap();
    fs::create_dir_all(&incremental).unwrap();
    fs::write(incremental.join("cache.bin"), b"incremental cache").unwrap();
    fs::write(&final_binary, b"final binary").unwrap();

    let output = dcx(&skills)
        .current_dir(&project)
        .args(["cargo", "cleanup", "--days", "0", "--yes"])
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(!fingerprint.exists());
    assert!(!dependency.exists());
    assert!(!incremental.exists());
    assert!(final_binary.exists());
    assert!(
        String::from_utf8(output.stdout)
            .unwrap()
            .contains("Removed 3 cache paths")
    );
}

#[test]
fn cleanup_dry_run_reports_without_removing_cache() {
    let root = tempfile::tempdir().unwrap();
    let project = root.path().join("project");
    let skills = root.path().join("skills");
    let fingerprint = project.join("target/debug/.fingerprint/old-crate-0123456789abcdef");
    fs::create_dir_all(project.join("src")).unwrap();
    fs::write(
        project.join("Cargo.toml"),
        "[package]\nname = \"fixture\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
    )
    .unwrap();
    fs::write(project.join("src/main.rs"), "fn main() {}\n").unwrap();
    fs::create_dir_all(&fingerprint).unwrap();
    fs::write(fingerprint.join("bin-old_crate.json"), r#"{"rustc":42}"#).unwrap();

    let output = dcx(&skills)
        .current_dir(&project)
        .args(["cargo", "cleanup", "--dry-run"])
        .output()
        .unwrap();

    assert!(output.status.success());
    assert!(fingerprint.exists());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("toolchain"));
    assert!(stdout.contains("no longer installed"));
}
