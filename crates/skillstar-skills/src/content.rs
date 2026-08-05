//! Skill content use cases.
//!
//! This module owns hub/lockfile path resolution, Git worktree preparation,
//! local-skill mutations, and cache invalidation. Framework adapters should
//! call this interface instead of rebuilding those rules.

use std::path::{Component, Path, PathBuf};

use sha2::{Digest, Sha256};
use skillstar_core::infra::error::AppError;
use skillstar_core::types::{SkillContent, parse_skill_content};

use crate::git::ops as git_ops;
use crate::{installed_skill, local_skill, lockfile};

const SNAPSHOT_HASH_DOMAIN: &[u8] = b"skillstar.skill-snapshot.v2\0";
pub(crate) const SNAPSHOT_HASH_VERSION: u32 = 2;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SnapshotLimits {
    pub max_files: usize,
    pub max_file_bytes: u64,
    pub max_total_bytes: u64,
    pub max_depth: usize,
}

impl Default for SnapshotLimits {
    fn default() -> Self {
        Self {
            max_files: 2_048,
            max_file_bytes: 8 * 1024 * 1024,
            max_total_bytes: 32 * 1024 * 1024,
            max_depth: 64,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SnapshotFileKind {
    Regular,
    Symlink,
}

impl SnapshotFileKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Regular => "regular",
            Self::Symlink => "symlink",
        }
    }

