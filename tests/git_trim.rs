mod common;

use std::fs;

use common::{dtools, git, init_repository};

fn setup_remote_repository() -> (tempfile::TempDir, std::path::PathBuf) {
    let root = tempfile::tempdir().unwrap();
    let repository = root.path().join("repository");
    let remote = root.path().join("remote.git");
    fs::create_dir(&repository).unwrap();
    fs::create_dir(&remote).unwrap();
    init_repository(&repository);
    git(&remote, &["init", "--quiet", "--bare"]);
    git(
        &repository,
        &["remote", "add", "origin", remote.to_str().unwrap()],
    );
    git(&repository, &["push", "--quiet", "-u", "origin", "main"]);
    git(&remote, &["symbolic-ref", "HEAD", "refs/heads/main"]);
    git(&repository, &["remote", "set-head", "origin", "-a"]);
    (root, repository)
}

#[test]
fn keeps_a_newly_pushed_branch_while_its_upstream_exists() {
    let (root, repository) = setup_remote_repository();
    let skills = root.path().join("skills");
    git(&repository, &["switch", "--quiet", "-c", "feature"]);
    git(&repository, &["push", "--quiet", "-u", "origin", "feature"]);
    git(&repository, &["switch", "--quiet", "main"]);

    let output = dtools(&skills)
        .current_dir(&repository)
        .args(["git", "trim", "--dry-run"])
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("SKIP   feature"));
    assert!(stdout.contains("upstream exists"));
    assert!(
        String::from_utf8(git(&repository, &["branch", "--list", "feature"]).stdout)
            .unwrap()
            .contains("feature")
    );
}

#[test]
fn deletes_a_squash_merged_branch_after_its_upstream_is_pruned() {
    let (root, repository) = setup_remote_repository();
    let skills = root.path().join("skills");
    git(&repository, &["switch", "--quiet", "-c", "feature"]);
    fs::write(repository.join("feature.txt"), "one\ntwo\n").unwrap();
    git(&repository, &["add", "feature.txt"]);
    git(&repository, &["commit", "--quiet", "-m", "feature one"]);
    fs::write(repository.join("feature.txt"), "one\ntwo\nthree\n").unwrap();
    git(&repository, &["commit", "--quiet", "-am", "feature two"]);
    git(&repository, &["push", "--quiet", "-u", "origin", "feature"]);
    git(&repository, &["switch", "--quiet", "main"]);
    git(&repository, &["merge", "--quiet", "--squash", "feature"]);
    git(&repository, &["commit", "--quiet", "-m", "squash feature"]);
    git(&repository, &["push", "--quiet", "origin", "main"]);
    git(
        &repository,
        &["push", "--quiet", "origin", "--delete", "feature"],
    );
    git(&repository, &["fetch", "--quiet", "--prune", "origin"]);

    let preview = dtools(&skills)
        .current_dir(&repository)
        .args(["git", "trim", "--dry-run"])
        .output()
        .unwrap();
    assert!(preview.status.success());
    let stdout = String::from_utf8(preview.stdout).unwrap();
    assert!(stdout.contains("DELETE feature"));
    assert!(stdout.contains("changes already present in origin/main"));

    let deletion = dtools(&skills)
        .current_dir(&repository)
        .args(["git", "trim", "--yes"])
        .output()
        .unwrap();
    assert!(deletion.status.success());
    assert!(
        String::from_utf8(git(&repository, &["branch", "--list", "feature"]).stdout)
            .unwrap()
            .trim()
            .is_empty()
    );
}

#[test]
fn deletes_a_squash_merged_branch_after_base_changes_the_same_file() {
    let (root, repository) = setup_remote_repository();
    let skills = root.path().join("skills");
    git(&repository, &["switch", "--quiet", "-c", "feature"]);
    fs::write(repository.join("feature.txt"), "merged\n").unwrap();
    git(&repository, &["add", "feature.txt"]);
    git(&repository, &["commit", "--quiet", "-m", "feature"]);
    git(&repository, &["push", "--quiet", "-u", "origin", "feature"]);
    git(&repository, &["switch", "--quiet", "main"]);
    git(&repository, &["merge", "--quiet", "--squash", "feature"]);
    git(&repository, &["commit", "--quiet", "-m", "squash feature"]);
    fs::write(repository.join("feature.txt"), "changed later\n").unwrap();
    git(
        &repository,
        &["commit", "--quiet", "-am", "change feature later"],
    );
    git(&repository, &["push", "--quiet", "origin", "main"]);
    git(
        &repository,
        &["push", "--quiet", "origin", "--delete", "feature"],
    );
    git(&repository, &["fetch", "--quiet", "--prune", "origin"]);
    git(
        &repository,
        &["config", "diff.external", "missing-dtools-test-command"],
    );

    let output = dtools(&skills)
        .current_dir(&repository)
        .args(["git", "trim", "--dry-run"])
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("DELETE feature"));
    assert!(stdout.contains("changes already present in origin/main"));
}

