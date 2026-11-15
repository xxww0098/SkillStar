//! Turn a [`SkillHealthReport`] into repair steps. Writes nothing; reads only
//! the report, path existence and the channel ownership registry.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use serde::Serialize;

use super::scan::{HealthIssue, IssueKind, SkillHealthReport, is_transient};

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum RepairAction {
    /// Re-create a SkillStar link/copy whose target is gone or stale.
    Relink,
    /// Replace an unmodified SkillStar copy with the current canonical content.
    RefreshCopy,
    /// Delete an entry only SkillStar creates (dead managed link, staging).
    RemoveOwnedResidue,
    /// Delete a link that resolves to itself; it holds no content.
    RemoveSelfLink,
    /// Put an interrupted operation's backup back where it came from.
    RestoreBackup { dest: PathBuf },
    /// Fetch a missing canonical folder again from its lock entry's source.
    ReinstallFromLock { source_url: String },
    /// Drop a lock entry whose folder is gone and cannot be re-fetched.
    PruneLockEntry,
    /// Turn an unlocked canonical folder into a local Skill (opt-in).
    AdoptAsLocal,
    /// Reinstall a Skill the pre-D-081 cleanup removed.
    ReinstallLegacy {
        git_url: String,
        git_ref: Option<String>,
        source_folder: Option<String>,
    },
    /// Re-run the mirror sync of one Agent.
    ResyncMirror,
    /// Re-run the pre-D-081 legacy hub cleanup.
    RerunLegacyCleanup,
    /// Re-run the hub/local → data/skills/local migration.
    RerunLocalMigration,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RepairStep {
    #[serde(flatten)]
    pub action: RepairAction,
    pub skill: Option<String>,
    pub agent_id: Option<String>,
    pub path: PathBuf,
    /// Why SkillStar may change this path.
    pub evidence: String,
}

/// A finding repair leaves alone, and why.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UntouchedIssue {
    pub issue: HealthIssue,
    pub reason: String,
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RepairPlan {
    pub steps: Vec<RepairStep>,
    pub untouched: Vec<UntouchedIssue>,
}

#[derive(Debug, Clone, Copy)]
pub struct PlanOptions {
    /// Re-fetch missing folders of updatable lock entries instead of
    /// pruning them.
    pub reinstall_from_lock: bool,
    /// Adopt unlocked canonical folders as local Skills.
    pub adopt_unlocked: bool,
}

impl Default for PlanOptions {
    fn default() -> Self {
        Self {
            reinstall_from_lock: true,
            adopt_unlocked: false,
        }
    }
}

pub fn plan(report: &SkillHealthReport) -> RepairPlan {
    plan_with(report, PlanOptions::default())
}

pub fn plan_with(report: &SkillHealthReport, options: PlanOptions) -> RepairPlan {
    Planner {
        report,
        options,
        plan: RepairPlan::default(),
        mirrors: BTreeSet::new(),
    }
    .run()
}

/// Order steps run in: canonical content first, then what links to it.
fn phase(action: &RepairAction) -> u8 {
    match action {
        RepairAction::RestoreBackup { .. } => 0,
        RepairAction::RemoveOwnedResidue | RepairAction::RemoveSelfLink => 1,
        RepairAction::RerunLegacyCleanup | RepairAction::RerunLocalMigration => 2,
        RepairAction::ReinstallFromLock { .. }
        | RepairAction::ReinstallLegacy { .. }
        | RepairAction::PruneLockEntry
        | RepairAction::AdoptAsLocal => 3,
        RepairAction::Relink | RepairAction::RefreshCopy => 4,
        RepairAction::ResyncMirror => 5,
    }
}

struct Planner<'a> {
    report: &'a SkillHealthReport,
    options: PlanOptions,
    plan: RepairPlan,
    mirrors: BTreeSet<String>,
}

