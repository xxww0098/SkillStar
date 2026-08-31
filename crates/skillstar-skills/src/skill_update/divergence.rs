//! Why an update stops, and what the user is allowed to do about it.
//!
//! Two different problems stop an update, and conflating them is what makes a
//! dialog impossible to answer. A Skill whose files no longer match the
//! lockfile baseline has a clean upstream state to go back to, so it can be
//! kept as a copy or thrown away. A Skill its source no longer ships has no
//! upstream state at all: `git checkout HEAD -- <path>` has nothing to
//! restore, so "discard" is not an exit — only keeping a copy or removing it
//! is. Classification and the resolutions it permits therefore live together.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::Path;

use crate::git::ops as git_ops;
use crate::lockfile::LockEntry;
use crate::update_checker::tracked_update_ref;
use crate::{content, lockfile, repo_link};

use super::plan;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum LocalDivergenceReason {
    ContentChanged,
    BaselineMissing,
    SnapshotFailed,
    /// The tracked source no longer ships this Skill, but the update stopped
    /// while its content is still on disk. A copy can still be kept.
    SourceRemoved,
    /// The tracked source no longer ships this Skill and its content is
    /// already gone. Nothing is left to copy or restore.
    SourceMissing,
}

impl LocalDivergenceReason {
    /// Does this stop mean the Skill has no source to go back to?
    pub fn is_source_gone(self) -> bool {
        matches!(self, Self::SourceRemoved | Self::SourceMissing)
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct SkillUpdateBlocked {
    pub name: String,
    pub reason: LocalDivergenceReason,
    pub suggested_local_name: String,
    /// Unexpected detail only. `reason` already states *why* the update
    /// stopped and the UI localizes it, so an expected stop carries no
    /// message here — only a real failure (e.g. an unreadable Skill) does.
    pub error: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum LocalDivergenceResolution {
    Preserve {
        local_name: String,
    },
    Discard,
    /// Remove the Skill outright. Only meaningful once its source is gone.
    Uninstall,
}

/// The pull proved that the tracked source no longer ships these installed
/// Skills.
///
/// Carried as an error so the update takes its ordinary rollback path: the
/// user must be offered a choice while the content is still on disk, never
/// after it has already been deleted. Only names travel here — what the user
/// may do about each one depends on what the rollback managed to restore, so
/// it is decided once the checkout has settled.
#[derive(Debug)]
pub(crate) struct SourceRemovedStop {
    pub names: Vec<String>,
}

impl std::fmt::Display for SourceRemovedStop {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "the update source no longer contains: {}",
            self.names.join(", ")
        )
    }
}

impl std::error::Error for SourceRemovedStop {}

/// Whether an installed Skill's content is on disk, and if not, why.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ContentPresence {
    Present,
    /// The hub link dangles but the tracked source still ships the Skill — a
    /// sparse checkout that has not materialized it. The next pull re-adds it,
    /// so this is never a question for the user.
    NotCheckedOut,
    /// The hub link dangles and the tracked source dropped the Skill. Its
    /// content is already gone; removal is the only exit.
    SourceMissing,
}

pub(crate) fn content_presence(name: &str) -> ContentPresence {
    let hub_path = skillstar_core::infra::paths::hub_skills_dir().join(name);
    // `exists` follows the managed link, so this is false exactly when the
    // Skill has no content behind it.
    if hub_path.exists() {
        return ContentPresence::Present;
    }
    match tracked_source_ships(&hub_path) {
        Some(true) => ContentPresence::NotCheckedOut,
        _ => ContentPresence::SourceMissing,
    }
}

/// Classify a Skill whose source dropped it, or `None` when the source is fine.
pub(crate) fn removed_source_reason(name: &str) -> Option<LocalDivergenceReason> {
    match content_presence(name) {
        ContentPresence::SourceMissing => Some(LocalDivergenceReason::SourceMissing),
        ContentPresence::NotCheckedOut => None,
        ContentPresence::Present => {
            let hub_path = skillstar_core::infra::paths::hub_skills_dir().join(name);
            (tracked_source_ships(&hub_path) == Some(false))
                .then_some(LocalDivergenceReason::SourceRemoved)
        }
    }
}

