use std::collections::{BTreeSet, HashSet};
use std::fs;
use std::io::{self, ErrorKind, IsTerminal, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus, Output, Stdio};

use anyhow::{Context, bail};
use globset::{Glob, GlobSet, GlobSetBuilder};
use tempfile::NamedTempFile;

use crate::AnyResult;

const EXCLUDE_FILE: &str = "dcx/trim-exclude";

#[derive(Debug)]
struct Branch {
    name: String,
    object: String,
    upstream: String,
    tracking: String,
    worktree: String,
}

#[derive(Debug, Clone, Copy)]
enum DeleteKind {
    Merged,
    Equivalent,
}

#[derive(Debug)]
struct DeleteCandidate {
    name: String,
    kind: DeleteKind,
}

#[derive(Debug, Default)]
struct ExcludeConfig {
    comments: Vec<String>,
    patterns: BTreeSet<String>,
}

pub fn trim(
    bases: &[String],
    command_line_excludes: &[String],
    dry_run: bool,
    update: bool,
    yes: bool,
) -> AnyResult {
    ensure_repository()?;
    if update {
        git_checked(&["fetch", "--all", "--prune"])?;
    }

    let bases = resolve_bases(bases)?;
    let base_local_names = base_local_names(&bases)?;
    let mut patterns = load_exclude_config()?.patterns;
    for pattern in command_line_excludes {
        validate_pattern(pattern)?;
        patterns.insert(pattern.to_owned());
    }
    let excludes = build_glob_set(&patterns)?;
    let branches = local_branches()?;
    let mut candidates = Vec::new();

    for branch in branches {
        if base_local_names.contains(&branch.name) {
            print_status("SKIP", &branch.name, "base branch");
            continue;
        }
        if !branch.worktree.is_empty() {
            print_status("SKIP", &branch.name, "checked out in a worktree");
            continue;
        }
        if excludes.is_match(&branch.name) {
            print_status("SKIP", &branch.name, "excluded by rule");
            continue;
        }
        if branch.upstream.is_empty() {
            print_status("SKIP", &branch.name, "no upstream");
            continue;
        }
        if branch.tracking != "[gone]" {
            print_status("SKIP", &branch.name, "upstream exists");
            continue;
        }

        let mut absorbed = None;
        for base in &bases {
            if let Some(kind) = changes_absorbed(&branch, base)? {
                absorbed = Some((kind, base));
                break;
            }
        }
        if let Some((kind, base)) = absorbed {
            let reason = match kind {
                DeleteKind::Merged => format!("merged into {base}"),
                DeleteKind::Equivalent => format!("changes already present in {base}"),
            };
            print_status("DELETE", &branch.name, &reason);
            candidates.push(DeleteCandidate {
                name: branch.name,
                kind,
            });
        } else {
            print_status(
                "KEEP",
                &branch.name,
                "upstream gone but unique changes remain",
            );
        }
    }

    if dry_run || candidates.is_empty() {
        return Ok(());
    }
    if !yes {
        if !io::stdin().is_terminal() {
            bail!("confirmation requires a terminal; use --yes to delete listed branches");
        }
        print!("Delete {} local branches? [y/N] ", candidates.len());
        io::stdout()
            .flush()
            .context("failed to flush confirmation")?;
        let mut answer = String::new();
        io::stdin()
            .read_line(&mut answer)
            .context("failed to read confirmation")?;
        if !matches!(answer.trim(), "y" | "Y" | "yes" | "YES") {
            return Ok(());
        }
    }

    for candidate in candidates {
        let flag = match candidate.kind {
            DeleteKind::Merged => "-d",
            DeleteKind::Equivalent => "-D",
        };
        git_checked(&["branch", flag, "--", &candidate.name])?;
    }
    Ok(())
}

