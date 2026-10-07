//! Bring skills an Agent already installed under SkillStar management.
//!
//! [`scan`] is read-only. [`plan`] selects what can be linked or copied.
//! [`apply`] holds the skill transaction lock, re-checks each step, and
//! restores that step when a later write fails. `skillstar doctor --fix`
//! does not call this: adoption is explicit (`doctor --adopt`, and
//! [`super::repair_installations`]).
//!
//! A directory that contains `.git` or another excluded name is reported
//! and left in place. Replacing it with a link would drop those files,
//! because the confined copy never carries them.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use serde::Serialize;
use ss_core::infra::{fs_ops, paths};

use crate::deployment::ownership::{self, Ownership};
use crate::skill_lock::{self, LockFileState, SkillLock};

/// Names a confined copy skips. If any of these sit in the Agent directory,
/// replacing that directory with a link would delete them, so the directory
/// is reported and not moved.
pub(super) const INTAKE_EXCLUDES: &[&str] = &[
    ".git",
    ".skillstar",
    ".skillstar-deploy.json",
    ".DS_Store",
    "Thumbs.db",
    "desktop.ini",
    "__pycache__",
    "__pypackages__",
    "metadata.json",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum IntakeKind {
    /// Canonical has no such skill. Copy it in, then link.
    Adopt,
    /// Canonical already has the same bytes. Link, do not copy.
    Relink,
    /// Same name, different bytes or extra files. Leave both.
    Conflict,
    /// `.git` or another excluded name. Leave the directory.
    Excluded,
    /// A link that does not point at a SkillStar skill.
    Foreign,
    /// The canonical name is taken by something we must not overwrite.
    Occupied,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IntakeFinding {
    pub kind: IntakeKind,
    pub skill: String,
    pub agent_id: String,
    pub path: PathBuf,
    pub reason: String,
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IntakeReport {
    pub findings: Vec<IntakeFinding>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum IntakeAction {
    Adopt,
    Relink,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IntakeStep {
    #[serde(flatten)]
    pub action: IntakeAction,
    pub skill: String,
    pub agent_id: String,
    pub path: PathBuf,
    pub reason: String,
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IntakePlan {
    pub steps: Vec<IntakeStep>,
    /// Conflicts, excluded trees, foreign links, occupied names.
    pub reported: Vec<IntakeFinding>,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct IntakeApplyOptions {
    pub dry_run: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum IntakeStatus {
    Applied,
    WouldApply,
    AlreadyDone,
    Skipped { reason: String },
    Failed { error: String },
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IntakeStepOutcome {
    pub step: IntakeStep,
    #[serde(flatten)]
    pub status: IntakeStatus,
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IntakeOutcome {
    pub steps: Vec<IntakeStepOutcome>,
}

impl IntakeOutcome {
    pub fn applied_count(&self) -> usize {
        self.count(|status| matches!(status, IntakeStatus::Applied))
    }

    pub fn would_apply_count(&self) -> usize {
        self.count(|status| matches!(status, IntakeStatus::WouldApply))
    }

    pub fn failed(&self) -> impl Iterator<Item = &IntakeStepOutcome> {
        self.steps
            .iter()
            .filter(|outcome| matches!(outcome.status, IntakeStatus::Failed { .. }))
    }

    fn count(&self, pred: impl Fn(&IntakeStatus) -> bool) -> usize {
        self.steps
            .iter()
            .filter(|outcome| pred(&outcome.status))
            .count()
    }
}

enum Class {
    Skip,
    Step {
        action: IntakeAction,
        reason: &'static str,
    },
    Hold {
        kind: IntakeKind,
        reason: String,
    },
}

/// Read every Agent global skills directory except those that *are* the
/// canonical root (Pi, Cline). Enabled and disabled profiles are both
/// visited: the Agent installed the skill whether or not Settings has
/// turned the profile on. Nothing here writes.
pub fn scan() -> IntakeReport {
    let mut report = IntakeReport::default();
    for (agent_id, root) in agent_roots() {
        let entries = match std::fs::read_dir(&root) {
            Ok(entries) => entries,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(error) => {
                report.findings.push(finding(
                    IntakeKind::Excluded,
                    String::new(),
                    agent_id,
                    root,
                    error.to_string(),
                ));
                continue;
            }
        };
        let mut entries: Vec<_> = entries.flatten().collect();
        entries.sort_by_key(|entry| entry.file_name());
        for entry in entries {
            let Some(name) = entry.file_name().into_string().ok() else {
                continue;
            };
            if name.starts_with('.') {
                continue;
            }
            let path = entry.path();
            match classify_entry(&agent_id, &name, &path) {
                Ok(Class::Skip) => {}
                Ok(Class::Step { action, reason }) => {
                    let kind = match action {
                        IntakeAction::Adopt => IntakeKind::Adopt,
                        IntakeAction::Relink => IntakeKind::Relink,
                    };
                    report
                        .findings
                        .push(finding(kind, name, agent_id.clone(), path, reason));
                }
                Ok(Class::Hold { kind, reason }) => {
                    report
                        .findings
                        .push(finding(kind, name, agent_id.clone(), path, reason));
                }
                Err(error) => report.findings.push(finding(
                    IntakeKind::Excluded,
                    name,
                    agent_id.clone(),
                    path,
                    format!("{error:#}"),
                )),
            }
        }
    }
    report
}

pub fn plan(report: &IntakeReport) -> IntakePlan {
    let mut plan = IntakePlan::default();
    for finding in &report.findings {
        match finding.kind {
            IntakeKind::Adopt => plan.steps.push(step(IntakeAction::Adopt, finding)),
            IntakeKind::Relink => plan.steps.push(step(IntakeAction::Relink, finding)),
            IntakeKind::Conflict
            | IntakeKind::Excluded
            | IntakeKind::Foreign
            | IntakeKind::Occupied => plan.reported.push(finding.clone()),
        }
    }
    plan
}

pub fn apply(plan: &IntakePlan) -> Result<IntakeOutcome> {
    apply_with(plan, IntakeApplyOptions::default())
}

/// `dry_run` takes the same lock and runs the same checks, then writes nothing.
pub fn apply_with(plan: &IntakePlan, options: IntakeApplyOptions) -> Result<IntakeOutcome> {
    let _guard = crate::skill_update::acquire_update_transaction_lock()?;
    let mut outcome = IntakeOutcome::default();
    for step in &plan.steps {
        let status = run_step(step, options.dry_run).unwrap_or_else(|error| IntakeStatus::Failed {
            error: format!("{error:#}"),
        });
        if matches!(status, IntakeStatus::Applied) {
            crate::installed_skill::invalidate_cache();
        }
        outcome.steps.push(IntakeStepOutcome {
            step: step.clone(),
            status,
        });
    }
    Ok(outcome)
}

fn run_step(step: &IntakeStep, dry_run: bool) -> Result<IntakeStatus> {
    let class = classify_entry(&step.agent_id, &step.skill, &step.path)?;
    let action = match class {
        Class::Skip => return Ok(IntakeStatus::AlreadyDone),
        Class::Hold { reason, .. } => return Ok(IntakeStatus::Skipped { reason }),
        Class::Step { action, .. } => action,
    };
    let action = match (step.action, action) {
        (IntakeAction::Adopt, IntakeAction::Relink) => IntakeAction::Relink,
        (planned, actual) if planned == actual => actual,
        _ => {
            return Ok(IntakeStatus::Skipped {
                reason: "the directory changed before this step; left unchanged".into(),
            });
        }
    };
    if dry_run {
        return Ok(IntakeStatus::WouldApply);
    }
    match action {
        IntakeAction::Adopt => {
            if let Some(reason) = lock_block() {
                return Ok(IntakeStatus::Skipped {
                    reason: reason.into(),
                });
            }
            apply::adopt(&step.path, &step.skill, &step.agent_id)?;
        }
        IntakeAction::Relink => {
            let target = relink_target(&step.skill)?;
            match apply::relink(&step.path, &target)? {
                Some(reason) => return Ok(IntakeStatus::Skipped { reason }),
                None => {}
            }
        }
    }
    Ok(IntakeStatus::Applied)
}

fn finding(
    kind: IntakeKind,
    skill: String,
    agent_id: String,
    path: PathBuf,
    reason: impl Into<String>,
) -> IntakeFinding {
    IntakeFinding {
        kind,
        skill,
        agent_id,
        path,
        reason: reason.into(),
    }
}

fn step(action: IntakeAction, finding: &IntakeFinding) -> IntakeStep {
    IntakeStep {
        action,
        skill: finding.skill.clone(),
        agent_id: finding.agent_id.clone(),
        path: finding.path.clone(),
        reason: finding.reason.clone(),
    }
}

/// Global skills directories, one per physical path. Canonical-root agents
/// are omitted so the SkillStar store is never treated as a folder to replace.
fn agent_roots() -> Vec<(String, PathBuf)> {
    let mut seen = BTreeSet::new();
    let mut roots = Vec::new();
    for profile in crate::agents::list_profiles() {
        if !profile.has_global_skills() {
            continue;
        }
        if ownership::targets_canonical_root(&profile.global_skills_dir) {
            continue;
        }
        let key = fs_ops::canonicalize_existing_prefix(&profile.global_skills_dir);
        if !seen.insert(key) {
            continue;
        }
        roots.push((profile.id, profile.global_skills_dir));
    }
    roots
}

fn classify_entry(_agent_id: &str, name: &str, path: &Path) -> Result<Class> {
    if fs_ops::is_link(path) {
        return Ok(classify_link(name, path));
    }
    let meta = match path.symlink_metadata() {
        Ok(meta) => meta,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Class::Skip),
        Err(error) => {
            return Ok(hold(
                IntakeKind::Excluded,
                format!("could not read {}: {error}", path.display()),
            ));
        }
    };
    if !meta.is_dir() || !path.join("SKILL.md").is_file() {
        return Ok(Class::Skip);
    }
    if let Err(error) = crate::content::validate_skill_name(name) {
        return Ok(hold(IntakeKind::Occupied, error.to_string()));
    }
    if let Some(reason) = blocking(path)? {
        return Ok(hold(IntakeKind::Excluded, reason));
    }
    if let Err(reason) = crate::validation::ensure_installable(path) {
        return Ok(hold(IntakeKind::Excluded, reason));
    }
    if let Some(reason) = channel_block(name)
        && live_canonical(name).is_none()
    {
        // A channel skill that already lives in the canonical root can still
        // be relinked when the bytes match. Creating a new local copy cannot.
        return Ok(hold(IntakeKind::Occupied, reason));
    }
    if ownership::marker_content_hash(path).is_some() {
        return Ok(
            if ownership::owned_deployment(path, name) == Ownership::Copy {
                Class::Skip
            } else {
                hold(
                    IntakeKind::Conflict,
                    "SkillStar copy no longer matches its deploy marker; left unchanged",
                )
            },
        );
    }
    if skill_lock::load().entry_for_folder(name).is_some() && live_canonical(name).is_none() {
        return Ok(hold(
            IntakeKind::Occupied,
            "install lock already records this skill; left unchanged",
        ));
    }
    match live_canonical(name) {
        Some(canon) => classify_against(path, &canon),
        None if name_taken(name) => Ok(hold(
            IntakeKind::Occupied,
            "canonical name is occupied; left unchanged",
        )),
        None => Ok(Class::Step {
            action: IntakeAction::Adopt,
            reason: "copy into local storage, record a local adoption, then link the Agent directory at the canonical skill",
        }),
    }
}

fn classify_link(name: &str, path: &Path) -> Class {
    match ownership::owned_deployment(path, name) {
        Ownership::Link { .. } => Class::Skip,
        _ if path.join("SKILL.md").is_file() => hold(
            IntakeKind::Foreign,
            "link points outside SkillStar; left unchanged",
        ),
        _ => Class::Skip,
    }
}

fn classify_against(agent: &Path, canon: &Path) -> Result<Class> {
    let resolved = std::fs::canonicalize(canon).unwrap_or_else(|_| canon.to_path_buf());
    if content_matches(agent, &resolved)? {
        Ok(Class::Step {
            action: IntakeAction::Relink,
            reason: "content matches the canonical skill; replace the Agent directory with a relative link and do not move files",
        })
    } else {
        Ok(hold(
            IntakeKind::Conflict,
            "Content conflict; both versions kept",
        ))
    }
}

fn hold(kind: IntakeKind, reason: impl Into<String>) -> Class {
    Class::Hold {
        kind,
        reason: reason.into(),
    }
}

/// The directory whose bytes are the installed skill, if one is live.
///
/// Prefers the canonical entry. A local directory counts only when the
/// canonical name is absent, so a broken canonical link is not treated as
/// the skill we should link to.
fn live_canonical(name: &str) -> Option<PathBuf> {
    let hub = paths::hub_skills_dir().join(name);
    if live_skill_dir(&hub) {
        return Some(hub);
    }
    // A broken hub link is not the skill. A real local directory still is,
    // so a same-name Agent folder is compared to it instead of copied over it.
    let local = paths::local_skills_dir().join(name);
    live_skill_dir(&local).then_some(local)
}

fn name_taken(name: &str) -> bool {
    paths::hub_skills_dir()
        .join(name)
        .symlink_metadata()
        .is_ok()
        || paths::local_skills_dir()
            .join(name)
            .symlink_metadata()
            .is_ok()
}

fn live_skill_dir(path: &Path) -> bool {
    path.is_dir() && path.join("SKILL.md").is_file()
}

fn relink_target(name: &str) -> Result<PathBuf> {
    live_canonical(name).context("canonical skill is not a live directory")
}

pub(super) fn channel_block(name: &str) -> Option<String> {
    match crate::skill_mutation::skill_is_channel_managed(name) {
        Ok(true) => Some("owned by a shared channel; left unchanged".into()),
        Ok(false) => None,
        Err(_) => Some("channel ownership could not be read; left unchanged".into()),
    }
}

fn lock_block() -> Option<&'static str> {
    match SkillLock::read_state(&skill_lock::lock_path()) {
        LockFileState::Missing | LockFileState::Ready(_) => None,
        LockFileState::Outdated(_) => Some("install lock uses an older schema; left unchanged"),
        LockFileState::TooNew(_) => {
            Some("install lock is newer than this SkillStar; left unchanged")
        }
        LockFileState::Corrupt(_) => Some("install lock is unreadable; left unchanged"),
    }
}

pub(super) fn content_matches(agent: &Path, canon: &Path) -> Result<bool> {
    let agent_entries = entry_set(agent)?;
    let canon_entries = entry_set(canon)?;
    if agent_entries
        .iter()
        .any(|entry| !canon_entries.contains(entry))
    {
        return Ok(false);
    }
    if ownership::dir_content_hash(agent)? != ownership::dir_content_hash(canon)? {
        return Ok(false);
    }
    executable_matches(agent, canon)
}

fn executable_matches(agent: &Path, canon: &Path) -> Result<bool> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        for rel in file_rels(agent)? {
            let left = std::fs::metadata(agent.join(&rel))?.permissions().mode() & 0o111;
            let right = std::fs::metadata(canon.join(&rel))?.permissions().mode() & 0o111;
            if left != right {
                return Ok(false);
            }
        }
    }
    let _ = (agent, canon);
    Ok(true)
}

mod apply;

pub(super) fn blocking(root: &Path) -> Result<Option<String>> {
    fn walk(root: &Path, dir: &Path) -> Result<Option<String>> {
        let root_real = std::fs::canonicalize(root)?;
        for entry in
            std::fs::read_dir(dir).with_context(|| format!("could not list {}", dir.display()))?
        {
            let entry = entry?;
            let name = entry.file_name();
            if INTAKE_EXCLUDES.iter().any(|excluded| name == *excluded) {
                let display = name.to_string_lossy();
                let reason = if display == ".git" {
                    "Git working tree left unchanged".to_string()
                } else {
                    format!("excluded entry {display} left unchanged")
                };
                return Ok(Some(reason));
            }
            let path = entry.path();
            if fs_ops::is_link(&path) {
                let target = std::fs::canonicalize(&path).with_context(|| {
                    format!("broken internal link {}; left unchanged", path.display())
                })?;
                if !target.starts_with(&root_real) {
                    bail!(
                        "internal link {} escapes the skill; left unchanged",
                        path.display()
                    );
                }
                let inside = target.strip_prefix(&root_real).unwrap_or(&target);
                if inside.components().any(|part| {
                    INTAKE_EXCLUDES
                        .iter()
                        .any(|excluded| part.as_os_str() == *excluded)
                }) {
                    return Ok(Some("excluded entry left unchanged".to_string()));
                }
                continue;
            }
            if path.is_dir()
                && let Some(reason) = walk(root, &path)?
            {
                return Ok(Some(reason));
            }
        }
        Ok(None)
    }
    walk(root, root)
}

pub(super) fn entry_set(root: &Path) -> Result<BTreeSet<PathBuf>> {
    let mut out = BTreeSet::new();
    fn walk(root: &Path, dir: &Path, out: &mut BTreeSet<PathBuf>) -> Result<()> {
        for entry in std::fs::read_dir(dir)? {
            let path = entry?.path();
            out.insert(path.strip_prefix(root).unwrap_or(&path).to_path_buf());
            if path.is_dir() && !fs_ops::is_link(&path) {
                walk(root, &path, out)?;
            }
        }
        Ok(())
    }
    walk(root, root, &mut out)?;
    Ok(out)
}

fn file_rels(root: &Path) -> Result<Vec<PathBuf>> {
    let mut out = Vec::new();
    fn walk(root: &Path, dir: &Path, out: &mut Vec<PathBuf>) -> Result<()> {
        for entry in std::fs::read_dir(dir)? {
            let path = entry?.path();
            if fs_ops::is_link(&path) {
                if path.is_file() {
                    out.push(path.strip_prefix(root).unwrap_or(&path).to_path_buf());
                }
                continue;
            }
            if path.is_dir() {
                walk(root, &path, out)?;
            } else if path.is_file() {
                out.push(path.strip_prefix(root).unwrap_or(&path).to_path_buf());
            }
        }
        Ok(())
    }
    walk(root, root, &mut out)?;
    Ok(out)
}

#[cfg(test)]
pub(super) fn failpoint(name: &str) -> Result<()> {
    if std::env::var("SKILLSTAR_INTAKE_FAIL").ok().as_deref() == Some(name) {
        bail!("injected failure after {name}");
    }
    Ok(())
}

#[cfg(not(test))]
pub(super) fn failpoint(_name: &str) -> Result<()> {
    Ok(())
}

#[cfg(test)]
#[path = "intake/tests.rs"]
mod tests;