/// Does the revision the next update resets to still contain this Skill?
///
/// `None` when the question cannot be answered — no managed checkout, no
/// resolvable revision, or a Skill that *is* the checkout root.
fn tracked_source_ships(hub_path: &Path) -> Option<bool> {
    let repo_root = repo_link::repo_root_of(hub_path)?;
    let pathspec = super::actual_repo_pathspec(hub_path, &repo_root).ok()??;
    git_ops::revision_contains_path(&repo_root, tracked_update_ref(&repo_root), &pathspec)
        // A checkout that has never fetched cannot disprove anything, so fall
        // back to what is committed locally rather than calling it deleted.
        .or_else(|| git_ops::revision_contains_path(&repo_root, "HEAD", &pathspec))
}

/// Installed Skills of a checkout that have no content once the pull is done.
///
/// A successful pull leaves every Skill the source still ships on disk, so
/// whatever is missing afterwards was dropped by the source.
pub(crate) fn skills_dropped_by_source(installed_names: &[String]) -> Vec<String> {
    let hub = skillstar_core::infra::paths::hub_skills_dir();
    installed_names
        .iter()
        .filter(|name| !hub.join(name).exists())
        .cloned()
        .collect()
}

/// Describe dropped Skills once the checkout has been rolled back.
///
/// The rollback decides what is still possible: content that came back can be
/// kept as a local copy, content that did not can only be removed. Asking the
/// disk after the fact beats predicting it beforehand.
pub(crate) fn blocked_for_dropped_source(names: &[String]) -> Vec<SkillUpdateBlocked> {
    names
        .iter()
        .map(|name| SkillUpdateBlocked {
            name: name.clone(),
            reason: match content_presence(name) {
                ContentPresence::Present => LocalDivergenceReason::SourceRemoved,
                _ => LocalDivergenceReason::SourceMissing,
            },
            suggested_local_name: suggested_local_name(name),
            error: None,
        })
        .collect()
}

pub(crate) fn local_divergences_for_group(
    entries: &[LockEntry],
    group: &plan::RepoGroup,
) -> Vec<SkillUpdateBlocked> {
    let hub = skillstar_core::infra::paths::hub_skills_dir();
    let representative_path = hub.join(&group.representative);
    let representative = entries
        .iter()
        .find(|entry| entry.name == group.representative);
    let shared_root = repo_link::repo_root_of(&representative_path);

    let candidates: Vec<&LockEntry> = match shared_root {
        Some(_) => entries
            .iter()
            .filter(|candidate| {
                let path = hub.join(&candidate.name);
                repo_link::repo_root_of(&path) == shared_root
            })
            .collect(),
        None => representative.into_iter().collect(),
    };

    let mut blocked: Vec<_> = candidates
        .into_iter()
        .filter(|entry| {
            let path = skillstar_core::infra::paths::hub_skills_dir().join(&entry.name);
            path.symlink_metadata().is_ok()
        })
        .filter_map(|entry| match content_presence(&entry.name) {
            // The pull materializes it again; stopping here would ask the user
            // to resolve something that is not broken.
            ContentPresence::NotCheckedOut => None,
            ContentPresence::SourceMissing => Some(source_missing(&entry.name)),
            ContentPresence::Present => match content::snapshot(&entry.name) {
                Ok(snapshot)
                    if entry.content_hash_version == Some(content::SNAPSHOT_HASH_VERSION)
                        && entry.content_hash.as_deref() == Some(&snapshot.content_hash) =>
                {
                    None
                }
                Ok(_) => Some(SkillUpdateBlocked {
                    name: entry.name.clone(),
                    reason: if entry.content_hash.is_some()
                        && entry.content_hash_version == Some(content::SNAPSHOT_HASH_VERSION)
                    {
                        LocalDivergenceReason::ContentChanged
                    } else {
                        LocalDivergenceReason::BaselineMissing
                    },
                    suggested_local_name: suggested_local_name(&entry.name),
                    error: None,
                }),
                Err(error) => Some(SkillUpdateBlocked {
                    name: entry.name.clone(),
                    reason: LocalDivergenceReason::SnapshotFailed,
                    suggested_local_name: suggested_local_name(&entry.name),
                    error: Some(error.to_string()),
                }),
            },
        })
        .collect();

    if let Some(shared_root) = shared_root
        && let Ok(installed) = std::fs::read_dir(&hub)
    {
        for installed in installed.flatten() {
            let Some(name) = installed.file_name().to_str().map(str::to_string) else {
                continue;
            };
            if !crate::hub_entry::is_managed_hub_entry(&name) {
                continue;
            }
            if entries.iter().any(|entry| entry.name == name) {
                continue;
            }
            let valid_skill_target =
                skillstar_core::infra::fs_ops::read_link_resolved(&installed.path())
                    .is_ok_and(|target| target.join("SKILL.md").is_file());
            if valid_skill_target
                && repo_link::repo_root_of(&installed.path()).as_deref()
                    == Some(shared_root.as_path())
            {
                blocked.push(missing_baseline(&name));
            }
        }
    }
    if representative.is_none() && representative_path.symlink_metadata().is_ok() {
        blocked.push(match content_presence(&group.representative) {
            ContentPresence::SourceMissing => source_missing(&group.representative),
            _ => missing_baseline(&group.representative),
        });
    }

    blocked.sort_by(|left, right| {
        is_checkout_root_skill(&left.name)
            .cmp(&is_checkout_root_skill(&right.name))
            .then_with(|| left.name.cmp(&right.name))
    });
    blocked.dedup_by(|left, right| left.name == right.name);
    blocked
}