    fn hash_tag(self) -> u8 {
        match self {
            Self::Regular => 1,
            Self::Symlink => 2,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkillSnapshotFile {
    pub relative_path: String,
    pub kind: SnapshotFileKind,
    /// Unix executable state. False on platforms without Unix mode bits.
    pub executable: bool,
    /// Regular-file bytes, or the UTF-8 link target for a symlink entry.
    pub content: Vec<u8>,
}

impl SkillSnapshotFile {
    pub fn size(&self) -> u64 {
        self.content.len() as u64
    }

    pub fn symlink_target(&self) -> Option<&str> {
        (self.kind == SnapshotFileKind::Symlink)
            .then(|| std::str::from_utf8(&self.content).ok())
            .flatten()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkillSnapshot {
    pub name: String,
    /// Canonical effective content root. Never serialize this path to the UI.
    pub root: PathBuf,
    pub content_hash: String,
    pub files: Vec<SkillSnapshotFile>,
    pub total_bytes: u64,
}

impl SkillSnapshot {
    /// Materialize this captured snapshot into a new directory for read-only analysis.
    ///
    /// Internal symlinks are deliberately represented as small sentinel files instead
    /// of being recreated. This preserves their target as evidence without allowing a
    /// staged ACP workspace to escape through a link.
    pub fn materialize_to(&self, destination: &Path) -> Result<(), AppError> {
        if destination.symlink_metadata().is_ok() {
            return Err(AppError::Other(format!(
                "Snapshot destination already exists: {}",
                destination.display()
            )));
        }

        std::fs::create_dir_all(destination)?;
        let result = (|| -> Result<(), AppError> {
            for file in &self.files {
                let relative = normalized_relative_path(&file.relative_path)?;
                let output_path = destination.join(relative);
                if let Some(parent) = output_path.parent() {
                    std::fs::create_dir_all(parent)?;
                }

                match file.kind {
                    SnapshotFileKind::Regular => std::fs::write(output_path, &file.content)?,
                    SnapshotFileKind::Symlink => {
                        let target = file.symlink_target().ok_or_else(|| {
                            AppError::Other(format!(
                                "Snapshot symlink target is not UTF-8: {}",
                                file.relative_path
                            ))
                        })?;
                        std::fs::write(
                            output_path,
                            format!("SkillStar snapshot symlink (not followed): {target}\n"),
                        )?;
                    }
                }
            }
            Ok(())
        })();

        if result.is_err() {
            let _ = std::fs::remove_dir_all(destination);
        }
        result
    }
}

/// Capture the complete, bounded on-disk contents of one installed Skill.
///
/// The root hub link is resolved once, but links found inside the Skill are
/// recorded and never followed. The returned hash is computed from the exact
/// captured bytes, so callers can safely use the same snapshot for ACP input
/// and version matching without a hash/read time-of-check gap.
pub fn snapshot(name: &str) -> Result<SkillSnapshot, AppError> {
    snapshot_with_limits(name, SnapshotLimits::default())
}

/// Snapshot for an explicit read/generation action that may materialize a
/// legacy lazy Git worktree. Divergence detection must keep using [`snapshot`].
pub fn snapshot_materialized(name: &str) -> Result<SkillSnapshot, AppError> {
    validate_skill_name(name)?;
    let skill_entry = skillstar_core::infra::paths::hub_skills_dir().join(name);
    materialize_managed_worktree(&skill_entry)?;
    snapshot(name)
}

fn materialize_managed_worktree(skill_entry: &Path) -> Result<(), AppError> {
    // Repo-cache Skills are often links to a nested directory. In a historical
    // lazy checkout that target does not exist yet, so looking for `.git` at
    // the link target itself is insufficient; walk to the physical repo root
    // first and materialize the checkout there.
    let repo_root = if skillstar_core::infra::fs_ops::is_link(skill_entry) {
        crate::repo_link::repo_root_of(skill_entry)
    } else {
        git_ops::find_repo_root(skill_entry)
    };
    if let Some(repo_root) = repo_root {
        git_ops::ensure_worktree_checked_out(&repo_root).map_err(AppError::Anyhow)?;
    }
    Ok(())
}

pub fn snapshot_with_limits(name: &str, limits: SnapshotLimits) -> Result<SkillSnapshot, AppError> {
    validate_skill_name(name)?;
    let root = resolve_snapshot_root(name)?;
    snapshot_resolved_root(name, root, limits)
}

/// Capture an already-resolved managed source before an install mutates hub links.
pub(crate) fn snapshot_path(name: &str, root: &Path) -> Result<SkillSnapshot, AppError> {
    validate_skill_name(name)?;
    let root = std::fs::canonicalize(root).map_err(|error| {
        AppError::Other(format!(
            "Failed to resolve Skill source {}: {error}",
            root.display()
        ))
    })?;
    if !is_skill_content_root(&root) {
        return Err(AppError::Other(format!(
            "Skill source has no SKILL.md: {}",
            root.display()
        )));
    }
    snapshot_resolved_root(name, root, SnapshotLimits::default())
}

fn snapshot_resolved_root(
    name: &str,
    root: PathBuf,
    limits: SnapshotLimits,
) -> Result<SkillSnapshot, AppError> {
    let mut files = Vec::new();
    let mut total_bytes = 0u64;
    collect_snapshot_files(&root, &root, 0, limits, &mut files, &mut total_bytes)?;
    files.sort_by(|left, right| left.relative_path.cmp(&right.relative_path));

    let content_hash = snapshot_hash(&files);
    Ok(SkillSnapshot {
        name: name.to_string(),
        root,
        content_hash,
        files,
        total_bytes,
    })
}

pub(crate) fn validate_skill_name(name: &str) -> Result<(), AppError> {
    if name.is_empty()
        || name.contains(['/', '\\', '\0'])
        || name.contains(['<', '>', ':', '"', '|', '?', '*'])
        || name.chars().any(char::is_control)
        || name.ends_with(['.', ' '])
        || is_windows_reserved_name(name)
        || name == "."
        || name == ".."
        || !matches!(
            Path::new(name).components().collect::<Vec<_>>().as_slice(),
            [Component::Normal(_)]
        )
    {
        return Err(AppError::Other(format!("Invalid skill name: {name:?}")));
    }
    Ok(())
}

fn is_windows_reserved_name(name: &str) -> bool {
    let stem = name.split('.').next().unwrap_or(name).to_ascii_uppercase();
    matches!(stem.as_str(), "CON" | "PRN" | "AUX" | "NUL" | "CLOCK$")
        || stem.strip_prefix("COM").is_some_and(|suffix| {
            matches!(suffix, "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9")
        })
        || stem.strip_prefix("LPT").is_some_and(|suffix| {
            matches!(suffix, "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9")
        })
}

fn resolve_snapshot_root(name: &str) -> Result<PathBuf, AppError> {
    let hub_root = skillstar_core::infra::paths::hub_root();
    let skill_entry = skillstar_core::infra::paths::hub_skills_dir().join(name);
    if skill_entry.symlink_metadata().is_err() {
        return Err(not_found(name));
    }

    let canonical_hub = std::fs::canonicalize(&hub_root).map_err(|error| {
        AppError::Other(format!(
            "Failed to resolve SkillStar hub root {}: {error}",
            hub_root.display()
        ))
    })?;
    let canonical_entry = std::fs::canonicalize(&skill_entry).map_err(|error| {
        AppError::Other(format!(
            "Failed to resolve skill root {}: {error}",
            skill_entry.display()
        ))
    })?;
    ensure_within(&canonical_entry, &canonical_hub, "skill root")?;
    if !canonical_entry.is_dir() {
        return Err(AppError::Other(format!(
            "Skill root is not a directory: {}",
            canonical_entry.display()
        )));
    }

    let candidate = if is_skill_content_root(&canonical_entry) {
        Some(canonical_entry.clone())
    } else if let Some(source_folder) = lockfile_source_folder(name) {
        let relative = normalized_relative_path(&source_folder)?;
        let nested = canonical_entry.join(relative);
        if is_skill_content_root(&nested) {
            Some(nested)
        } else {
            find_nested_snapshot_root(&canonical_entry, name)?
        }
    } else {
        find_nested_snapshot_root(&canonical_entry, name)?
    }
    .ok_or_else(|| not_found(name))?;

    let canonical_candidate = std::fs::canonicalize(&candidate).map_err(|error| {
        AppError::Other(format!(
            "Failed to resolve effective skill root {}: {error}",
            candidate.display()
        ))
    })?;
    ensure_within(
        &canonical_candidate,
        &canonical_entry,
        "effective skill root",
    )?;
    ensure_within(&canonical_candidate, &canonical_hub, "effective skill root")?;
    if !is_skill_content_root(&canonical_candidate) {
        return Err(not_found(name));
    }
    Ok(canonical_candidate)
}

fn ensure_within(path: &Path, root: &Path, label: &str) -> Result<(), AppError> {
    if !path.starts_with(root) {
        return Err(AppError::Other(format!(
            "Resolved {label} escapes its managed root: {}",
            path.display()
        )));
    }
    Ok(())
}

fn is_skill_content_root(path: &Path) -> bool {
    path.is_dir() && path.join("SKILL.md").symlink_metadata().is_ok()
}

fn find_nested_snapshot_root(root: &Path, skill_name: &str) -> Result<Option<PathBuf>, AppError> {
    const SKIP_DIRS: &[&str] = &[".git", "node_modules", "target", "dist", "build", ".next"];

    let mut stack = vec![root.to_path_buf()];
    let mut matches = Vec::new();
    while let Some(dir) = stack.pop() {
        let mut entries = std::fs::read_dir(&dir)?.collect::<Result<Vec<_>, _>>()?;
        entries.sort_by_key(|entry| entry.file_name());

        // Push in reverse order so traversal remains lexicographic.
        for entry in entries.into_iter().rev() {
            let path = entry.path();
            if skillstar_core::infra::fs_ops::is_link(&path) {
                continue;
            }
            let metadata = entry.metadata()?;
            if !metadata.is_dir() {
                continue;
            }

            let dir_name = entry.file_name();
            let Some(dir_name) = dir_name.to_str() else {
                return Err(AppError::Other(format!(
                    "Skill path is not valid UTF-8: {}",
                    path.display()
                )));
            };
            if SKIP_DIRS.contains(&dir_name) {
                continue;
            }
            if dir_name == skill_name && is_skill_content_root(&path) {
                matches.push(path.clone());
            }
            stack.push(path);
        }
    }

    matches.sort();
    Ok(matches.into_iter().next())
}

fn collect_snapshot_files(
    root: &Path,
    dir: &Path,
    depth: usize,
    limits: SnapshotLimits,
    files: &mut Vec<SkillSnapshotFile>,
    total_bytes: &mut u64,
) -> Result<(), AppError> {
    if depth > limits.max_depth {
        return Err(AppError::Other(format!(
            "Skill snapshot exceeds maximum directory depth of {}",
            limits.max_depth
        )));
    }

    let mut entries = std::fs::read_dir(dir)?.collect::<Result<Vec<_>, _>>()?;
    entries.sort_by_key(|entry| entry.file_name());
    for entry in entries {
        let path = entry.path();
        if snapshot_entry_is_ignored(&entry.file_name()) {
            continue;
        }

        if skillstar_core::infra::fs_ops::is_link(&path) {
            let target = read_raw_link_target(&path)?;
            let target = target.to_str().ok_or_else(|| {
                AppError::Other(format!(
                    "Symlink target is not valid UTF-8: {}",
                    path.display()
                ))
            })?;
            push_snapshot_file(
                files,
                total_bytes,
                limits,
                SkillSnapshotFile {
                    relative_path: relative_path_string(root, &path)?,
                    kind: SnapshotFileKind::Symlink,
                    executable: false,
                    content: target.as_bytes().to_vec(),
                },
            )?;
            continue;
        }

        let metadata = entry.metadata()?;
        if metadata.is_dir() {
            collect_snapshot_files(root, &path, depth + 1, limits, files, total_bytes)?;
        } else if metadata.is_file() {
            if metadata.len() > limits.max_file_bytes {
                return Err(AppError::Other(format!(
                    "Skill snapshot file exceeds {} bytes: {}",
                    limits.max_file_bytes,
                    path.display()
                )));
            }
            push_snapshot_file(
                files,
                total_bytes,
                limits,
                SkillSnapshotFile {
                    relative_path: relative_path_string(root, &path)?,
                    kind: SnapshotFileKind::Regular,
                    executable: is_executable(&metadata),
                    content: std::fs::read(&path)?,
                },
            )?;
        } else {
            return Err(AppError::Other(format!(
                "Unsupported special file in skill snapshot: {}",
                path.display()
            )));
        }
    }
    Ok(())
}

#[cfg(unix)]
fn is_executable(metadata: &std::fs::Metadata) -> bool {
    use std::os::unix::fs::PermissionsExt as _;
    metadata.permissions().mode() & 0o111 != 0
}

#[cfg(not(unix))]
fn is_executable(_metadata: &std::fs::Metadata) -> bool {
    false
}

pub(crate) fn snapshot_entry_is_ignored(name: &std::ffi::OsStr) -> bool {
    let Some(name) = name.to_str() else {
        return false;
    };
    matches!(
        name,
        ".git" | ".skillstar" | ".DS_Store" | "Thumbs.db" | "desktop.ini"
    ) || name.starts_with("._")
        || name.ends_with('~')
        || name.ends_with(".swp")
        || name.ends_with(".swo")
        || name.ends_with(".tmp")
}

fn push_snapshot_file(
    files: &mut Vec<SkillSnapshotFile>,
    total_bytes: &mut u64,
    limits: SnapshotLimits,
    file: SkillSnapshotFile,
) -> Result<(), AppError> {
    if files.len() >= limits.max_files {
        return Err(AppError::Other(format!(
            "Skill snapshot exceeds maximum file count of {}",
            limits.max_files
        )));
    }
    let size = file.size();
    if size > limits.max_file_bytes {
        return Err(AppError::Other(format!(
            "Skill snapshot entry exceeds {} bytes: {}",
            limits.max_file_bytes, file.relative_path
        )));
    }
    let next_total = total_bytes
        .checked_add(size)
        .ok_or_else(|| AppError::Other("Skill snapshot byte count overflowed".to_string()))?;
    if next_total > limits.max_total_bytes {
        return Err(AppError::Other(format!(
            "Skill snapshot exceeds maximum total size of {} bytes",
            limits.max_total_bytes
        )));
    }
    *total_bytes = next_total;
    files.push(file);
    Ok(())
}

fn read_raw_link_target(path: &Path) -> std::io::Result<PathBuf> {
    #[cfg(windows)]
    {
        std::fs::read_link(path).or_else(|error| junction::get_target(path).or(Err(error)))
    }
    #[cfg(not(windows))]
    {
        std::fs::read_link(path)
    }
}

fn relative_path_string(root: &Path, path: &Path) -> Result<String, AppError> {
    let relative = path.strip_prefix(root).map_err(|_| {
        AppError::Other(format!(
            "Skill snapshot path escaped its root: {}",
            path.display()
        ))
    })?;
    let mut parts = Vec::new();
    for component in relative.components() {
        let Component::Normal(part) = component else {
            return Err(AppError::Other(format!(
                "Invalid relative path in skill snapshot: {}",
                relative.display()
            )));
        };
        let part = part.to_str().ok_or_else(|| {
            AppError::Other(format!(
                "Skill snapshot path is not valid UTF-8: {}",
                relative.display()
            ))
        })?;
        if part.contains(['/', '\\']) {
            return Err(AppError::Other(format!(
                "Ambiguous path component in skill snapshot: {part:?}"
            )));
        }
        parts.push(part);
    }
    if parts.is_empty() {
        return Err(AppError::Other("Skill snapshot path is empty".to_string()));
    }
    Ok(parts.join("/"))
}

fn normalized_relative_path(value: &str) -> Result<PathBuf, AppError> {
    if value.is_empty() || value.contains('\\') {
        return Err(AppError::Other(format!(
            "Invalid relative skill path: {value:?}"
        )));
    }
    let mut output = PathBuf::new();
    for part in value.split('/') {
        if part.is_empty() || part == "." || part == ".." || part.contains('\0') {
            return Err(AppError::Other(format!(
                "Invalid relative skill path: {value:?}"
            )));
        }
        output.push(part);
    }
    Ok(output)
}

fn snapshot_hash(files: &[SkillSnapshotFile]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(SNAPSHOT_HASH_DOMAIN);
    hasher.update((files.len() as u64).to_be_bytes());
    for file in files {
        let path = file.relative_path.as_bytes();
        hasher.update((path.len() as u64).to_be_bytes());
        hasher.update(path);
        hasher.update([file.kind.hash_tag()]);
        hasher.update([u8::from(file.executable)]);
        hasher.update((file.content.len() as u64).to_be_bytes());
        hasher.update(&file.content);
    }
    format_sha256(hasher.finalize().as_slice())
}

fn format_sha256(digest: &[u8]) -> String {
    let mut output = String::with_capacity("sha256:".len() + digest.len() * 2);
    output.push_str("sha256:");
    for byte in digest {
        use std::fmt::Write as _;
        let _ = write!(output, "{byte:02x}");
    }
    output
}

pub fn read_raw(name: &str) -> Result<String, AppError> {
    validate_skill_name(name)?;
    let skill_dir = skillstar_core::infra::paths::hub_skills_dir().join(name);
    let effective_dir = resolve_content_dir(name).unwrap_or_else(|| resolve_skill_dir(&skill_dir));
    let skill_md = effective_dir.join("SKILL.md");
    if !skill_md.exists() {
        return Err(not_found(name));
    }
    Ok(std::fs::read_to_string(skill_md)?)
}

pub fn delete_local(name: &str) -> Result<(), AppError> {
    validate_skill_name(name)?;
    let _transaction_guard =
        crate::skill_update::acquire_update_transaction_lock().map_err(|error| {
            AppError::Other(format!("Unable to lock local Skill deletion: {error}"))
        })?;
    crate::shared_channels::ensure_generic_skill_mutation_allowed(name)
        .map_err(|error| AppError::Other(error.to_string()))?;
    if !local_skill::is_local_skill(name) {
        return Err(AppError::Other(format!(
            "Skill '{name}' is not a local Skill and cannot be deleted through the local authoring flow"
        )));
    }
    local_skill::delete(name).map_err(AppError::Anyhow)?;
    installed_skill::invalidate_cache();
    Ok(())
}

pub fn migrate_local_skills() -> Result<u32, AppError> {
    local_skill::migrate_existing().map_err(AppError::Anyhow)
}

pub fn list_files(name: &str) -> Result<Vec<String>, AppError> {
    validate_skill_name(name)?;
    let skill_dir = skillstar_core::infra::paths::hub_skills_dir().join(name);
    if !skill_dir.exists() {
        return Err(not_found(name));
    }

    let effective_dir = resolve_content_dir(name).unwrap_or_else(|| resolve_skill_dir(&skill_dir));
    let mut files = Vec::new();
    collect_files_recursive(&effective_dir, &effective_dir, &mut files);
    files.sort();
    Ok(files)
}

pub fn read(name: &str) -> Result<SkillContent, AppError> {
    validate_skill_name(name)?;
    let skill_dir = skillstar_core::infra::paths::hub_skills_dir().join(name);
    if !skill_dir.exists() {
        return Err(not_found(name));
    }

    materialize_managed_worktree(&skill_dir)?;
    let effective_dir = resolve_content_dir(name).unwrap_or(skill_dir);
    let skill_path = effective_dir.join("SKILL.md");
    if !skill_path.exists() {
        return Err(not_found(name));
    }

    let full_content = std::fs::read_to_string(skill_path)?;
    Ok(parse_skill_content(name.to_string(), full_content))
}

pub fn update(name: &str, content: &str) -> Result<(), AppError> {
    validate_skill_name(name)?;
    let _transaction_guard =
        crate::skill_update::acquire_update_transaction_lock().map_err(|error| {
            AppError::Other(format!("Unable to lock Skill content update: {error}"))
        })?;
    crate::shared_channels::ensure_generic_skill_mutation_allowed(name)
        .map_err(|error| AppError::Other(error.to_string()))?;
    let skill_dir = skillstar_core::infra::paths::hub_skills_dir().join(name);
    if !skill_dir.exists() {
        return Err(not_found(name));
    }

    let effective_dir = resolve_content_dir(name).unwrap_or(skill_dir);
    let skill_path = effective_dir.join("SKILL.md");
    if !skill_path.exists() {
        return Err(not_found(name));
    }

    skillstar_core::infra::fs_ops::atomic_write(&skill_path, content.as_bytes())?;
    Ok(())
}

pub(crate) fn resolve_content_dir(name: &str) -> Option<PathBuf> {
    let skill_dir = skillstar_core::infra::paths::hub_skills_dir().join(name);
    if !skill_dir.exists() {
        return None;
    }

    let effective_dir = resolve_skill_dir(&skill_dir);
    if effective_dir.join("SKILL.md").exists() {
        return Some(effective_dir);
    }

    if let Some(source_folder) = lockfile_source_folder(name) {
        let nested = effective_dir.join(source_folder);
        if nested.join("SKILL.md").exists() {
            return Some(nested);
        }
    }

    find_nested_skill_dir_by_name(&effective_dir, name)
}

fn resolve_skill_dir(skill_dir: &Path) -> PathBuf {
    if !skillstar_core::infra::fs_ops::is_link(skill_dir) {
        return skill_dir.to_path_buf();
    }

    skillstar_core::infra::fs_ops::read_link_resolved(skill_dir)
        .unwrap_or_else(|_| skill_dir.to_path_buf())
}

fn lockfile_source_folder(skill_name: &str) -> Option<String> {
    let lock_path = lockfile::lockfile_path();
    let lockfile = lockfile::Lockfile::load(&lock_path).ok()?;
    lockfile
        .skills
        .into_iter()
        .find(|entry| entry.name == skill_name)
        .and_then(|entry| entry.source_folder)
}

fn find_nested_skill_dir_by_name(root: &Path, skill_name: &str) -> Option<PathBuf> {
    const SKIP_DIRS: &[&str] = &[".git", "node_modules", "target", "dist", "build", ".next"];

    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let entries = match std::fs::read_dir(&dir) {
            Ok(entries) => entries,
            Err(_) => continue,
        };

        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }

            let dir_name = entry.file_name().to_string_lossy().to_string();
            if SKIP_DIRS.iter().any(|skip| *skip == dir_name) {
                continue;
            }
            if dir_name == skill_name && path.join("SKILL.md").exists() {
                return Some(path);
            }
            stack.push(path);
        }
    }

    None
}

fn collect_files_recursive(root: &Path, dir: &Path, files: &mut Vec<String>) {
    let entries = match std::fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(_) => return,
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let file_name = entry.file_name();
        let name = file_name.to_string_lossy();
        if name == ".git" {
            continue;
        }

        if path.is_dir() {
            collect_files_recursive(root, &path, files);
        } else if let Ok(relative) = path.strip_prefix(root) {
            files.push(relative.to_string_lossy().replace('\\', "/"));
        }
    }
}

fn not_found(name: &str) -> AppError {
    AppError::SkillNotFound {
        name: name.to_string(),
    }
}

#[cfg(test)]
#[path = "content/tests.rs"]
mod tests;
