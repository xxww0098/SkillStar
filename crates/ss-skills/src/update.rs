//! Skill update semantics (D-081).
//!
//! Check = compare each lock entry's `skill_folder_hash` (a git tree SHA)
//! against the upstream tree, grouped by `(source_url, git_ref)` so skills on
//! different refs never compare against the wrong tree (`crate::update_check`).
//! Apply = fetch each source group through the persistent sparse import cache
//! (refreshed at the locked ref). Groups run with bounded concurrency; the
//! installs still serialize on the update-transaction lock. Manual updates
//! overwrite. Automatic updates re-check the install baseline under that lock
//! and keep a canonical copy that is no longer provably as installed (D-095).

use std::collections::{BTreeMap, VecDeque};
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
    /// Automatic admission left the canonical copy in place. The content no
    /// longer matches the install baseline, or that cannot be proven.
    KeptLocal,
}

/// Whether this apply may replace a canonical copy that diverged from its
/// install baseline. The decision is applied under the transaction lock,
/// after the fetch, so an edit that lands while the network runs still counts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum OverwriteAdmission {
    /// Manual update. Local edits are replaced.
    Overwrite,
    /// Background update. Skip when the baseline does not match.
    ProtectBaseline,
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

/// Source groups fetched and applied at the same time, mirroring the check
/// path's concurrency. The fetches overlap; each install still runs under the
/// update-transaction lock, whose process mutex queues the worker threads.
const MAX_CONCURRENT_UPDATE_SOURCES: usize = 4;

/// Overwrite-reinstall the named skills from their locked sources.
///
/// Skills sharing a `(source, ref)` are fetched once, through the persistent
/// sparse import cache refreshed at that ref. Groups run with bounded
/// concurrency. One skill per unit of work: a failure is reported for that
/// name only and does not block the rest. Skills whose upstream folder
/// vanished are reported as [`UpdateResult::Removed`] for the UI's
/// remove/convert exits.
pub fn apply_updates(names: &[String], session: &GitOperationSession) -> Vec<AppliedUpdate> {
    apply_updates_admitting(names, session, OverwriteAdmission::Overwrite)
}

pub(crate) fn apply_updates_admitting(
    names: &[String],
    session: &GitOperationSession,
    admission: OverwriteAdmission,
) -> Vec<AppliedUpdate> {
    let lock = skill_lock::load();
    let mut outcomes: BTreeMap<String, UpdateResult> = BTreeMap::new();
    let mut groups: skill_lock::SourceGroups = BTreeMap::new();
    for name in names {
        // `name` is the canonical folder. Other lock writers may have recorded
        // the entry under the raw frontmatter name instead.
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

    apply_source_groups(groups, session, admission, &mut outcomes);

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

/// Fetch and apply every source group, at most
/// [`MAX_CONCURRENT_UPDATE_SOURCES`] groups at a time. Groups are
/// independent sources, so the network waits overlap; a group's failure is
/// reported for its names only.
fn apply_source_groups(
    groups: skill_lock::SourceGroups,
    session: &GitOperationSession,
    admission: OverwriteAdmission,
    outcomes: &mut BTreeMap<String, UpdateResult>,
) {
    let queue = std::sync::Mutex::new(VecDeque::from_iter(groups));
    let applied = std::sync::Mutex::new(Vec::<(String, UpdateResult)>::new());
    std::thread::scope(|scope| {
        let workers = MAX_CONCURRENT_UPDATE_SOURCES.min(queue.lock().expect("group queue").len());
        for _ in 0..workers {
            scope.spawn(|| {
                loop {
                    let group = queue.lock().expect("group queue").pop_front();
                    let Some(((source_url, git_ref), members)) = group else {
                        break;
                    };
                    let results =
                        fetch_and_apply_group(&source_url, git_ref, members, session, admission);
                    applied.lock().expect("group results").extend(results);
                }
            });
        }
    });
    outcomes.extend(applied.into_inner().expect("group results"));
}

/// One group's fetch, then its members' installs. The fetch runs outside the
/// update transaction (network); each Skill re-validates its entry under it
/// so a concurrent reinstall or uninstall is not overwritten.
fn fetch_and_apply_group(
    source_url: &str,
    git_ref: Option<String>,
    members: Vec<(String, SkillLockEntry)>,
    session: &GitOperationSession,
    admission: OverwriteAdmission,
) -> Vec<(String, UpdateResult)> {
    let fail_all = |members: Vec<(String, SkillLockEntry)>, reason: String| {
        members
            .into_iter()
            .map(|(name, _)| (name, UpdateResult::Failed(reason.clone())))
            .collect::<Vec<_>>()
    };
    let spec = match Source::parse(source_url) {
        Ok(mut spec) => {
            spec.git_ref = git_ref.or(spec.git_ref);
            spec.subpath = None;
            spec.skill_filter = None;
            spec
        }
        Err(error) => return fail_all(members, error.to_string()),
    };
    let folders = members
        .iter()
        .map(|(_, entry)| entry.skill_path.clone().unwrap_or_default())
        .collect::<Vec<String>>();
    let folder_refs = folders.iter().map(String::as_str).collect::<Vec<&str>>();
    let checkout = match fetch::fetch_for_update(&spec, &folder_refs, session) {
        Ok(checkout) => checkout,
        Err(error) => return fail_all(members, format!("{error:#}")),
    };
    members
        .into_iter()
        .map(|(name, entry)| {
            let result = apply_from_checkout(checkout.dir(), &spec, &name, &entry, admission);
            (name, result)
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

/// Background auto-update: check every generic locked Skill, then apply the
/// ones whose upstream tree changed.
///
/// Local-edit eligibility is not decided here. The apply admits with
/// [`OverwriteAdmission::ProtectBaseline`], so the baseline is read again
/// under the transaction lock after the fetch. A renamed upstream still needs
/// the user's decision and is not retried. The manual entry
/// ([`crate::git_skill::GitSkillFacade::update_skills`]) is the overwrite
/// admission of the same path, so the badge, lock, Agent/Project links and
/// installed cache move together. Channel-managed Skills, local creations and
/// bundle installs never participate.
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
    let pending: Vec<String> = states
        .into_iter()
        .filter(|state| {
            // A renamed upstream needs the user's decision; retrying is pointless.
            state.update_available
                && !matches!(
                    state.upstream_change,
                    Some(crate::update_state::UpstreamChange::IdentityChanged { .. })
                )
        })
        .map(|state| state.name)
        .collect();
    if pending.is_empty() {
        return report;
    }

    // The apply half is blocking (Git subprocesses, file writes); keep it off
    // the async worker the way the channel installers do.
    let session = session.clone();
    let outcome = tokio::task::spawn_blocking(move || {
        crate::git_skill::GitSkillFacade::new(session)
            .update_skills_admitting(&pending, OverwriteAdmission::ProtectBaseline)
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
            report.kept_local = outcome.kept_local;
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
    admission: OverwriteAdmission,
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
    if admission == OverwriteAdmission::ProtectBaseline
        && let Some(change) = crate::install_baseline::local_change(name)
    {
        crate::update_state::record(name, true, Some(change));
        return UpdateResult::KeptLocal;
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
