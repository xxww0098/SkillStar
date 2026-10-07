//! vercel-labs/skills update semantics (D-081).
//!
//! Check = compare each lock entry's `skill_folder_hash` (a git tree SHA)
//! against the upstream tree, grouped by `(source_url, git_ref)` so skills on
//! different refs never compare against the wrong tree (`crate::update_check`).
//! Apply = fetch each source group into one temp checkout and
//! overwrite-reinstall — manual updates do not detect or preserve local edits.
//! The background monitor skips Skills whose
//! content no longer matches their install baseline (D-095).

use std::collections::BTreeMap;
use std::path::Path;
use std::sync::Arc;

use crate::fetch;
use crate::git::transport::GitOperationSession;
use crate::installer::{self, InstallUnit};
use crate::skill_lock::{self, SkillLockEntry};
use crate::source_resolver::Source;

/// Upstream verdict for one locked skill.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Upstream {
    /// Upstream folder tree SHA (compare against the lock to decide).
    Hash(String),
    /// The locked `skill_path` no longer exists upstream.
    Removed,
    /// Could not determine (network/API failure) — keep the previous badge.
    Unknown,
}

/// Lock `ref` to fetch. Blank means the remote default branch (`HEAD`), not
/// `main` or `master` — those are only correct when they happen to be HEAD.
pub(crate) fn effective_git_ref(git_ref: Option<String>) -> Option<String> {
    git_ref.and_then(|value| {
        let trimmed = value.trim();
        (!trimmed.is_empty()).then(|| trimmed.to_string())
    })
}

/// Outcome of applying one update.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppliedUpdate {
    pub name: String,
    pub result: UpdateResult,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UpdateResult {
    /// Reinstalled from upstream; carries the new folder hash.
    Updated { folder_hash: Option<String> },
    /// Upstream no longer contains the skill.
    Removed,
    /// Upstream renamed the skill (frontmatter `name`); nothing was
    /// installed. Reinstalling would create a second identity next to the
    /// locked one, so the user decides.
    IdentityChanged { upstream_name: String },
    /// Local creation, bundle or unknown source: there is no upstream.
    NotUpdatable,
    /// Entry missing from the lock or source unparsable.
    Failed(String),
}

/// Check upstream state for the given lock entries.
///
/// `token` is the SkillStar GitHub App token when signed in (`None` uses the
/// anonymous budget). GitHub sources try the REST fast path first and fall
/// back to one temp shallow clone per group in `session` (so private
/// repositories are checked with the user's credentials); non-GitHub sources
/// always clone.
pub async fn check_upstream(
    entries: &[(String, SkillLockEntry)],
    token: Option<&str>,
    session: &GitOperationSession,
) -> BTreeMap<String, Upstream> {
    let api = crate::update_check::GitHubTreeApi {
        token: token.map(str::to_string),
    };
    crate::update_check::check_upstream_with(entries, Arc::new(api), session).await
}

/// Overwrite-reinstall the named skills from their locked sources.
///
/// Skills sharing a `(source, ref)` are fetched once. One skill per unit of
/// work: a failure is reported for that name only and does not block the
/// rest. Skills whose upstream folder vanished are reported as
/// [`UpdateResult::Removed`] for the UI's remove/convert exits.
pub fn apply_updates(names: &[String], session: &GitOperationSession) -> Vec<AppliedUpdate> {
    let lock = skill_lock::load();
    let mut outcomes: BTreeMap<String, UpdateResult> = BTreeMap::new();
    let mut groups: skill_lock::SourceGroups = BTreeMap::new();
    for name in names {
        // `name` is the canonical folder. vercel may have recorded the entry
        // under the raw frontmatter name instead.
        match lock.entry_for_folder(name) {
            None => {
                outcomes.insert(
                    name.clone(),
                    UpdateResult::Failed(format!("'{name}' is not recorded in the lock")),
                );
            }
            Some((_, entry)) if !entry.source_type.is_updatable() => {
                outcomes.insert(name.clone(), UpdateResult::NotUpdatable);
            }
            Some((_, entry)) => groups
                .entry((
                    entry.source_url.clone(),
                    effective_git_ref(entry.git_ref.clone()),
                ))
                .or_default()
                .push((name.clone(), entry.clone())),
        }
    }

    for ((source_url, git_ref), members) in groups {
        let spec = match Source::parse(&source_url) {
            Ok(mut spec) => {
                spec.git_ref = git_ref.or(spec.git_ref);
                spec.subpath = None;
                spec.skill_filter = None;
                spec
            }
            Err(error) => {
                for (name, _) in members {
                    outcomes.insert(name, UpdateResult::Failed(error.to_string()));
                }
                continue;
            }
        };
        // Fetch outside the transaction (network); each Skill re-validates its
        // entry under it so a concurrent reinstall or uninstall is not overwritten.
        let checkout = match fetch::fetch_source(&spec, session) {
            Ok(checkout) => checkout,
            Err(error) => {
                let reason = format!("{error:#}");
                for (name, _) in members {
                    outcomes.insert(name, UpdateResult::Failed(reason.clone()));
                }
                continue;
            }
        };
        for (name, entry) in members {
            let result = apply_from_checkout(checkout.dir(), &spec, &name, &entry);
            outcomes.insert(name, result);
        }
    }

    names
        .iter()
        .map(|name| AppliedUpdate {
            name: name.clone(),
            result: outcomes
                .get(name)
                .cloned()
                .unwrap_or_else(|| UpdateResult::Failed(format!("'{name}' produced no result"))),
        })
        .collect()
}

