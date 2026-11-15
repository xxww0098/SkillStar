//! Which Agent/Project skill entries SkillStar owns — the only judgment.
//!
//! A link is owned only when it resolves to exactly `<root>/<expected_name>`
//! inside the canonical skills root, the local-authoring directory, or the
//! legacy hub's `skills/` directory, and that path is a directory. A dangling
//! link whose text is still the pre-D-100 `~/.agents/skills/<name>` is the
//! same deployment after that directory was removed. A real directory is owned
//! only when it carries a [`DEPLOY_MARKER`] whose source name matches and
//! whose `contentHash` still matches the directory. An unmarked directory is
//! foreign when it contains a comparison exclusion (`.git`, `.skillstar`, the
//! marker) or its file set differs from the canonical Skill. Project trees
//! never treat an unmarked directory as owned.

use std::collections::BTreeSet;
use std::io::Read;
use std::path::Path;

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use ss_core::infra::{fs_ops, paths};
use tracing::warn;

use crate::materialize;

/// Marker file at the root of every copy deployment.
pub const DEPLOY_MARKER: &str = ".skillstar-deploy.json";
const DEPLOY_MARKER_VERSION: u32 = 1;

/// Entries never carried into a copy deployment, and never ignored when
/// deciding whether an unmarked directory is a user's own tree.
const DEPLOY_COPY_EXCLUDES: &[&str] = &[".git", ".skillstar", DEPLOY_MARKER];

