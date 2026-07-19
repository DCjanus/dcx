mod common;

use std::fs;

use common::dtools;

#[test]
fn installs_repairs_and_uninstalls_the_bundled_skill() {
    let root = tempfile::tempdir().unwrap();
    let skills = root.path().join("skills");

    let missing = dtools(&skills).args(["skill", "status"]).output().unwrap();
    assert!(!missing.status.success());
    assert!(
        String::from_utf8(missing.stdout)
            .unwrap()
            .contains("status: missing")
    );

    let install = dtools(&skills).args(["skill", "install"]).output().unwrap();
    assert!(install.status.success());
    let installed = skills.join("dtools-cli");
    assert!(installed.join("SKILL.md").is_file());
    assert!(installed.join("agents/openai.yaml").is_file());
    assert!(installed.join(".dtools-managed").is_file());

    let current = dtools(&skills).args(["skill", "status"]).output().unwrap();
    assert!(current.status.success());
    assert!(
        String::from_utf8(current.stdout)
            .unwrap()
            .contains("status: current")
    );

    fs::remove_file(installed.join("agents/openai.yaml")).unwrap();
    let incomplete = dtools(&skills).args(["skill", "status"]).output().unwrap();
    assert!(!incomplete.status.success());
    assert!(
        String::from_utf8(incomplete.stdout)
            .unwrap()
            .contains("status: outdated")
    );
    let repair = dtools(&skills).args(["skill", "install"]).output().unwrap();
    assert!(repair.status.success());

    fs::write(installed.join("SKILL.md"), "modified\n").unwrap();
    let outdated = dtools(&skills).args(["skill", "status"]).output().unwrap();
    assert!(!outdated.status.success());
    assert!(
        String::from_utf8(outdated.stdout)
            .unwrap()
            .contains("status: outdated")
    );
    let help = dtools(&skills).arg("--help").output().unwrap();
    assert!(help.status.success());
    assert_eq!(
        fs::read_to_string(installed.join("SKILL.md")).unwrap(),
        "modified\n"
    );
    let input = root.path().join("input.txt");
    fs::write(&input, "pear\napple\n").unwrap();
    let functional_command = dtools(&skills).args(["sort"]).arg(&input).output().unwrap();
    assert!(functional_command.status.success());
    assert_ne!(
        fs::read_to_string(installed.join("SKILL.md")).unwrap(),
        "modified\n"
    );

    fs::write(installed.join("personal.txt"), "keep\n").unwrap();
    let uninstall = dtools(&skills)
        .args(["skill", "uninstall"])
        .output()
        .unwrap();
    assert!(uninstall.status.success());
    assert!(installed.join("personal.txt").is_file());
    assert!(!installed.join("SKILL.md").exists());
    assert!(!installed.join("agents/openai.yaml").exists());
    assert!(!installed.join(".dtools-managed").exists());
}

#[test]
fn force_install_backs_up_an_unmanaged_skill() {
    let root = tempfile::tempdir().unwrap();
    let skills = root.path().join("skills");
    let installed = skills.join("dtools-cli");
    fs::create_dir_all(&installed).unwrap();
    fs::write(installed.join("SKILL.md"), "personal\n").unwrap();

    let refused = dtools(&skills).args(["skill", "install"]).output().unwrap();
    assert!(!refused.status.success());
    assert_eq!(
        fs::read_to_string(installed.join("SKILL.md")).unwrap(),
        "personal\n"
    );

    let forced = dtools(&skills)
        .args(["skill", "install", "--force"])
        .output()
        .unwrap();
    assert!(forced.status.success());
    assert!(installed.join(".dtools-managed").is_file());
    let backup = fs::read_dir(&skills)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .find(|path| {
            path.file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with("dtools-cli.backup-")
        })
        .unwrap();
    assert_eq!(
        fs::read_to_string(backup.join("SKILL.md")).unwrap(),
        "personal\n"
    );
}

#[cfg(unix)]
#[test]
fn follows_a_symbolic_link_used_as_the_skills_root() {
    use std::os::unix::fs::symlink;

    let root = tempfile::tempdir().unwrap();
    let resolved_skills = root.path().join("resolved-skills");
    let logical_skills = root.path().join("logical-skills");
    fs::create_dir(&resolved_skills).unwrap();
    symlink(&resolved_skills, &logical_skills).unwrap();

    let install = dtools(&logical_skills)
        .args(["skill", "install"])
        .output()
        .unwrap();

    assert!(install.status.success());
    assert!(resolved_skills.join("dtools-cli/.dtools-managed").is_file());
    let stdout = String::from_utf8(install.stdout).unwrap();
    assert!(stdout.contains(&format!(
        "logical-path: {}",
        logical_skills.join("dtools-cli").display()
    )));
    assert!(stdout.contains(&format!(
        "resolved-path: {}",
        fs::canonicalize(&resolved_skills)
            .unwrap()
            .join("dtools-cli")
            .display()
    )));
}

#[cfg(unix)]
#[test]
fn refuses_to_follow_a_symbolic_link_for_a_managed_file() {
    use std::os::unix::fs::symlink;

    let root = tempfile::tempdir().unwrap();
    let skills = root.path().join("skills");
    let install = dtools(&skills).args(["skill", "install"]).output().unwrap();
    assert!(install.status.success());
    let installed_file = skills.join("dtools-cli/SKILL.md");
    let outside = root.path().join("outside.md");
    fs::write(&outside, "outside\n").unwrap();
    fs::remove_file(&installed_file).unwrap();
    symlink(&outside, &installed_file).unwrap();
    let input = root.path().join("input.txt");
    fs::write(&input, "pear\napple\n").unwrap();

    let output = dtools(&skills).args(["sort"]).arg(&input).output().unwrap();

    assert!(output.status.success());
    assert!(
        String::from_utf8(output.stderr)
            .unwrap()
            .contains("warning:")
    );
    assert_eq!(fs::read_to_string(outside).unwrap(), "outside\n");
}

#[test]
fn rejects_a_relative_skills_directory() {
    let output = dtools(std::path::Path::new("relative-skills"))
        .args(["skill", "install"])
        .output()
        .unwrap();

    assert!(!output.status.success());
    assert!(
        String::from_utf8(output.stderr)
            .unwrap()
            .contains("DTOOLS_SKILLS_DIR must be an absolute path")
    );
}