impl Planner<'_> {
    fn run(mut self) -> RepairPlan {
        for issue in &self.report.issues {
            self.plan_issue(issue);
        }
        self.plan.steps.sort_by_key(|step| phase(&step.action));
        self.plan
    }

    fn step(&mut self, action: RepairAction, issue: &HealthIssue, evidence: impl Into<String>) {
        self.plan.steps.push(RepairStep {
            action,
            skill: issue.skill.clone(),
            agent_id: issue.agent_id.clone(),
            path: issue.path.clone(),
            evidence: evidence.into(),
        });
    }

    fn leave(&mut self, issue: &HealthIssue, reason: impl Into<String>) {
        self.plan.untouched.push(UntouchedIssue {
            issue: issue.clone(),
            reason: reason.into(),
        });
    }

    /// Whether the canonical folder of `name` exists or a planned step
    /// brings it back.
    fn canonical_available(&self, name: &str) -> bool {
        let path = self.report.canonical_root.join(name);
        let restored = self.report.issues.iter().any(|issue| {
            issue.skill.as_deref() == Some(name)
                && issue.agent_id.is_none()
                && self.reinstalls(issue)
        });
        path.exists() || restored
    }

    /// Whether this canonical issue is repaired by fetching the Skill again.
    fn reinstalls(&self, issue: &HealthIssue) -> bool {
        match &issue.kind {
            IssueKind::LockWithoutCanonical { updatable, .. } => {
                *updatable && self.options.reinstall_from_lock && !self.channel_owned(issue)
            }
            IssueKind::LegacyReinstallPending { .. } => self.report.lock_writable,
            IssueKind::BrokenLink { .. } => issue.skill.as_deref().is_some_and(|name| {
                self.report
                    .lock_entries
                    .get(name)
                    .is_some_and(|entry| entry.source_type.is_updatable())
                    && self.options.reinstall_from_lock
                    && !self.channel_owned(issue)
            }),
            _ => false,
        }
    }

    fn channel_owned(&self, issue: &HealthIssue) -> bool {
        issue.skill.as_deref().is_some_and(|name| {
            crate::skill_mutation::skill_is_channel_managed(name).unwrap_or(true)
        })
    }

    fn plan_issue(&mut self, issue: &HealthIssue) {
        if issue.agent_id.is_none() && self.channel_owned(issue) {
            return self.leave(
                issue,
                "owned by a shared channel; repair it from the channel",
            );
        }
        match &issue.kind {
            IssueKind::LockTooNew { .. } | IssueKind::LockCorrupt { .. } => self.leave(
                issue,
                "the install lock cannot be written safely; no lock change is planned",
            ),
            IssueKind::LockOutdated { .. } => {
                self.leave(issue, "the next install rewrites the lock in the current schema")
            }
            IssueKind::MissingSkillMd => self.leave(issue, "folder content belongs to its author"),
            IssueKind::ForeignLink { .. } => {
                self.leave(issue, "link points outside SkillStar's storage")
            }
            IssueKind::UnmanagedDirectory => {
                self.leave(issue, "no SkillStar deploy marker; the folder is not SkillStar's")
            }
            IssueKind::CanonicalWithoutLock if self.options.adopt_unlocked => self.step(
                RepairAction::AdoptAsLocal,
                issue,
                "explicit adoption; content is copied to local storage before the folder is replaced",
            ),
            IssueKind::CanonicalWithoutLock => self.leave(
                issue,
                "installed by another tool or by hand; adopt it explicitly to manage it",
            ),
            IssueKind::LockWithoutCanonical { source_url, .. } => {
                if self.reinstalls(issue) {
                    self.step(
                        RepairAction::ReinstallFromLock {
                            source_url: source_url.clone(),
                        },
                        issue,
                        "install lock records this Skill and its source",
                    )
                } else {
                    self.step(
                        RepairAction::PruneLockEntry,
                        issue,
                        "install lock entry names a folder that no longer exists",
                    )
                }
            }
            IssueKind::BrokenLink { target } => self.plan_broken_link(issue, target),
            IssueKind::SelfLink => {
                self.step(RepairAction::RemoveSelfLink, issue, "link resolves to itself")
            }
            IssueKind::StaleCopy { modified: true, .. } => self.leave(
                issue,
                "copy changed since SkillStar deployed it (or predates content hashing)",
            ),
            IssueKind::StaleCopy { .. } => {
                let name = issue.skill.as_deref().unwrap_or_default();
                if self.canonical_available(name) {
                    self.step(
                        RepairAction::RefreshCopy,
                        issue,
                        "SkillStar deploy marker; content unchanged since deploy",
                    )
                } else {
                    self.step(
                        RepairAction::RemoveOwnedResidue,
                        issue,
                        "unchanged SkillStar copy of a Skill that is no longer installed",
                    )
                }
            }
            IssueKind::MirrorDrift => {
                let agent = issue.agent_id.clone().unwrap_or_default();
                if self.mirrors.insert(agent) {
                    self.step(
                        RepairAction::ResyncMirror,
                        issue,
                        "mirror sync only replaces SkillStar-owned entries",
                    )
                }
            }
            IssueKind::TransientResidue => self.plan_transient(issue),
            IssueKind::MigrationResidue => self.plan_migration(issue),
            IssueKind::LegacyReinstallPending {
                git_url,
                git_ref,
                source_folder,
            } => {
                if self.report.lock_writable {
                    self.step(
                        RepairAction::ReinstallLegacy {
                            git_url: git_url.clone(),
                            git_ref: git_ref.clone(),
                            source_folder: source_folder.clone(),
                        },
                        issue,
                        "removed by the legacy hub cleanup, which recorded its source",
                    )
                } else {
                    self.leave(issue, "the install lock cannot be written safely")
                }
            }
        }
    }

    fn plan_broken_link(&mut self, issue: &HealthIssue, target: &Path) {
        let name = issue.skill.clone().unwrap_or_default();
        if issue.agent_id.is_some() {
            if self.canonical_available(&name) {
                return self.step(
                    RepairAction::Relink,
                    issue,
                    format!("SkillStar link into {} lost its target", target.display()),
                );
            }
            return self.step(
                RepairAction::RemoveOwnedResidue,
                issue,
                "SkillStar link to a Skill that is no longer installed",
            );
        }
        self.step(
            RepairAction::RemoveOwnedResidue,
            issue,
            "dead link into SkillStar's storage",
        );
        let Some(entry) = self.report.lock_entries.get(&name) else {
            return;
        };
        if self.reinstalls(issue) {
            self.step(
                RepairAction::ReinstallFromLock {
                    source_url: entry.source_url.clone(),
                },
                issue,
                "install lock records this Skill and its source",
            )
        } else {
            self.step(
                RepairAction::PruneLockEntry,
                issue,
                "install lock entry names a folder that no longer exists",
            )
        }
    }

    fn plan_transient(&mut self, issue: &HealthIssue) {
        let name = issue
            .path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default();
        debug_assert!(is_transient(&name));
        let Some((kind, skill)) = transient_parts(&name) else {
            return self.leave(issue, "not a staging name SkillStar recognizes");
        };
        let dest = skill.and_then(|skill| issue.path.parent().map(|dir| dir.join(skill)));
        let target_missing = dest
            .as_ref()
            .is_none_or(|dest| dest.symlink_metadata().is_err());
        match kind {
            "stage" if target_missing => self.leave(
                issue,
                "staged install whose destination is missing; left in place",
            ),
            "stage" => self.step(
                RepairAction::RemoveOwnedResidue,
                issue,
                "SkillStar stage leftover beside an existing skill",
            ),
            "relink" | "importing" => self.step(
                RepairAction::RemoveOwnedResidue,
                issue,
                format!("SkillStar {kind} leftover of an interrupted operation"),
            ),
            "remove" | "backup" | "retain" => {
                let Some(dest) = dest else {
                    return self.leave(issue, "backup without a recognizable destination");
                };
                if dest.symlink_metadata().is_err() {
                    return self.step(
                        RepairAction::RestoreBackup { dest },
                        issue,
                        "interrupted operation whose result never landed",
                    );
                }
                if kind == "remove" {
                    return self.leave(
                        issue,
                        "removal copy kept because the destination is already present",
                    );
                }
                if crate::materialize::transient_replacement_committed(kind, &dest, &issue.path) {
                    self.step(
                        RepairAction::RemoveOwnedResidue,
                        issue,
                        "backup of a replace that was committed",
                    );
                } else {
                    self.leave(
                        issue,
                        "subscription or lock still points at the previous release, or the new content is not committed",
                    );
                }
            }
            _ => self.leave(issue, "not a staging name SkillStar recognizes"),
        }
    }

    fn plan_migration(&mut self, issue: &HealthIssue) {
        match issue.path.file_name().and_then(|name| name.to_str()) {
            Some("skills" | "content" | "lock.json") => self.step(
                RepairAction::RerunLegacyCleanup,
                issue,
                "pre-D-081 hub storage; its Skills are reinstalled from the recorded sources",
            ),
            Some("local") => self.step(
                RepairAction::RerunLocalMigration,
                issue,
                "pre-D-087 local Skill storage; migration never overwrites a newer copy",
            ),
            _ => self.leave(issue, "migration conflict; compare and remove it by hand"),
        }
    }
}

/// Split `.skillstar-{kind}-{skill}[-{uuid}]` / `.importing-{skill}` into
/// its kind and, when present, the Skill folder it belongs to.
pub(super) fn transient_parts(name: &str) -> Option<(&str, Option<&str>)> {
    if let Some(rest) = name.strip_prefix(".importing-") {
        return Some(("importing", Some(rest).filter(|rest| !rest.is_empty())));
    }
    crate::materialize::transient_kind_and_skill(name)
}