/// Combined exclusions applied when staging a Skill copy deployment:
/// deploy metadata + runtime environment directories (`node_modules`, `.venv`, ...).
const DEPLOY_STAGE_EXCLUDES: &[&str] = &[
    ".git",
    ".skillstar",
    DEPLOY_MARKER,
    "node_modules",
    ".venv",
    "venv",
    "__pycache__",
    ".pytest_cache",
    ".mypy_cache",
    ".ruff_cache",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ownership {
    Missing,
    /// Link/junction into a SkillStar-managed root; `alive` when it resolves.
    Link {
        alive: bool,
    },
    /// Directory copy carrying a marker for the expected Skill whose files
    /// still match what SkillStar deployed.
    Copy,
    /// Marked copy deployed by SkillStar that has been modified locally in the
    /// agent/project directory and must not be silently overwritten or wiped.
    SkillStarModifiedCopy,
    /// Present but not SkillStar's to touch.
    Foreign,
}

impl Ownership {
    pub fn is_owned(self) -> bool {
        matches!(self, Self::Link { .. } | Self::Copy)
    }

    pub fn is_modified_copy(self) -> bool {
        matches!(self, Self::SkillStarModifiedCopy)
    }
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct DeployMarker {
    version: u32,
    source: String,
    content_hash: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    tree_max_mtime_ms: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    tree_max_mtime_ns: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    file_count: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    total_bytes: Option<u64>,
}

/// Agent directories may still recognise a pre-marker copy. Project
/// directories never do: an unmarked tree there is reported and kept.
#[derive(Clone, Copy)]
enum UnmarkedCopy {
    LegacyIdentical,
    ReportOnly,
}

/// Classify `target`, the entry for Skill `expected_name` in an Agent or
/// mirror skills directory.
pub fn owned_deployment(target: &Path, expected_name: &str) -> Ownership {
    classify(target, expected_name, UnmarkedCopy::LegacyIdentical)
}

/// Project skills directories: a directory without a matching marker is the
/// user's, even when its bytes equal the canonical Skill.
pub(crate) fn owned_project_deployment(target: &Path, expected_name: &str) -> Ownership {
    classify(target, expected_name, UnmarkedCopy::ReportOnly)
}

fn classify(target: &Path, expected_name: &str, unmarked: UnmarkedCopy) -> Ownership {
    if fs_ops::is_link(target) {
        return link_ownership(target, expected_name);
    }
    match target.symlink_metadata() {
        Err(_) => Ownership::Missing,
        Ok(metadata) if metadata.is_dir() => match read_marker(target) {
            Some(marker)
                if marker.version == DEPLOY_MARKER_VERSION && marker.source == expected_name =>
            {
                copy_if_hash_matches(target, &marker)
            }
            Some(_) => Ownership::Foreign,
            None => match unmarked {
                UnmarkedCopy::ReportOnly => Ownership::Foreign,
                UnmarkedCopy::LegacyIdentical
                    if is_identical_unmarked_copy(target, expected_name) =>
                {
                    Ownership::Copy
                }
                UnmarkedCopy::LegacyIdentical => Ownership::Foreign,
            },
        },
        Ok(_) => Ownership::Foreign,
    }
}

fn copy_if_hash_matches(target: &Path, marker: &DeployMarker) -> Ownership {
    if marker.content_hash.is_empty() {
        return Ownership::Foreign;
    }
    if let (Some(expected_ms), Some(expected_count)) =
        (marker.tree_max_mtime_ms, marker.file_count)
        && let Ok(stats) = dir_tree_stats(target)
        && now_unix_ms().saturating_sub(stats.max_mtime_ms) >= 2_000
        && stats.max_mtime_ms == expected_ms
        && stats.file_count == expected_count
        && marker
            .tree_max_mtime_ns
            .is_none_or(|ns| ns == stats.max_mtime_ns)
        && marker
            .total_bytes
            .is_none_or(|bytes| bytes == stats.total_bytes)
    {
        return Ownership::Copy;
    }
    match dir_content_hash(target) {
        Ok(actual) if actual == marker.content_hash => Ownership::Copy,
        Ok(_) => Ownership::SkillStarModifiedCopy,
        Err(_) => Ownership::Foreign,
    }
}

/// A deployment link is owned only when it points at that Skill's own
/// directory: `<canonical>/<name>`, `<local>/<name>`, or legacy
/// `<hub>/skills/<name>`. A dangling link to the removed pre-D-100
/// `~/.agents/skills/<name>` is the same deployment and may be replaced.
/// A link into a subdirectory, a sibling Skill, or anywhere else is the
/// user's. A pre-D-100 path that still exists was left behind on a name
/// conflict and stays foreign.
fn link_ownership(target: &Path, expected_name: &str) -> Ownership {
    let Ok(raw) = fs_ops::read_link_resolved(target) else {
        return Ownership::Foreign;
    };
    let resolved = fs_ops::canonicalize_existing_prefix(&raw);
    if exact_managed_skill(&resolved, expected_name) {
        if resolved.exists() && !resolved.is_dir() {
            return Ownership::Foreign;
        }
        return Ownership::Link {
            alive: resolved.is_dir(),
        };
    }
    if dangling_legacy_agents_link(&raw, expected_name) {
        return Ownership::Link { alive: false };
    }
    Ownership::Foreign
}

/// `true` when `raw` is exactly `~/.agents/skills/<expected_name>` and that
/// path is gone. Canonicalize cannot see this: the missing tail is glued
/// back onto whatever prefix still exists, so it no longer matches a
/// managed root.
fn dangling_legacy_agents_link(raw: &Path, expected_name: &str) -> bool {
    let legacy = paths::legacy_agents_skills_root().join(expected_name);
    if lexical_path(raw) != lexical_path(&legacy) {
        return false;
    }
    matches!(
        legacy.symlink_metadata(),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound
    )
}

fn lexical_path(path: &Path) -> std::path::PathBuf {
    let mut out = std::path::PathBuf::new();
    for component in path.components() {
        match component {
            std::path::Component::ParentDir => {
                out.pop();
            }
            std::path::Component::CurDir => {}
            other => out.push(other.as_os_str()),
        }
    }
    out
}

fn exact_managed_skill(resolved: &Path, name: &str) -> bool {
    [
        paths::agents_skills_root().join(name),
        paths::legacy_local_skills_dir().join(name),
        paths::legacy_hub_root().join("skills").join(name),
    ]
    .iter()
    .any(|candidate| fs_ops::canonicalize_existing_prefix(candidate) == *resolved)
}

/// Copies deployed before the marker existed are recognised only while every
/// compared file matches and the target contains none of the names a copy
/// deployment skips. A checkout with `.git` or `.skillstar` is the user's.
fn is_identical_unmarked_copy(target: &Path, name: &str) -> bool {
    if contains_comparison_exclude(target) {
        return false;
    }
    let source = paths::agents_skills_root().join(name);
    let (Ok(source), Ok(target)) = (
        std::fs::canonicalize(&source),
        std::fs::canonicalize(target),
    ) else {
        return false;
    };
    if source == target || !source.is_dir() {
        return false;
    }
    let (Ok(source_files), Ok(target_files)) = (hashed_file_set(&source), hashed_file_set(&target))
    else {
        return false;
    };
    if source_files != target_files {
        return false;
    }
    matches!(
        (dir_content_hash(&source), dir_content_hash(&target)),
        (Ok(left), Ok(right)) if left == right
    )
}

fn is_deploy_excluded_entry(name: &std::ffi::OsStr, is_dir: bool) -> bool {
    DEPLOY_COPY_EXCLUDES
        .iter()
        .any(|excluded| name == *excluded)
        || crate::runtime_ignore::should_ignore_skill_entry(name, is_dir)
}

/// `true` when `dir` contains a skipped name, or cannot be listed. An empty
/// `.git` directory still counts: it adds no files, so a file-set compare
/// would otherwise call the checkout identical.
fn contains_comparison_exclude(dir: &Path) -> bool {
    fn walk(dir: &Path) -> bool {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return true;
        };
        for entry in entries.flatten() {
            let name = entry.file_name();
            if DEPLOY_COPY_EXCLUDES
                .iter()
                .any(|excluded| name == *excluded)
            {
                return true;
            }
            let path = entry.path();
            let is_dir = path.is_dir() && !fs_ops::is_link(&path);
            if is_dir && crate::runtime_ignore::should_ignore_skill_entry(&name, true) {
                continue;
            }
            if is_dir && walk(&path) {
                return true;
            }
        }
        false
    }
    walk(dir)
}

fn read_marker(dir: &Path) -> Option<DeployMarker> {
    let content = std::fs::read_to_string(dir.join(DEPLOY_MARKER)).ok()?;
    serde_json::from_str::<DeployMarker>(&content).ok()
}

/// Content hash a copy deployment recorded when it was written; `None` for an
/// unmarked copy or a marker without one.
pub(crate) fn marker_content_hash(dir: &Path) -> Option<String> {
    read_marker(dir)
        .filter(|marker| marker.version == DEPLOY_MARKER_VERSION)
        .map(|marker| marker.content_hash)
        .filter(|hash| !hash.is_empty())
}

/// Whether an Agent's Global skills directory *is* the canonical root
/// (`~/.skillstar/data/skills/installed`) or a directory inside it. Such
/// Agents read installed Skills directly; a deployment into that tree would
/// be a self-link or would delete canonical content.
pub fn targets_canonical_root(dir: &Path) -> bool {
    if dir.as_os_str().is_empty() {
        return false;
    }
    let dir = fs_ops::canonicalize_existing_prefix(dir);
    let root = fs_ops::canonicalize_existing_prefix(&paths::agents_skills_root());
    dir == root || dir.starts_with(&root)
}

/// Link `target` to `source`, falling back to a marked copy when the platform
/// refuses links. Returns `true` for the copy fallback.
pub(crate) fn deploy_link_or_copy(source: &Path, target: &Path, name: &str) -> Result<bool> {
    refuse_existing(target)?;
    match fs_ops::create_symlink(source, target) {
        Ok(()) => Ok(false),
        Err(_) => {
            deploy_copy(source, target, name)?;
            Ok(true)
        }
    }
}

/// Deploy `source` to `target` respecting `mode` (`symlink` vs `copy`) and
/// `disallow_junction` (strict symlink without Windows junction fallback before
/// degrading to copy). Returns `true` when a copy was materialized.
pub(crate) fn deploy_for_agent(
    source: &Path,
    target: &Path,
    name: &str,
    mode: crate::agents::AgentDeployMode,
    disallow_junction: bool,
) -> Result<bool> {
    match mode {
        crate::agents::AgentDeployMode::Copy => {
            deploy_copy(source, target, name)?;
            Ok(true)
        }
        crate::agents::AgentDeployMode::Symlink if disallow_junction => {
            refuse_existing(target)?;
            match fs_ops::create_symlink_strict(source, target) {
                Ok(()) => Ok(false),
                Err(_) => {
                    deploy_copy(source, target, name)?;
                    Ok(true)
                }
            }
        }
        crate::agents::AgentDeployMode::Symlink => deploy_link_or_copy(source, target, name),
    }
}

/// Copy `source` to `target` as an owned deployment of Skill `name`. Links in
/// the Skill are only followed while they stay inside the Skill folder.
pub(crate) fn deploy_copy(source: &Path, target: &Path, name: &str) -> Result<()> {
    refuse_existing(target)?;
    let mut staged = stage_copy(source, target, name)?;
    refuse_existing(target)?;
    staged.swap()?;
    staged.commit();
    Ok(())
}

/// Copy a skill tree excluding deploy metadata and runtime artifacts.
pub(crate) fn copy_clean_skill_tree(source: &Path, dest: &Path) -> Result<()> {
    let boundary = std::fs::canonicalize(source)
        .with_context(|| format!("Failed to resolve skill source {}", source.display()))?;
    materialize::copy_confined(&boundary, dest, &boundary, DEPLOY_STAGE_EXCLUDES)
}

/// Replace only an unchanged, marked Project copy. The complete replacement
/// and its marker are staged before checking ownership and the old baseline
/// again, so a staging failure or an external edit leaves the live copy intact.
/// Callers serialize Project writes; refreshing always preserves copy mode.
pub(crate) fn refresh_project_copy(source: &Path, target: &Path, name: &str) -> Result<()> {
    let baseline = project_copy_baseline(target, name)?;
    let staged = stage_copy(source, target, name)?;
    commit_project_copy(staged, target, name, &baseline)
}

/// Explicitly switch an unchanged, owned Project copy to a link, or a marked
/// copy when the platform refuses links. Returns `true` for the copy fallback.
/// Callers serialize Project writes; staging and revalidation leave the live
/// copy intact until the complete replacement is ready to swap.
pub(crate) fn replace_project_copy_with_link_or_copy(
    source: &Path,
    target: &Path,
    name: &str,
) -> Result<bool> {
    let baseline = project_copy_baseline(target, name)?;
    let mut was_copy = false;
    let staged = materialize::StagedReplace::stage(target, |staging| {
        was_copy = deploy_link_or_copy(source, staging, name)?;
        Ok(())
    })?;
    commit_project_copy(staged, target, name, &baseline)?;
    Ok(was_copy)
}

fn project_copy_baseline(target: &Path, name: &str) -> Result<String> {
    if owned_project_deployment(target, name) != Ownership::Copy {
        bail!(
            "Destination is not an owned Project copy, refusing to replace: {}",
            target.display()
        );
    }
    dir_content_hash(target)
}

fn commit_project_copy(
    mut staged: materialize::StagedReplace,
    target: &Path,
    name: &str,
    baseline: &str,
) -> Result<()> {
    #[cfg(test)]
    if let Some(probe) = PROJECT_COPY_STAGED_PROBE.with(|cell| cell.take()) {
        probe(target);
    }
    if owned_project_deployment(target, name) != Ownership::Copy
        || dir_content_hash(target)? != baseline
    {
        bail!(
            "Project copy changed while staging, refusing to overwrite: {}",
            target.display()
        );
    }
    staged.swap()?;
    staged.commit();
    Ok(())
}

#[cfg(test)]
thread_local! {
    static PROJECT_COPY_STAGED_PROBE: std::cell::Cell<Option<fn(&Path)>> = const { std::cell::Cell::new(None) };
}

/// Build a confined copy and its marker without touching the live destination.
fn stage_copy(source: &Path, target: &Path, name: &str) -> Result<materialize::StagedReplace> {
    let boundary = std::fs::canonicalize(source)
        .with_context(|| format!("Failed to resolve deploy source {}", source.display()))?;
    materialize::StagedReplace::stage(target, |staging| {
        materialize::copy_confined(&boundary, staging, &boundary, DEPLOY_STAGE_EXCLUDES)?;
        let stats = dir_tree_stats(staging)?;
        let marker = DeployMarker {
            version: DEPLOY_MARKER_VERSION,
            source: name.to_string(),
            content_hash: dir_content_hash(staging)?,
            tree_max_mtime_ms: Some(stats.max_mtime_ms),
            tree_max_mtime_ns: Some(stats.max_mtime_ns),
            file_count: Some(stats.file_count),
            total_bytes: Some(stats.total_bytes),
        };
        std::fs::write(
            staging.join(DEPLOY_MARKER),
            serde_json::to_vec_pretty(&marker)?,
        )?;
        Ok(())
    })
}

/// Mark a hand-built test fixture as a copy SkillStar deployed.
#[cfg(any(test, feature = "test-support"))]
pub fn mark_copy_for_test(dir: &Path, name: &str) {
    let stats = dir_tree_stats(dir).unwrap_or_default();
    let marker = DeployMarker {
        version: DEPLOY_MARKER_VERSION,
        source: name.to_string(),
        content_hash: dir_content_hash(dir).unwrap_or_default(),
        tree_max_mtime_ms: Some(stats.max_mtime_ms),
        tree_max_mtime_ns: Some(stats.max_mtime_ns),
        file_count: Some(stats.file_count),
        total_bytes: Some(stats.total_bytes),
    };
    std::fs::write(
        dir.join(DEPLOY_MARKER),
        serde_json::to_vec(&marker).unwrap(),
    )
    .unwrap();
}

fn refuse_existing(target: &Path) -> Result<()> {
    if target.symlink_metadata().is_ok() || fs_ops::is_link(target) {
        bail!(
            "Destination already exists, refusing to overwrite: {}",
            target.display()
        );
    }
    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Removal {
    Removed,
    Missing,
    Foreign,
}

/// Remove `target` only when [`owned_deployment`] says SkillStar owns it.
pub(crate) fn remove_owned(target: &Path, expected_name: &str) -> Result<Removal> {
    remove_with(target, expected_name, UnmarkedCopy::LegacyIdentical)
}

/// Remove a project entry SkillStar owns. An unmarked directory is left in
/// place and reported; it is never treated as a legacy copy.
pub(crate) fn remove_project_owned(target: &Path, expected_name: &str) -> Result<Removal> {
    let removal = remove_with(target, expected_name, UnmarkedCopy::ReportOnly)?;
    if removal == Removal::Foreign && target.symlink_metadata().is_ok() {
        warn!(
            target: "sync",
            path = %target.display(),
            skill = expected_name,
            "project entry is not a SkillStar deployment; left in place"
        );
    }
    Ok(removal)
}

fn remove_with(target: &Path, expected_name: &str, unmarked: UnmarkedCopy) -> Result<Removal> {
    match classify(target, expected_name, unmarked) {
        Ownership::Missing => Ok(Removal::Missing),
        Ownership::Foreign | Ownership::SkillStarModifiedCopy => Ok(Removal::Foreign),
        Ownership::Link { .. } | Ownership::Copy => {
            materialize::remove_entry(target)?;
            Ok(Removal::Removed)
        }
    }
}

#[derive(Debug, Default, Clone, Copy)]
struct TreeStats {
    file_count: u64,
    total_bytes: u64,
    max_mtime_ms: u64,
    max_mtime_ns: u64,
}

fn now_unix_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| u64::try_from(d.as_millis()).unwrap_or(u64::MAX))
        .unwrap_or(0)
}

