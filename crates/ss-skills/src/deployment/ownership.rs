//! Which Agent/Project skill entries SkillStar owns — the only judgment.
//!
//! A link is owned only when it resolves to exactly `<root>/<expected_name>`
//! inside the canonical skills root, the local-authoring directory, or the
//! legacy hub's `skills/` directory, and that path is a directory. A real
//! directory is owned only when it carries a [`DEPLOY_MARKER`] whose source
//! name matches and whose `contentHash` still matches the directory. An
//! unmarked directory is foreign when it contains a comparison exclusion
//! (`.git`, `.skillstar`, the marker) or its file set differs from the
//! canonical Skill. Project trees never treat an unmarked directory as owned.

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

/// Entries never carried into a copy deployment, and never ignored when
/// deciding whether an unmarked directory is a user's own tree.
const DEPLOY_COPY_EXCLUDES: &[&str] = &[".git", ".skillstar", DEPLOY_MARKER];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ownership {
    Missing,
    /// Link/junction into a SkillStar-managed root; `alive` when it resolves.
    Link {
        alive: bool,
    },
    /// Directory copy carrying a marker for the expected Skill.
    Copy,
    /// Present but not SkillStar's to touch.
    Foreign,
}

impl Ownership {
    pub fn is_owned(self) -> bool {
        matches!(self, Self::Link { .. } | Self::Copy)
    }
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct DeployMarker {
    version: u32,
    source: String,
    content_hash: String,
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
            Some(marker) if marker.source == expected_name => copy_if_hash_matches(target, &marker),
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
    match dir_content_hash(target) {
        Ok(actual) if actual == marker.content_hash => Ownership::Copy,
        _ => Ownership::Foreign,
    }
}

/// A deployment link is owned only when it points at that Skill's own
/// directory: `<canonical>/<name>`, `<local>/<name>`, or legacy
/// `<hub>/skills/<name>`. A link into a subdirectory, a sibling Skill, or
/// anywhere else in the hub is the user's.
fn link_ownership(target: &Path, expected_name: &str) -> Ownership {
    let Ok(raw) = fs_ops::read_link_resolved(target) else {
        return Ownership::Foreign;
    };
    let resolved = fs_ops::canonicalize_existing_prefix(&raw);
    if !exact_managed_skill(&resolved, expected_name) {
        return Ownership::Foreign;
    }
    if resolved.exists() && !resolved.is_dir() {
        return Ownership::Foreign;
    }
    Ownership::Link {
        alive: resolved.is_dir(),
    }
}

fn exact_managed_skill(resolved: &Path, name: &str) -> bool {
    [
        paths::agents_skills_root().join(name),
        paths::local_skills_dir().join(name),
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
            if path.is_dir() && !fs_ops::is_link(&path) && walk(&path) {
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

/// Copy `source` to `target` as an owned deployment of Skill `name`. Links in
/// the Skill are only followed while they stay inside the Skill folder.
pub(crate) fn deploy_copy(source: &Path, target: &Path, name: &str) -> Result<()> {
    refuse_existing(target)?;
    let boundary = std::fs::canonicalize(source)
        .with_context(|| format!("Failed to resolve deploy source {}", source.display()))?;
    let mut staged = materialize::StagedReplace::stage(target, |staging| {
        materialize::copy_confined(&boundary, staging, &boundary, DEPLOY_COPY_EXCLUDES)?;
        let marker = DeployMarker {
            version: 1,
            source: name.to_string(),
            content_hash: dir_content_hash(staging)?,
        };
        std::fs::write(
            staging.join(DEPLOY_MARKER),
            serde_json::to_vec_pretty(&marker)?,
        )?;
        Ok(())
    })?;
    staged.swap()?;
    staged.commit();
    Ok(())
}

/// Mark a hand-built test fixture as a copy SkillStar deployed.
#[cfg(test)]
pub(crate) fn mark_copy_for_test(dir: &Path, name: &str) {
    let marker = DeployMarker {
        version: 1,
        source: name.to_string(),
        content_hash: dir_content_hash(dir).unwrap_or_default(),
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
        Ownership::Foreign => Ok(Removal::Foreign),
        Ownership::Link { .. } | Ownership::Copy => {
            materialize::remove_entry(target)?;
            Ok(Removal::Removed)
        }
    }
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
        if DEPLOY_COPY_EXCLUDES
            .iter()
            .any(|excluded| name == *excluded)
        {
            continue;
        }
        let path = entry.path();
        if path.is_dir() && !fs_ops::is_link(&path) {
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
