use std::collections::BTreeSet;
use std::env;
use std::ffi::OsString;
use std::fs;
use std::io::{ErrorKind, Write};
use std::path::{Component, Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context, bail};
use tempfile::NamedTempFile;

use crate::AnyResult;

const SKILL_NAME: &str = "dcx-cli";
const MANIFEST_NAME: &str = ".dcx-managed";
const MANIFEST_HEADER: &str = "dcx-skill-manifest-v1";

struct BundledFile {
    path: &'static str,
    contents: &'static [u8],
}

const BUNDLED_FILES: &[BundledFile] = &[
    BundledFile {
        path: "SKILL.md",
        contents: include_bytes!("../skills/dcx-cli/SKILL.md"),
    },
    BundledFile {
        path: "agents/openai.yaml",
        contents: include_bytes!("../skills/dcx-cli/agents/openai.yaml"),
    },
];

#[derive(Debug)]
struct SkillPaths {
    logical: PathBuf,
    resolved: PathBuf,
}

pub fn auto_update() -> AnyResult {
    let paths = skill_paths(false)?;
    if !paths.logical.exists() {
        return Ok(());
    }
    if !is_managed_directory(&paths.logical)? {
        return Ok(());
    }
    synchronize(&paths.logical)
}

pub fn install(force: bool) -> AnyResult {
    let mut paths = skill_paths(true)?;
    let mut backup = None;

    if paths.logical.exists() || fs::symlink_metadata(&paths.logical).is_ok() {
        if is_managed_directory(&paths.logical)? {
            synchronize(&paths.logical)?;
        } else if force {
            let backup_path = backup_path(&paths.logical)?;
            fs::rename(&paths.logical, &backup_path).with_context(|| {
                format!(
                    "failed to move unmanaged skill to {}",
                    backup_path.display()
                )
            })?;
            backup = Some(backup_path);
            create_and_synchronize(&paths.logical)?;
        } else {
            bail!(
                "{} already exists and is not managed by dcx; use --force to back it up and replace it",
                paths.logical.display()
            );
        }
    } else {
        create_and_synchronize(&paths.logical)?;
    }

    paths.resolved = resolve_path(&paths.logical)?;
    println!("logical-path: {}", paths.logical.display());
    println!("resolved-path: {}", paths.resolved.display());
    if let Some(backup) = backup {
        println!("backup-path: {}", backup.display());
    }
    Ok(())
}

pub fn status() -> AnyResult<bool> {
    let paths = skill_paths(false)?;
    let status = if !paths.logical.exists() && fs::symlink_metadata(&paths.logical).is_err() {
        "missing"
    } else if !is_managed_directory(&paths.logical)? {
        "unmanaged"
    } else if is_current(&paths.logical)? {
        "current"
    } else {
        "outdated"
    };

    println!("status: {status}");
    println!("logical-path: {}", paths.logical.display());
    println!("resolved-path: {}", paths.resolved.display());
    Ok(status == "current")
}

pub fn print_path() -> AnyResult {
    println!("{}", skill_paths(false)?.resolved.display());
    Ok(())
}

pub fn uninstall() -> AnyResult {
    let paths = skill_paths(false)?;
    if !paths.logical.exists() && fs::symlink_metadata(&paths.logical).is_err() {
        return Ok(());
    }
    if !is_managed_directory(&paths.logical)? {
        bail!("{} is not managed by dcx", paths.logical.display());
    }

    let manifest_path = paths.logical.join(MANIFEST_NAME);
    ensure_regular_file(&manifest_path)?;
    let managed_files = read_manifest(&manifest_path)?;
    for relative in &managed_files {
        remove_managed_file(&paths.logical, relative)?;
    }
    fs::remove_file(&manifest_path)
        .with_context(|| format!("failed to remove {}", manifest_path.display()))?;

    let mut directories = managed_files
        .iter()
        .filter_map(|path| path.parent())
        .filter(|path| !path.as_os_str().is_empty())
        .map(Path::to_path_buf)
        .collect::<Vec<_>>();
    directories.sort_by_key(|path| std::cmp::Reverse(path.components().count()));
    directories.dedup();
    for relative in directories {
        remove_directory_if_empty(&paths.logical.join(relative))?;
    }
    remove_directory_if_empty(&paths.logical)?;
    println!("uninstalled: {}", paths.logical.display());
    Ok(())
}

fn skill_paths(create_root: bool) -> AnyResult<SkillPaths> {
    let root = skills_root()?;
    if create_root {
        fs::create_dir_all(&root)
            .with_context(|| format!("failed to create skills directory {}", root.display()))?;
    }
    let logical = root.join(SKILL_NAME);
    let resolved = resolve_path(&logical)?;
    Ok(SkillPaths { logical, resolved })
}

fn skills_root() -> AnyResult<PathBuf> {
    if let Some(directory) = env::var_os("DCX_SKILLS_DIR") {
        let directory = PathBuf::from(directory);
        if !directory.is_absolute() {
            bail!("DCX_SKILLS_DIR must be an absolute path");
        }
        return Ok(directory);
    }

    let home = home_directory().context("failed to determine the home directory")?;
    Ok(home.join(".agents").join("skills"))
}