pub fn inspect_skill_local_divergence(name: &str) -> Result<Option<SkillUpdateBlocked>> {
    content::validate_skill_name(name).map_err(anyhow::Error::from)?;
    let entries = lockfile::Lockfile::load(&lockfile::lockfile_path())
        .context("failed to load the Skill lockfile before inspecting divergence")?
        .skills;
    let group = plan::RepoGroup {
        representative: name.to_string(),
        covered: Vec::new(),
    };
    Ok(local_divergences_for_group(&entries, &group)
        .into_iter()
        .find(|blocked| blocked.name == name))
}

fn is_checkout_root_skill(name: &str) -> bool {
    let path = skillstar_core::infra::paths::hub_skills_dir().join(name);
    let Some(repo_root) = repo_link::repo_root_of(&path) else {
        return false;
    };
    skillstar_core::infra::fs_ops::read_link_resolved(&path)
        .is_ok_and(|target| same_physical_path(&target, &repo_root))
}

pub(super) fn same_physical_path(left: &Path, right: &Path) -> bool {
    let left = std::fs::canonicalize(left).unwrap_or_else(|_| left.to_path_buf());
    let right = std::fs::canonicalize(right).unwrap_or_else(|_| right.to_path_buf());
    left == right
}

fn missing_baseline(name: &str) -> SkillUpdateBlocked {
    SkillUpdateBlocked {
        name: name.to_string(),
        reason: LocalDivergenceReason::BaselineMissing,
        suggested_local_name: suggested_local_name(name),
        error: None,
    }
}

fn source_missing(name: &str) -> SkillUpdateBlocked {
    SkillUpdateBlocked {
        name: name.to_string(),
        reason: LocalDivergenceReason::SourceMissing,
        suggested_local_name: suggested_local_name(name),
        error: None,
    }
}

pub fn suggested_local_name(name: &str) -> String {
    let hub = skillstar_core::infra::paths::hub_skills_dir();
    let local = skillstar_core::infra::paths::local_skills_dir();
    let base = format!("{name}.local");
    for suffix in 1u32.. {
        let candidate = if suffix == 1 {
            base.clone()
        } else {
            format!("{base}.{suffix}")
        };
        if hub.join(&candidate).symlink_metadata().is_err()
            && local.join(&candidate).symlink_metadata().is_err()
        {
            return candidate;
        }
    }
    unreachable!("the local-copy name space cannot be exhausted")
}