#[test]
fn keeps_a_gone_branch_that_still_has_unique_changes() {
    let (root, repository) = setup_remote_repository();
    let skills = root.path().join("skills");
    git(&repository, &["switch", "--quiet", "-c", "feature"]);
    fs::write(repository.join("feature.txt"), "not merged\n").unwrap();
    git(&repository, &["add", "feature.txt"]);
    git(&repository, &["commit", "--quiet", "-m", "feature"]);
    git(&repository, &["push", "--quiet", "-u", "origin", "feature"]);
    git(&repository, &["switch", "--quiet", "main"]);
    git(
        &repository,
        &["push", "--quiet", "origin", "--delete", "feature"],
    );
    git(&repository, &["fetch", "--quiet", "--prune", "origin"]);

    let output = dtools(&skills)
        .current_dir(&repository)
        .args(["git", "trim", "--dry-run"])
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("KEEP   feature"));
    assert!(stdout.contains("unique changes remain"));
}

#[test]
fn keeps_a_patch_equivalent_branch_when_the_historical_tree_differs() {
    let (root, repository) = setup_remote_repository();
    let skills = root.path().join("skills");
    git(&repository, &["switch", "--quiet", "-c", "feature"]);
    fs::write(repository.join("feature.txt"), "value \n").unwrap();
    git(&repository, &["add", "feature.txt"]);
    git(&repository, &["commit", "--quiet", "-m", "feature"]);
    git(&repository, &["push", "--quiet", "-u", "origin", "feature"]);
    git(&repository, &["switch", "--quiet", "main"]);
    fs::write(repository.join("feature.txt"), "value\n").unwrap();
    git(&repository, &["add", "feature.txt"]);
    git(
        &repository,
        &["commit", "--quiet", "-m", "similar change on main"],
    );
    git(&repository, &["push", "--quiet", "origin", "main"]);
    git(
        &repository,
        &["push", "--quiet", "origin", "--delete", "feature"],
    );
    git(&repository, &["fetch", "--quiet", "--prune", "origin"]);

    let output = dtools(&skills)
        .current_dir(&repository)
        .args(["git", "trim", "--dry-run"])
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("KEEP   feature"));
    assert!(stdout.contains("unique changes remain"));
}

#[test]
fn manages_repository_local_exclude_patterns() {
    let root = tempfile::tempdir().unwrap();
    let repository = root.path().join("repository");
    let skills = root.path().join("skills");
    fs::create_dir(&repository).unwrap();
    init_repository(&repository);

    let add = dtools(&skills)
        .current_dir(&repository)
        .args(["git", "trim", "exclude", "add", "release/*", "develop"])
        .output()
        .unwrap();
    assert!(add.status.success());

    let list = dtools(&skills)
        .current_dir(&repository)
        .args(["git", "trim", "exclude", "list"])
        .output()
        .unwrap();
    assert!(list.status.success());
    assert_eq!(
        String::from_utf8(list.stdout).unwrap(),
        "develop\nrelease/*\n"
    );
    assert_eq!(
        fs::read_to_string(repository.join(".git/dtools/trim-exclude")).unwrap(),
        "develop\nrelease/*\n"
    );

    let remove = dtools(&skills)
        .current_dir(&repository)
        .args(["git", "trim", "exclude", "remove", "develop"])
        .output()
        .unwrap();
    assert!(remove.status.success());
    assert_eq!(
        fs::read_to_string(repository.join(".git/dtools/trim-exclude")).unwrap(),
        "release/*\n"
    );
}

#[test]
fn deletes_a_regularly_merged_branch_with_safe_git_deletion() {
    let (root, repository) = setup_remote_repository();
    let skills = root.path().join("skills");
    git(&repository, &["switch", "--quiet", "-c", "feature"]);
    fs::write(repository.join("feature.txt"), "merged\n").unwrap();
    git(&repository, &["add", "feature.txt"]);
    git(&repository, &["commit", "--quiet", "-m", "feature"]);
    git(&repository, &["push", "--quiet", "-u", "origin", "feature"]);
    git(&repository, &["switch", "--quiet", "main"]);
    git(&repository, &["merge", "--quiet", "--no-ff", "feature"]);
    git(&repository, &["push", "--quiet", "origin", "main"]);
    git(
        &repository,
        &["push", "--quiet", "origin", "--delete", "feature"],
    );
    git(&repository, &["fetch", "--quiet", "--prune", "origin"]);

    let output = dtools(&skills)
        .current_dir(&repository)
        .args(["git", "trim", "--dry-run"])
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("DELETE feature"));
    assert!(stdout.contains("merged into origin/main"));
}