pub fn exclude_add(patterns: &[String], current: bool) -> AnyResult {
    ensure_repository()?;
    let mut config = load_exclude_config()?;
    for pattern in patterns {
        validate_pattern(pattern)?;
        config.patterns.insert(pattern.to_owned());
    }
    if current {
        let branch = git_stdout(&["symbolic-ref", "--quiet", "--short", "HEAD"])
            .context("HEAD is detached; --current requires a checked-out branch")?;
        validate_pattern(&branch)?;
        config.patterns.insert(branch);
    }
    if patterns.is_empty() && !current {
        bail!("provide at least one pattern or use --current");
    }
    write_exclude_config(&config)
}

pub fn exclude_remove(patterns: &[String]) -> AnyResult {
    ensure_repository()?;
    let mut config = load_exclude_config()?;
    for pattern in patterns {
        config.patterns.remove(pattern);
    }
    write_exclude_config(&config)
}

pub fn exclude_list() -> AnyResult {
    ensure_repository()?;
    for pattern in load_exclude_config()?.patterns {
        println!("{pattern}");
    }
    Ok(())
}

fn ensure_repository() -> AnyResult {
    git_stdout(&["rev-parse", "--git-dir"])
        .context("not inside a Git repository")
        .map(|_| ())
}

fn resolve_bases(requested: &[String]) -> AnyResult<Vec<String>> {
    let bases = if requested.is_empty() {
        git_stdout(&["for-each-ref", "--format=%(symref:short)", "refs/remotes"])?
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty())
            .map(str::to_owned)
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect::<Vec<_>>()
    } else {
        requested.to_vec()
    };
    if bases.is_empty() {
        bail!("failed to determine a base branch; pass --base explicitly");
    }
    for base in &bases {
        rev_parse_commit(base).with_context(|| format!("invalid base branch {base}"))?;
    }
    Ok(bases)
}

fn base_local_names(bases: &[String]) -> AnyResult<HashSet<String>> {
    let mut names = HashSet::new();
    for base in bases {
        let full_name = git_stdout(&["rev-parse", "--symbolic-full-name", base])?;
        if let Some(name) = full_name.strip_prefix("refs/heads/") {
            names.insert(name.to_owned());
        } else if let Some(remote_name) = full_name.strip_prefix("refs/remotes/") {
            if let Some((_, name)) = remote_name.split_once('/') {
                names.insert(name.to_owned());
            }
        }
    }
    Ok(names)
}

fn local_branches() -> AnyResult<Vec<Branch>> {
    let output = git_stdout(&[
        "for-each-ref",
        "--format=%(refname:short)%00%(objectname)%00%(upstream:short)%00%(upstream:track)%00%(worktreepath)",
        "refs/heads",
    ])?;
    output
        .lines()
        .map(|line| {
            let fields = line.split('\0').collect::<Vec<_>>();
            if fields.len() != 5 {
                bail!("unexpected git for-each-ref output");
            }
            Ok(Branch {
                name: fields[0].to_owned(),
                object: fields[1].to_owned(),
                upstream: fields[2].to_owned(),
                tracking: fields[3].to_owned(),
                worktree: fields[4].to_owned(),
            })
        })
        .collect()
}

fn changes_absorbed(branch: &Branch, base: &str) -> AnyResult<Option<DeleteKind>> {
    let base_object = rev_parse_commit(base)?;
    let ancestor = git_status(&["merge-base", "--is-ancestor", &branch.object, &base_object])?;
    if ancestor.success() {
        return Ok(Some(DeleteKind::Merged));
    }
    if ancestor.code() != Some(1) {
        bail!("git merge-base failed for {} and {base}", branch.name);
    }

    if let Some(merged_tree) = merge_tree(&base_object, &branch.object)? {
        let base_tree = revision_tree(&base_object)?;
        if merged_tree == base_tree {
            return Ok(Some(DeleteKind::Equivalent));
        }
    }

    if changes_historically_absorbed(branch, &base_object)? {
        return Ok(Some(DeleteKind::Equivalent));
    }

    Ok(None)
}