#[cfg(windows)]
fn home_directory() -> Option<PathBuf> {
    env::var_os("USERPROFILE")
        .or_else(|| env::var_os("HOME"))
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
}

#[cfg(not(windows))]
fn home_directory() -> Option<PathBuf> {
    env::var_os("HOME")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
}

fn resolve_path(path: &Path) -> AnyResult<PathBuf> {
    if path.exists() {
        return fs::canonicalize(path)
            .with_context(|| format!("failed to resolve {}", path.display()));
    }
    if let Some(parent) = path.parent() {
        if parent.exists() {
            let parent = fs::canonicalize(parent)
                .with_context(|| format!("failed to resolve {}", parent.display()))?;
            return Ok(parent.join(path.file_name().unwrap_or_default()));
        }
    }
    Ok(path.to_path_buf())
}

fn is_managed_directory(path: &Path) -> AnyResult<bool> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == ErrorKind::NotFound => return Ok(false),
        Err(error) => {
            return Err(error).with_context(|| format!("failed to inspect {}", path.display()));
        }
    };
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Ok(false);
    }

    let manifest = path.join(MANIFEST_NAME);
    match fs::symlink_metadata(&manifest) {
        Ok(metadata) if metadata.file_type().is_symlink() => {
            bail!(
                "managed manifest must not be a symbolic link: {}",
                manifest.display()
            )
        }
        Ok(metadata) => Ok(metadata.is_file()),
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(false),
        Err(error) => {
            Err(error).with_context(|| format!("failed to inspect {}", manifest.display()))
        }
    }
}

fn create_and_synchronize(path: &Path) -> AnyResult {
    fs::create_dir_all(path)
        .with_context(|| format!("failed to create skill directory {}", path.display()))?;
    synchronize(path)
}

fn synchronize(path: &Path) -> AnyResult {
    ensure_directory_is_not_symlink(path)?;
    let manifest_path = path.join(MANIFEST_NAME);
    let old_files = if manifest_path.exists() {
        ensure_regular_file(&manifest_path)?;
        read_manifest(&manifest_path)?
    } else {
        BTreeSet::new()
    };
    let new_files = bundled_file_paths();

    for stale in old_files.difference(&new_files) {
        remove_managed_file(path, stale)?;
    }
    for bundled in BUNDLED_FILES {
        let relative = safe_relative_path(bundled.path)?;
        let destination = path.join(&relative);
        ensure_internal_parents(path, &relative)?;
        if destination.exists() || fs::symlink_metadata(&destination).is_ok() {
            ensure_regular_file(&destination)?;
        }
        if fs::read(&destination).ok().as_deref() != Some(bundled.contents) {
            atomic_write(&destination, bundled.contents)?;
        }
    }

    let manifest = manifest_contents(&new_files);
    if fs::read(&manifest_path).ok().as_deref() != Some(manifest.as_bytes()) {
        atomic_write(&manifest_path, manifest.as_bytes())?;
    }
    Ok(())
}

fn is_current(path: &Path) -> AnyResult<bool> {
    ensure_directory_is_not_symlink(path)?;
    let manifest_path = path.join(MANIFEST_NAME);
    ensure_regular_file(&manifest_path)?;
    let managed_files = read_manifest(&manifest_path)?;
    if managed_files != bundled_file_paths() {
        return Ok(false);
    }
    for bundled in BUNDLED_FILES {
        let relative = safe_relative_path(bundled.path)?;
        let destination = path.join(relative);
        match fs::symlink_metadata(&destination) {
            Ok(metadata) if metadata.file_type().is_symlink() => {
                bail!(
                    "managed skill file must not be a symbolic link: {}",
                    destination.display()
                );
            }
            Ok(metadata) if !metadata.is_file() => {
                bail!(
                    "managed skill path is not a file: {}",
                    destination.display()
                );
            }
            Ok(_) => {}
            Err(error) if error.kind() == ErrorKind::NotFound => return Ok(false),
            Err(error) => {
                return Err(error)
                    .with_context(|| format!("failed to inspect {}", destination.display()));
            }
        }
        if fs::read(&destination)
            .with_context(|| format!("failed to read {}", destination.display()))?
            != bundled.contents
        {
            return Ok(false);
        }
    }
    Ok(true)
}

fn bundled_file_paths() -> BTreeSet<PathBuf> {
    BUNDLED_FILES
        .iter()
        .map(|file| PathBuf::from(file.path))
        .collect()
}

fn manifest_contents(files: &BTreeSet<PathBuf>) -> String {
    let mut output = format!("{MANIFEST_HEADER}\n");
    for path in files {
        output.push_str("file=");
        output.push_str(&path.to_string_lossy());
        output.push('\n');
    }
    output
}