fn mtime_parts(meta: &std::fs::Metadata) -> (u64, u64) {
    meta.modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| {
            let ms = u64::try_from(d.as_millis()).unwrap_or(u64::MAX);
            let ns = u64::try_from(d.as_nanos()).unwrap_or(u64::MAX);
            (ms, ns)
        })
        .unwrap_or((0, 0))
}

fn dir_tree_stats(dir: &Path) -> Result<TreeStats> {
    fn walk(dir: &Path, stats: &mut TreeStats) -> Result<()> {
        let entries = std::fs::read_dir(dir)
            .with_context(|| format!("failed to list '{}' while stating", dir.display()))?;
        for entry in entries {
            let entry = entry?;
            let path = entry.path();
            let is_link = fs_ops::is_link(&path);
            let meta = match entry.metadata() {
                Ok(meta) => meta,
                Err(_) => continue,
            };
            let is_dir = meta.is_dir() && !is_link;
            if is_deploy_excluded_entry(&entry.file_name(), is_dir) {
                continue;
            }
            if is_dir {
                walk(&path, stats)?;
            } else if meta.is_file() && !is_link {
                stats.file_count += 1;
                stats.total_bytes = stats.total_bytes.saturating_add(meta.len());
                let (ms, ns) = mtime_parts(&meta);
                let best = (stats.max_mtime_ms, stats.max_mtime_ns).max((ms, ns));
                stats.max_mtime_ms = best.0;
                stats.max_mtime_ns = best.1;
            }
        }
        Ok(())
    }

    let mut stats = TreeStats::default();
    walk(dir, &mut stats)?;
    Ok(stats)
}