fn changes_historically_absorbed(branch: &Branch, base_object: &str) -> AnyResult<bool> {
    let merge_base = git_stdout(&["merge-base", &branch.object, base_object])?;
    let branch_patch_ids = git_patch_ids(&[
        "-c",
        "diff.external=",
        "diff",
        "--no-ext-diff",
        "--no-textconv",
        "--full-index",
        "--binary",
        &merge_base,
        &branch.object,
    ])?;
    let Some((branch_patch_id, _)) = branch_patch_ids.first() else {
        return Ok(false);
    };

    let excluded_merge_base = format!("^{merge_base}");
    let historical_patch_ids = git_patch_ids(&[
        "-c",
        "diff.external=",
        "log",
        "--first-parent",
        "--no-merges",
        "--format=medium",
        "--no-ext-diff",
        "--no-textconv",
        "--full-index",
        "--binary",
        "-p",
        base_object,
        &excluded_merge_base,
    ])?;

    for (patch_id, commit) in historical_patch_ids {
        if patch_id != *branch_patch_id {
            continue;
        }
        let parent = rev_parse_commit(&format!("{commit}^"))?;
        let Some(merged_tree) = merge_tree(&parent, &branch.object)? else {
            continue;
        };
        if merged_tree == revision_tree(&commit)? {
            return Ok(true);
        }
    }

    Ok(false)
}

fn merge_tree(left: &str, right: &str) -> AnyResult<Option<String>> {
    let arguments = ["merge-tree", "--write-tree", left, right];
    let merge = git_output(&arguments)?;
    if !merge.status.success() {
        if merge.status.code() == Some(1) {
            return Ok(None);
        }
        return Err(git_failure(&arguments, &merge));
    }
    let merged_tree = String::from_utf8(merge.stdout)
        .context("git merge-tree returned non-UTF-8 output")?
        .lines()
        .next()
        .context("git merge-tree returned no tree")?
        .trim()
        .to_owned();
    Ok(Some(merged_tree))
}

fn revision_tree(revision: &str) -> AnyResult<String> {
    git_stdout(&["rev-parse", &format!("{revision}^{{tree}}")])
}

fn git_patch_ids(arguments: &[&str]) -> AnyResult<Vec<(String, String)>> {
    let mut producer = Command::new("git")
        .args(arguments)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .with_context(|| format!("failed to run git {}", arguments.join(" ")))?;
    let stdout = producer
        .stdout
        .take()
        .context("git returned no stdout pipe")?;
    let patch_arguments = ["patch-id", "--stable"];
    let patch_output = Command::new("git")
        .args(patch_arguments)
        .stdin(Stdio::from(stdout))
        .output()
        .context("failed to run git patch-id --stable")?;
    let producer_output = producer
        .wait_with_output()
        .with_context(|| format!("failed to wait for git {}", arguments.join(" ")))?;
    if !producer_output.status.success() {
        return Err(git_failure(arguments, &producer_output));
    }
    if !patch_output.status.success() {
        return Err(git_failure(&patch_arguments, &patch_output));
    }

    String::from_utf8(patch_output.stdout)
        .context("git patch-id returned non-UTF-8 output")?
        .lines()
        .map(|line| {
            let fields = line.split_whitespace().collect::<Vec<_>>();
            if fields.len() != 2 {
                bail!("unexpected git patch-id output");
            }
            Ok((fields[0].to_owned(), fields[1].to_owned()))
        })
        .collect()
}

fn load_exclude_config() -> AnyResult<ExcludeConfig> {
    let path = exclude_path()?;
    let contents = match fs::read_to_string(&path) {
        Ok(contents) => contents,
        Err(error) if error.kind() == ErrorKind::NotFound => return Ok(ExcludeConfig::default()),
        Err(error) => {
            return Err(error)
                .with_context(|| format!("failed to read exclude file {}", path.display()));
        }
    };
    let mut config = ExcludeConfig::default();
    for line in contents.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        if line.starts_with('#') {
            config.comments.push(line.to_owned());
            continue;
        }
        validate_pattern(line).with_context(|| format!("invalid pattern in {}", path.display()))?;
        config.patterns.insert(line.to_owned());
    }
    Ok(config)
}

