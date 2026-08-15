#![allow(deprecated)]

use std::collections::{BTreeSet, HashMap, HashSet};
use std::fs::{self, File, OpenOptions};
use std::hash::{Hash, Hasher, SipHasher};
use std::io::{IsTerminal, Write};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, SystemTime};

use anyhow::{Context, bail};
use comfy_table::{Cell, Table};
use fs2::FileExt;
use rustc_stable_hash::StableSipHasher128;
use serde_json::Value;
use size::Size;

use crate::{AnyResult, cargo_cleanup_tui};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum CandidateKind {
    Toolchain,
    Incremental,
}

impl CandidateKind {
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Toolchain => "旧工具链",
            Self::Incremental => "增量缓存",
        }
    }

    pub(crate) fn risk(self) -> RiskLevel {
        match self {
            Self::Toolchain => RiskLevel::Low,
            Self::Incremental => RiskLevel::Medium,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum RiskLevel {
    Low,
    Medium,
}

impl RiskLevel {
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Low => "低风险",
            Self::Medium => "中风险",
        }
    }

    pub(crate) fn explanation(self) -> &'static str {
        match self {
            Self::Low => "当前环境已经无法复用这些产物，清理后通常没有额外代价。",
            Self::Medium => "缓存仅因长期未修改而入选，清理后下一次编译可能明显变慢。",
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct CleanupCandidate {
    pub(crate) kind: CandidateKind,
    pub(crate) profile: String,
    pub(crate) reason: String,
    pub(crate) size: u64,
    pub(crate) paths: Vec<PathBuf>,
    pub(crate) profile_path: PathBuf,
}

/// 审计并清理当前 Cargo workspace 的低复用概率构建缓存。
pub fn cleanup(days: u64, dry_run: bool, yes: bool) -> AnyResult {
    let target = resolve_target_directory()?;
    if !target.is_dir() {
        println!("未找到 Cargo target 目录：{}", target.display());
        return Ok(());
    }

    let installed = installed_toolchain_hashes()?;
    let candidates = scan_target(&target, &installed, days)?;
    if candidates.is_empty() {
        println!(
            "未在 {} 中发现可清理的 Cargo 缓存（incremental 阈值：{days} 天）",
            target.display()
        );
        return Ok(());
    }

    print_report(&target, &candidates, days)?;
    if dry_run {
        return Ok(());
    }

    let selected = if yes {
        (0..candidates.len()).collect::<Vec<_>>()
    } else {
        if !std::io::stdin().is_terminal() || !std::io::stderr().is_terminal() {
            bail!("交互式清理需要终端；请改用 --dry-run 或 --yes");
        }
        cargo_cleanup_tui::select_candidates(candidates.clone())?
    };
    if selected.is_empty() {
        println!("未选择任何缓存，没有清理文件。");
        return Ok(());
    }

    let selected_candidates = selected
        .into_iter()
        .filter_map(|index| candidates.get(index))
        .collect::<Vec<_>>();
    let _locks = lock_profiles(&selected_candidates)?;
    let incremental_cutoff = cutoff_for_days(days);
    let mut removed_size = 0;
    let mut removed_paths = 0;
    for candidate in selected_candidates {
        for path in &candidate.paths {
            if !path.exists() {
                continue;
            }
            ensure_cleanup_path(&target, path)?;
            if candidate.kind == CandidateKind::Incremental
                && latest_modified(path)? > incremental_cutoff
            {
                continue;
            }
            removed_size += path_size(path);
            remove_path(path).with_context(|| format!("无法清理缓存 {}", path.display()))?;
            removed_paths += 1;
        }
    }

    println!(
        "已清理 {removed_paths} 个缓存路径（约 {}）。",
        Size::from_bytes(removed_size)
    );
    Ok(())
}

fn resolve_target_directory() -> AnyResult<PathBuf> {
    let cargo = std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into());
    let output = Command::new(cargo)
        .args(["metadata", "--no-deps", "--format-version", "1"])
        .output()
        .context("无法执行 cargo metadata")?;
    if !output.status.success() {
        bail!(
            "cargo metadata 执行失败：{}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    let metadata: Value =
        serde_json::from_slice(&output.stdout).context("无法解析 cargo metadata 输出")?;
    let target = metadata
        .get("target_directory")
        .and_then(Value::as_str)
        .context("cargo metadata 未返回 target_directory")?;
    Ok(PathBuf::from(target))
}

fn installed_toolchain_hashes() -> AnyResult<HashSet<u64>> {
    let mut hashes = HashSet::from([0]);
    let toolchains = Command::new("rustup")
        .args(["toolchain", "list"])
        .output()
        .ok()
        .filter(|output| output.status.success())
        .map(|output| {
            String::from_utf8_lossy(&output.stdout)
                .lines()
                .filter_map(|line| line.split_whitespace().next())
                .map(str::to_owned)
                .collect::<Vec<_>>()
        });

    if let Some(toolchains) = toolchains {
        for toolchain in toolchains {
            let output = Command::new("rustc")
                .args([format!("+{toolchain}"), "-vV".to_owned()])
                .output();
            if let Ok(output) = output
                && output.status.success()
            {
                add_rustc_hashes(&mut hashes, &output.stdout);
            }
        }
    }
    if hashes.len() == 1 {
        let output = Command::new("rustc")
            .arg("-vV")
            .output()
            .context("无法执行 rustc -vV")?;
        if !output.status.success() {
            bail!(
                "rustc -vV 执行失败：{}",
                String::from_utf8_lossy(&output.stderr).trim()
            );
        }
        add_rustc_hashes(&mut hashes, &output.stdout);
    }
    Ok(hashes)
}

fn add_rustc_hashes(hashes: &mut HashSet<u64>, output: &[u8]) {
    let version = String::from_utf8_lossy(output);
    let mut current = StableSipHasher128::new();
    version.hash(&mut current);
    hashes.insert(Hasher::finish(&current));

    // Cargo 1.84 and older used SipHasher for this fingerprint field.
    let mut legacy = SipHasher::new_with_keys(0, 0);
    version.hash(&mut legacy);
    hashes.insert(legacy.finish());
}

pub(crate) fn scan_target(
    target: &Path,
    installed_toolchains: &HashSet<u64>,
    incremental_days: u64,
) -> AnyResult<Vec<CleanupCandidate>> {
    let mut fingerprint_dirs = Vec::new();
    find_fingerprint_dirs(target, target, 0, &mut fingerprint_dirs)?;
    let cutoff = cutoff_for_days(incremental_days);
    let mut candidates = Vec::new();

    for fingerprint_dir in fingerprint_dirs {
        let profile_path = fingerprint_dir
            .parent()
            .context("fingerprint 目录缺少 profile 父目录")?;
        let profile = profile_path
            .strip_prefix(target)
            .unwrap_or(profile_path)
            .display()
            .to_string();

        let stale_hashes = stale_artifact_hashes(&fingerprint_dir, installed_toolchains)?;
        let paths = artifact_paths(profile_path, &stale_hashes)?;
        if !paths.is_empty() {
            candidates.push(CleanupCandidate {
                kind: CandidateKind::Toolchain,
                profile: profile.clone(),
                reason: "由已卸载的 rustc toolchain 生成".to_owned(),
                size: paths.iter().map(|path| path_size(path)).sum(),
                paths,
                profile_path: profile_path.to_owned(),
            });
        }

        let incremental = profile_path.join("incremental");
        let paths = stale_incremental_paths(&incremental, cutoff)?;
        if !paths.is_empty() {
            candidates.push(CleanupCandidate {
                kind: CandidateKind::Incremental,
                profile,
                reason: format!("至少 {incremental_days} 天未修改"),
                size: paths.iter().map(|path| path_size(path)).sum(),
                paths,
                profile_path: profile_path.to_owned(),
            });
        }
    }

    candidates.sort_by(|left, right| {
        left.kind
            .risk()
            .cmp(&right.kind.risk())
            .then(left.profile.cmp(&right.profile))
            .then(left.kind.cmp(&right.kind))
    });
    Ok(candidates)
}

fn cutoff_for_days(days: u64) -> SystemTime {
    SystemTime::now()
        .checked_sub(Duration::from_secs(days.saturating_mul(24 * 60 * 60)))
        .unwrap_or(SystemTime::UNIX_EPOCH)
}

fn find_fingerprint_dirs(
    target: &Path,
    directory: &Path,
    depth: usize,
    found: &mut Vec<PathBuf>,
) -> AnyResult {
    if depth > 2 {
        return Ok(());
    }
    for entry in fs::read_dir(directory)
        .with_context(|| format!("无法读取 target 目录 {}", directory.display()))?
    {
        let entry = entry?;
        let file_type = entry.file_type()?;
        if !file_type.is_dir() || file_type.is_symlink() {
            continue;
        }
        let path = entry.path();
        if entry.file_name() == ".fingerprint" {
            found.push(path);
        } else if path != target.join("doc") && path != target.join("package") {
            find_fingerprint_dirs(target, &path, depth + 1, found)?;
        }
    }
    Ok(())
}

fn stale_artifact_hashes(
    fingerprint_dir: &Path,
    installed_toolchains: &HashSet<u64>,
) -> AnyResult<HashSet<String>> {
    let mut reusable = HashMap::<String, bool>::new();
    for entry in fs::read_dir(fingerprint_dir)? {
        let entry = entry?;
        if !entry.file_type()?.is_dir() {
            continue;
        }
        let name = entry.file_name();
        let name = name.to_string_lossy();
        let Some(hash) = hash_from_path_name(&name) else {
            continue;
        };
        let installed = fingerprint_rustc(&entry.path())
            .map(|rustc| installed_toolchains.contains(&rustc))
            .unwrap_or(true);
        reusable
            .entry(hash.to_owned())
            .and_modify(|value| *value |= installed)
            .or_insert(installed);
    }
    Ok(reusable
        .into_iter()
        .filter_map(|(hash, installed)| (!installed).then_some(hash))
        .collect())
}

fn fingerprint_rustc(directory: &Path) -> Option<u64> {
    fs::read_dir(directory)
        .ok()?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| {
            path.extension()
                .is_some_and(|extension| extension == "json")
        })
        .find_map(|path| {
            let value: Value = serde_json::from_slice(&fs::read(path).ok()?).ok()?;
            value.get("rustc")?.as_u64()
        })
}

fn hash_from_path_name(name: &str) -> Option<&str> {
    let stem = name.split('.').next()?;
    let hash = stem.rsplit_once('-')?.1;
    (hash.len() == 16 && hash.bytes().all(|byte| byte.is_ascii_hexdigit())).then_some(hash)
}

fn artifact_paths(profile: &Path, stale_hashes: &HashSet<String>) -> AnyResult<Vec<PathBuf>> {
    let mut paths = Vec::new();
    for directory in [
        profile.to_owned(),
        profile.join(".fingerprint"),
        profile.join("build"),
        profile.join("deps"),
        profile.join("native"),
    ] {
        let Ok(entries) = fs::read_dir(directory) else {
            continue;
        };
        for entry in entries {
            let entry = entry?;
            let name = entry.file_name();
            if hash_from_path_name(&name.to_string_lossy())
                .is_some_and(|hash| stale_hashes.contains(hash))
            {
                paths.push(entry.path());
            }
        }
    }
    paths.sort();
    paths.dedup();
    Ok(paths)
}

fn stale_incremental_paths(directory: &Path, cutoff: SystemTime) -> AnyResult<Vec<PathBuf>> {
    let Ok(entries) = fs::read_dir(directory) else {
        return Ok(Vec::new());
    };
    let mut paths = Vec::new();
    for entry in entries {
        let entry = entry?;
        let path = entry.path();
        if entry.file_type()?.is_dir() && latest_modified(&path)? <= cutoff {
            paths.push(path);
        }
    }
    paths.sort();
    Ok(paths)
}

fn latest_modified(path: &Path) -> AnyResult<SystemTime> {
    let mut latest = fs::symlink_metadata(path)?.modified()?;
    if fs::symlink_metadata(path)?.file_type().is_dir() {
        for entry in fs::read_dir(path)? {
            latest = latest.max(latest_modified(&entry?.path())?);
        }
    }
    Ok(latest)
}

fn path_size(path: &Path) -> u64 {
    let Ok(metadata) = fs::symlink_metadata(path) else {
        return 0;
    };
    if !metadata.file_type().is_dir() {
        return metadata.len();
    }
    fs::read_dir(path)
        .into_iter()
        .flatten()
        .filter_map(Result::ok)
        .map(|entry| path_size(&entry.path()))
        .sum()
}

fn print_report(target: &Path, candidates: &[CleanupCandidate], days: u64) -> AnyResult {
    let mut table = Table::new();
    table.set_header(["构建配置", "风险", "类别", "大小", "路径数", "入选原因"]);
    for candidate in candidates {
        table.add_row([
            Cell::new(&candidate.profile),
            Cell::new(candidate.kind.risk().label()),
            Cell::new(candidate.kind.label()),
            Cell::new(Size::from_bytes(candidate.size)),
            Cell::new(candidate.paths.len()),
            Cell::new(&candidate.reason),
        ]);
    }
    writeln!(std::io::stdout(), "Target 目录：{}", target.display())?;
    writeln!(
        std::io::stdout(),
        "Incremental 缓存过期阈值：{days} 天\n{table}"
    )?;
    Ok(())
}

struct ProfileLocks(#[allow(dead_code)] Vec<File>);

fn lock_profiles(candidates: &[&CleanupCandidate]) -> AnyResult<ProfileLocks> {
    let profiles = candidates
        .iter()
        .map(|candidate| candidate.profile_path.clone())
        .collect::<BTreeSet<_>>();
    let mut locks = Vec::new();
    for profile in profiles {
        for name in [".cargo-lock", ".cargo-build-lock"] {
            let path = profile.join(name);
            let file = OpenOptions::new()
                .read(true)
                .write(true)
                .create(true)
                .truncate(false)
                .open(&path)
                .with_context(|| format!("无法打开 Cargo 锁文件 {}", path.display()))?;
            FileExt::try_lock_exclusive(&file).with_context(|| {
                format!(
                    "Cargo target profile 正在使用，拒绝清理 {}",
                    profile.display()
                )
            })?;
            locks.push(file);
        }
    }
    Ok(ProfileLocks(locks))
}

fn ensure_cleanup_path(target: &Path, path: &Path) -> AnyResult {
    if path == target || !path.starts_with(target) {
        bail!("拒绝删除 Cargo target 以外的路径：{}", path.display());
    }
    Ok(())
}

fn remove_path(path: &Path) -> std::io::Result<()> {
    let metadata = fs::symlink_metadata(path)?;
    if metadata.file_type().is_dir() {
        fs::remove_dir_all(path)
    } else {
        fs::remove_file(path)
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;
    use std::fs;

    use super::{CandidateKind, scan_target};

    #[test]
    fn finds_stale_toolchain_artifacts_and_incremental_cache() {
        let directory = tempfile::tempdir().unwrap();
        let profile = directory.path().join("debug");
        let fingerprint = profile
            .join(".fingerprint")
            .join("old-crate-0123456789abcdef");
        fs::create_dir_all(&fingerprint).unwrap();
        fs::write(fingerprint.join("lib-old_crate.json"), r#"{"rustc":42}"#).unwrap();
        fs::create_dir_all(profile.join("deps")).unwrap();
        fs::write(
            profile.join("deps/libold_crate-0123456789abcdef.rlib"),
            b"artifact",
        )
        .unwrap();
        let incremental = profile.join("incremental/old_crate-0123456789abcdef");
        fs::create_dir_all(&incremental).unwrap();
        fs::write(incremental.join("cache.bin"), b"cache").unwrap();

        let candidates = scan_target(directory.path(), &HashSet::from([7]), 0).unwrap();

        assert!(
            candidates
                .iter()
                .any(|candidate| candidate.kind == CandidateKind::Toolchain)
        );
        assert!(
            candidates
                .iter()
                .any(|candidate| candidate.kind == CandidateKind::Incremental)
        );
        assert!(
            candidates
                .iter()
                .flat_map(|candidate| &candidate.paths)
                .any(|path| path.ends_with("libold_crate-0123456789abcdef.rlib"))
        );
    }

    #[test]
    fn keeps_unhashed_final_artifacts_and_unknown_fingerprints() {
        let directory = tempfile::tempdir().unwrap();
        let profile = directory.path().join("release");
        let fingerprint = profile
            .join(".fingerprint")
            .join("unknown-0123456789abcdef");
        fs::create_dir_all(&fingerprint).unwrap();
        fs::write(fingerprint.join("invoked.timestamp"), b"").unwrap();
        fs::write(profile.join("application"), b"final binary").unwrap();

        let candidates = scan_target(directory.path(), &HashSet::new(), 30).unwrap();

        assert!(candidates.is_empty());
        assert!(profile.join("application").exists());
    }
}