fn hashed_file_set(dir: &Path) -> Result<BTreeSet<String>> {
    let mut files = BTreeSet::new();
    collect_files(dir, dir, &mut files)?;
    Ok(files)
}

fn collect_files(base: &Path, dir: &Path, out: &mut BTreeSet<String>) -> Result<()> {
    let entries = std::fs::read_dir(dir)
        .with_context(|| format!("failed to list '{}' while hashing", dir.display()))?;
    for entry in entries {
        let entry = entry?;
        let name = entry.file_name();
        let path = entry.path();
        let is_dir = path.is_dir() && !fs_ops::is_link(&path);
        if is_deploy_excluded_entry(&name, is_dir) {
            continue;
        }
        if is_dir {
            collect_files(base, &path, out)?;
        } else if path.is_file()
            && let Ok(rel) = path.strip_prefix(base)
        {
            out.insert(rel.to_string_lossy().replace('\\', "/"));
        }
    }
    Ok(())
}

/// Deterministic SHA-256 over relative paths and file bytes, skipping what a
/// copy deployment never carries, so a copy and its source compare equal.
///
/// Each path and each file is length-prefixed, and file bytes are read in
/// chunks, so `ab`+`c` does not collide with `a`+`bc` and a large file is
/// not buffered whole.
pub(crate) fn dir_content_hash(dir: &Path) -> Result<String> {
    let files = hashed_file_set(dir)?;
    let mut hasher = Sha256::new();
    hasher.update((files.len() as u64).to_le_bytes());
    for rel in &files {
        hasher.update((rel.len() as u64).to_le_bytes());
        hasher.update(rel.as_bytes());
        hash_file(&mut hasher, &dir.join(rel))?;
    }
    Ok(hasher
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect())
}

fn hash_file(hasher: &mut Sha256, path: &Path) -> Result<()> {
    let mut file = std::fs::File::open(path)
        .with_context(|| format!("failed to read '{}' while hashing", path.display()))?;
    let declared = file.metadata().map(|meta| meta.len()).unwrap_or(0);
    hasher.update(declared.to_le_bytes());
    let mut buf = [0u8; 64 * 1024];
    let mut remaining = declared;
    let mut hashed = 0u64;
    while remaining > 0 {
        let want = usize::try_from(remaining.min(buf.len() as u64)).unwrap_or(buf.len());
        let read = file
            .read(&mut buf[..want])
            .with_context(|| format!("failed to read '{}' while hashing", path.display()))?;
        if read == 0 {
            break;
        }
        hasher.update(&buf[..read]);
        let read = read as u64;
        remaining = remaining.saturating_sub(read);
        hashed += read;
    }
    hasher.update(hashed.to_le_bytes());
    Ok(())
}

#[cfg(test)]
#[path = "ownership_tests.rs"]
mod tests;