fn write_exclude_config(config: &ExcludeConfig) -> AnyResult {
    let path = exclude_path()?;
    let parent = path.parent().context("exclude file has no parent")?;
    fs::create_dir_all(parent).with_context(|| format!("failed to create {}", parent.display()))?;
    let mut contents = String::new();
    for comment in &config.comments {
        contents.push_str(comment);
        contents.push('\n');
    }
    if !config.comments.is_empty() && !config.patterns.is_empty() {
        contents.push('\n');
    }
    for pattern in &config.patterns {
        contents.push_str(pattern);
        contents.push('\n');
    }
    let mut temporary = NamedTempFile::new_in(parent)
        .with_context(|| format!("failed to create a temporary file in {}", parent.display()))?;
    temporary
        .write_all(contents.as_bytes())
        .with_context(|| format!("failed to write exclude file {}", path.display()))?;
    temporary
        .flush()
        .with_context(|| format!("failed to flush exclude file {}", path.display()))?;
    temporary
        .as_file()
        .sync_all()
        .with_context(|| format!("failed to sync exclude file {}", path.display()))?;
    temporary
        .persist(&path)
        .map_err(|error| error.error)
        .with_context(|| format!("failed to replace exclude file {}", path.display()))?;
    Ok(())
}

fn exclude_path() -> AnyResult<PathBuf> {
    let common = git_stdout(&["rev-parse", "--path-format=absolute", "--git-common-dir"])?;
    Ok(Path::new(&common).join(EXCLUDE_FILE))
}

fn validate_pattern(pattern: &str) -> AnyResult {
    if pattern.trim() != pattern || pattern.is_empty() || pattern.starts_with('#') {
        bail!("invalid exclude pattern: {pattern:?}");
    }
    Glob::new(pattern).with_context(|| format!("invalid exclude pattern {pattern:?}"))?;
    Ok(())
}

fn build_glob_set(patterns: &BTreeSet<String>) -> AnyResult<GlobSet> {
    let mut builder = GlobSetBuilder::new();
    for pattern in patterns {
        builder.add(
            Glob::new(pattern).with_context(|| format!("invalid exclude pattern {pattern:?}"))?,
        );
    }
    builder.build().context("failed to build exclude patterns")
}

fn rev_parse_commit(revision: &str) -> AnyResult<String> {
    git_stdout(&["rev-parse", "--verify", &format!("{revision}^{{commit}}")])
}

fn print_status(status: &str, branch: &str, reason: &str) {
    println!("{status:<6} {branch}  {reason}");
}

fn git_stdout(arguments: &[&str]) -> AnyResult<String> {
    let output = git_output(arguments)?;
    if !output.status.success() {
        return Err(git_failure(arguments, &output));
    }
    Ok(String::from_utf8(output.stdout)
        .context("git returned non-UTF-8 output")?
        .trim()
        .to_owned())
}

fn git_checked(arguments: &[&str]) -> AnyResult {
    let output = git_output(arguments)?;
    if output.status.success() {
        Ok(())
    } else {
        Err(git_failure(arguments, &output))
    }
}

fn git_status(arguments: &[&str]) -> AnyResult<ExitStatus> {
    Ok(git_output(arguments)?.status)
}

fn git_output(arguments: &[&str]) -> AnyResult<Output> {
    Command::new("git")
        .args(arguments)
        .output()
        .with_context(|| format!("failed to run git {}", arguments.join(" ")))
}

fn git_failure(arguments: &[&str], output: &Output) -> anyhow::Error {
    let stderr = String::from_utf8_lossy(&output.stderr);
    anyhow::anyhow!(
        "git {} failed with {}: {}",
        arguments.join(" "),
        output.status,
        stderr.trim()
    )
}