fn read_manifest(path: &Path) -> AnyResult<BTreeSet<PathBuf>> {
    let contents = fs::read_to_string(path)
        .with_context(|| format!("failed to read managed manifest {}", path.display()))?;
    let mut lines = contents.lines();
    if lines.next() != Some(MANIFEST_HEADER) {
        bail!("invalid managed manifest: {}", path.display());
    }
    lines
        .map(|line| {
            let relative = line
                .strip_prefix("file=")
                .context("invalid managed manifest entry")?;
            safe_relative_path(relative)
        })
        .collect()
}

fn safe_relative_path(path: &str) -> AnyResult<PathBuf> {
    let path = PathBuf::from(path);
    if path.as_os_str().is_empty()
        || path
            .components()
            .any(|component| !matches!(component, Component::Normal(_)))
    {
        bail!("invalid managed skill path: {}", path.display());
    }
    Ok(path)
}

fn ensure_internal_parents(root: &Path, relative: &Path) -> AnyResult {
    let Some(parent) = relative.parent() else {
        return Ok(());
    };
    let mut current = root.to_path_buf();
    for component in parent.components() {
        let Component::Normal(component) = component else {
            bail!("invalid managed skill path: {}", relative.display());
        };
        current.push(component);
        match fs::symlink_metadata(&current) {
            Ok(metadata) if metadata.file_type().is_symlink() => {
                bail!(
                    "managed skill path must not be a symbolic link: {}",
                    current.display()
                )
            }
            Ok(metadata) if !metadata.is_dir() => {
                bail!(
                    "managed skill parent is not a directory: {}",
                    current.display()
                )
            }
            Ok(_) => {}
            Err(error) if error.kind() == ErrorKind::NotFound => fs::create_dir(&current)
                .with_context(|| format!("failed to create {}", current.display()))?,
            Err(error) => {
                return Err(error)
                    .with_context(|| format!("failed to inspect {}", current.display()));
            }
        }
    }
    Ok(())
}

fn ensure_directory_is_not_symlink(path: &Path) -> AnyResult {
    let metadata = fs::symlink_metadata(path)
        .with_context(|| format!("failed to inspect {}", path.display()))?;
    if metadata.file_type().is_symlink() {
        bail!(
            "managed skill directory must not be a symbolic link: {}",
            path.display()
        );
    }
    if !metadata.is_dir() {
        bail!("managed skill path is not a directory: {}", path.display());
    }
    Ok(())
}

fn ensure_regular_file(path: &Path) -> AnyResult {
    let metadata = fs::symlink_metadata(path)
        .with_context(|| format!("failed to inspect {}", path.display()))?;
    if metadata.file_type().is_symlink() {
        bail!(
            "managed skill file must not be a symbolic link: {}",
            path.display()
        );
    }
    if !metadata.is_file() {
        bail!("managed skill path is not a file: {}", path.display());
    }
    Ok(())
}

fn remove_managed_file(root: &Path, relative: &Path) -> AnyResult {
    let path = root.join(relative);
    match fs::symlink_metadata(&path) {
        Ok(metadata) if metadata.file_type().is_symlink() => {
            bail!(
                "managed skill file must not be a symbolic link: {}",
                path.display()
            )
        }
        Ok(metadata) if metadata.is_file() => fs::remove_file(&path)
            .with_context(|| format!("failed to remove managed file {}", path.display())),
        Ok(_) => bail!("managed skill path is not a file: {}", path.display()),
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error).with_context(|| format!("failed to inspect {}", path.display())),
    }
}

fn atomic_write(path: &Path, contents: &[u8]) -> AnyResult {
    let parent = path.parent().context("managed skill file has no parent")?;
    fs::create_dir_all(parent).with_context(|| format!("failed to create {}", parent.display()))?;
    let mut temporary = NamedTempFile::new_in(parent)
        .with_context(|| format!("failed to create a temporary file in {}", parent.display()))?;
    temporary
        .write_all(contents)
        .with_context(|| format!("failed to write temporary file for {}", path.display()))?;
    temporary
        .flush()
        .with_context(|| format!("failed to flush temporary file for {}", path.display()))?;
    temporary
        .as_file()
        .sync_all()
        .with_context(|| format!("failed to sync temporary file for {}", path.display()))?;
    temporary
        .persist(path)
        .map_err(|error| error.error)
        .with_context(|| format!("failed to replace {}", path.display()))?;
    Ok(())
}

fn backup_path(skill_path: &Path) -> AnyResult<PathBuf> {
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .context("system time is before the Unix epoch")?
        .as_secs();
    let mut name = OsString::from(SKILL_NAME);
    name.push(format!(".backup-{timestamp}"));
    let path = skill_path
        .parent()
        .context("skill path has no parent")?
        .join(name);
    if path.exists() || fs::symlink_metadata(&path).is_ok() {
        bail!("backup path already exists: {}", path.display());
    }
    Ok(path)
}

fn remove_directory_if_empty(path: &Path) -> AnyResult {
    match fs::remove_dir(path) {
        Ok(()) => Ok(()),
        Err(error)
            if matches!(
                error.kind(),
                ErrorKind::NotFound | ErrorKind::DirectoryNotEmpty
            ) =>
        {
            Ok(())
        }
        Err(error) => Err(error).with_context(|| format!("failed to remove {}", path.display())),
    }
}