#[test]
fn respects_glob_excludes_when_a_gone_branch_is_otherwise_deletable() {
    let (root, repository) = setup_remote_repository();
    let skills = root.path().join("skills");
    git(&repository, &["switch", "--quiet", "-c", "release/feature"]);
    fs::write(repository.join("feature.txt"), "merged\n").unwrap();
    git(&repository, &["add", "feature.txt"]);
    git(&repository, &["commit", "--quiet", "-m", "feature"]);
    git(
        &repository,
        &["push", "--quiet", "-u", "origin", "release/feature"],
    );
    git(&repository, &["switch", "--quiet", "main"]);
    git(
        &repository,
        &["merge", "--quiet", "--no-ff", "release/feature"],
    );
    git(&repository, &["push", "--quiet", "origin", "main"]);
    git(
        &repository,
        &["push", "--quiet", "origin", "--delete", "release/feature"],
    );
    git(&repository, &["fetch", "--quiet", "--prune", "origin"]);
    let add = dtools(&skills)
        .current_dir(&repository)
        .args(["git", "trim", "exclude", "add", "release/*"])
        .output()
        .unwrap();
    assert!(add.status.success());

    let output = dtools(&skills)
        .current_dir(&repository)
        .args(["git", "trim", "--dry-run"])
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("SKIP   release/feature"));
    assert!(stdout.contains("excluded by rule"));
}

#[test]
fn protects_a_branch_checked_out_in_a_linked_worktree() {
    let (root, repository) = setup_remote_repository();
    let skills = root.path().join("skills");
    let worktree = root.path().join("feature-worktree");
    git(&repository, &["switch", "--quiet", "-c", "feature"]);
    git(&repository, &["push", "--quiet", "-u", "origin", "feature"]);
    git(&repository, &["switch", "--quiet", "main"]);
    git(
        &repository,
        &["push", "--quiet", "origin", "--delete", "feature"],
    );
    git(&repository, &["fetch", "--quiet", "--prune", "origin"]);
    git(
        &repository,
        &[
            "worktree",
            "add",
            "--quiet",
            worktree.to_str().unwrap(),
            "feature",
        ],
    );

    let output = dtools(&skills)
        .current_dir(&repository)
        .args(["git", "trim", "--dry-run"])
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("SKIP   feature"));
    assert!(stdout.contains("checked out in a worktree"));
}

#[test]
fn recognizes_patch_equivalent_changes_after_a_rebase_style_merge() {
    let (root, repository) = setup_remote_repository();
    let skills = root.path().join("skills");
    git(&repository, &["switch", "--quiet", "-c", "feature"]);
    fs::write(repository.join("feature.txt"), "rebased\n").unwrap();
    git(&repository, &["add", "feature.txt"]);
    git(&repository, &["commit", "--quiet", "-m", "feature"]);
    let feature_commit =
        String::from_utf8(git(&repository, &["rev-parse", "feature"]).stdout).unwrap();
    git(&repository, &["push", "--quiet", "-u", "origin", "feature"]);
    git(&repository, &["switch", "--quiet", "main"]);
    fs::write(repository.join("main-only.txt"), "main\n").unwrap();
    git(&repository, &["add", "main-only.txt"]);
    git(&repository, &["commit", "--quiet", "-m", "advance main"]);
    git(
        &repository,
        &["cherry-pick", "--quiet", feature_commit.trim()],
    );
    git(&repository, &["push", "--quiet", "origin", "main"]);
    git(
        &repository,
        &["push", "--quiet", "origin", "--delete", "feature"],
    );
    git(&repository, &["fetch", "--quiet", "--prune", "origin"]);

    let output = dtools(&skills)
        .current_dir(&repository)
        .args(["git", "trim", "--dry-run"])
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("DELETE feature"));
    assert!(stdout.contains("changes already present in origin/main"));
}

#[test]
fn preserves_local_commits_added_after_the_remote_branch_was_merged() {
    let (root, repository) = setup_remote_repository();
    let skills = root.path().join("skills");
    git(&repository, &["switch", "--quiet", "-c", "feature"]);
    fs::write(repository.join("feature.txt"), "merged\n").unwrap();
    git(&repository, &["add", "feature.txt"]);
    git(&repository, &["commit", "--quiet", "-m", "merged feature"]);
    git(&repository, &["push", "--quiet", "-u", "origin", "feature"]);
    git(&repository, &["switch", "--quiet", "main"]);
    git(&repository, &["merge", "--quiet", "--squash", "feature"]);
    git(&repository, &["commit", "--quiet", "-m", "squash feature"]);
    git(&repository, &["push", "--quiet", "origin", "main"]);
    git(&repository, &["switch", "--quiet", "feature"]);
    fs::write(repository.join("local-only.txt"), "keep\n").unwrap();
    git(&repository, &["add", "local-only.txt"]);
    git(
        &repository,
        &["commit", "--quiet", "-m", "local-only change"],
    );
    git(&repository, &["switch", "--quiet", "main"]);
    git(
        &repository,
        &["push", "--quiet", "origin", "--delete", "feature"],
    );
    git(&repository, &["fetch", "--quiet", "--prune", "origin"]);

    let output = dtools(&skills)
        .current_dir(&repository)
        .args(["git", "trim", "--dry-run"])
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("KEEP   feature"));
    assert!(stdout.contains("unique changes remain"));
}