/// What one background auto-update run did.
#[derive(Debug, Clone, Default)]
pub struct AutoUpdateReport {
    /// Locked Skills whose upstream tree was compared this run.
    pub checked: usize,
    pub updated: Vec<String>,
    /// Upstream no longer ships them; nothing is deleted automatically.
    pub skipped: Vec<String>,
    /// An update is available but the canonical copy was edited after install
    /// (or that cannot be proven); left for a manual update.
    pub kept_local: Vec<String>,
    pub failed: Vec<crate::skill_update::SkillUpdateFailure>,
    /// Set when the run could not even start (check or apply task failure).
    pub error: Option<String>,
}

/// Background auto-update: check every generic locked Skill, then
/// overwrite-reinstall the ones whose upstream tree changed and whose content
/// still equals its install baseline.
///
/// The manual entries run the same pair — the check through
/// crate::installed_skill::refresh_skill_updates_in_session and the apply
/// through crate::git_skill::GitSkillFacade::update_skills — so the badge,
/// lock, Agent/Project links and installed cache move together. Channel-managed
/// Skills, local creations and bundle installs never participate.
pub async fn auto_update_locked_skills(session: &GitOperationSession) -> AutoUpdateReport {
    let mut report = AutoUpdateReport::default();
    let states = match crate::installed_skill::refresh_skill_updates_in_session(session).await {
        Ok(states) => states,
        Err(error) => {
            report.error = Some(format!("{error:#}"));
            return report;
        }
    };
    report.checked = states.len();
    let mut pending = Vec::new();
    for state in states.into_iter().filter(|state| {
        // A renamed upstream needs the user's decision; retrying is pointless.
        state.update_available
            && !matches!(
                state.upstream_change,
                Some(crate::update_state::UpstreamChange::IdentityChanged { .. })
            )
    }) {
        // Re-read the content now: the check may have run before an edit.
        match crate::install_baseline::local_change(&state.name) {
            None => pending.push(state.name),
            Some(change) => {
                crate::update_state::record(&state.name, true, Some(change));
                report.kept_local.push(state.name);
            }
        }
    }
    if pending.is_empty() {
        return report;
    }

    // The apply half is blocking (Git subprocesses, file writes); keep it off
    // the async worker the way the channel installers do.
    let session = session.clone();
    let outcome = tokio::task::spawn_blocking(move || {
        crate::git_skill::GitSkillFacade::new(session).update_skills(&pending)
    })
    .await;

    match outcome {
        Ok(outcome) => {
            report.updated = outcome
                .updated
                .into_iter()
                .map(|result| result.skill.name)
                .collect();
            report.skipped = outcome.skipped;
            report.failed = outcome.failed;
        }
        Err(error) => report.error = Some(format!("auto update task failed: {error}")),
    }
    report
}

fn apply_from_checkout(
    checkout: &Path,
    spec: &Source,
    name: &str,
    entry: &SkillLockEntry,
) -> UpdateResult {
    let _transaction = match crate::skill_update::acquire_update_transaction_lock() {
        Ok(guard) => guard,
        Err(error) => return UpdateResult::Failed(format!("{error:#}")),
    };
    let current = skill_lock::load();
    let unchanged = current.entry_for_folder(name).is_some_and(|(_, now)| {
        now.source_url == entry.source_url
            && now.git_ref == entry.git_ref
            && now.skill_path == entry.skill_path
    });
    if !unchanged {
        return UpdateResult::Failed(format!(
            "'{name}' changed while its update was being fetched; retry the update"
        ));
    }
    let outcome = reinstall_from_checkout(checkout, spec, name, entry);
    if let UpdateResult::Updated { folder_hash } = &outcome {
        let folder_hash = folder_hash.clone();
        if let Err(error) = skill_lock::mutate(|lock| {
            // upsert folds every key for this folder into the canonical name,
            // keeps unknown fields, and keeps the earliest installedAt.
            // SkillLockEntry serializes the pin as `ref` (it also reads `gitRef`).
            let Some(updated) = lock.entry_for_folder(name).map(|(_, stored)| {
                let mut stored = stored.clone();
                stored.skill_folder_hash = folder_hash.clone();
                stored.updated_at = chrono::Utc::now().to_rfc3339();
                stored
            }) else {
                return;
            };
            lock.upsert(name, updated);
        }) {
            return UpdateResult::Failed(format!("{error:#}"));
        }
    }
    outcome
}

fn reinstall_from_checkout(
    checkout: &Path,
    spec: &Source,
    name: &str,
    entry: &SkillLockEntry,
) -> UpdateResult {
    let folder = entry.skill_path.clone().unwrap_or_default();
    let dir = if folder.is_empty() {
        checkout.to_path_buf()
    } else {
        checkout.join(&folder)
    };
    if !dir.join("SKILL.md").is_file() {
        return UpdateResult::Removed;
    }
    // An upstream frontmatter rename is a new identity: installing it under
    // the locked folder would make the folder and its `name` disagree, and
    // installing it under the new name would orphan the locked one. Stop and
    // let the user choose.
    if let Some(upstream) = crate::validation::inspect_skill_frontmatter(&dir)
        .name
        .filter(|n| !n.trim().is_empty())
        && installer::canonical_skill_name(&upstream).ok().as_deref() != Some(name)
    {
        return UpdateResult::IdentityChanged {
            upstream_name: upstream,
        };
    }
    let unit = InstallUnit {
        id: name.to_string(),
        folder_path: folder,
    };
    match installer::install_units(checkout, spec, &[unit]) {
        Ok(_) => {
            let path = if entry.skill_path.as_deref().unwrap_or("").is_empty() {
                None
            } else {
                entry.skill_path.as_deref()
            };
            UpdateResult::Updated {
                folder_hash: fetch::folder_tree_hash(checkout, path),
            }
        }
        Err(error) => UpdateResult::Failed(format!("{error:#}")),
    }
}

#[cfg(test)]
#[path = "update_tests.rs"]
mod tests;
