//! Read-only health scan. Nothing here writes to disk.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use serde::Serialize;
use ss_core::infra::{fs_ops, paths};

use crate::deployment::ownership::{self, Ownership};
use crate::skill_lock::{LockFileState, SkillLock, SkillLockEntry};

/// Name prefixes SkillStar reserves for entries that only exist mid-operation.
pub(super) const TRANSIENT_PREFIXES: &[&str] = &[".skillstar-", ".importing-"];

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum IssueKind {
    /// A canonical folder no lock entry accounts for.
    CanonicalWithoutLock,
    /// A lock entry whose canonical folder is gone.
    LockWithoutCanonical { source_url: String, updatable: bool },
    /// A canonical folder without `SKILL.md`.
    MissingSkillMd,
    /// A SkillStar link (into a managed root) whose target is gone.
    BrokenLink { target: PathBuf },
    /// A link in an Agent directory to somewhere SkillStar does not manage.
    ForeignLink { target: PathBuf },
    /// A link that resolves to itself.
    SelfLink,
    /// A real directory in an Agent directory that shares an installed
    /// Skill's name but carries no SkillStar marker. Never touched.
    UnmanagedDirectory,
    /// A marked copy deployment whose content no longer matches canonical.
    StaleCopy {
        /// The copy itself was edited after deployment.
        modified: bool,
        canonical_missing: bool,
    },
    /// A mirror entry (e.g. Antigravity's) out of sync with its Agent.
    MirrorDrift,
    /// Staging, backup or removal leftovers of an interrupted operation.
    TransientResidue,
    /// Pre-D-081 / pre-D-087 storage still on disk.
    MigrationResidue,
    /// A Skill the legacy cleanup removed and promised to reinstall.
    LegacyReinstallPending {
        git_url: String,
        git_ref: Option<String>,
        source_folder: Option<String>,
    },
    /// The install lock uses a newer schema than this SkillStar.
    LockTooNew { version: u64 },
    /// The install lock cannot be parsed.
    LockCorrupt { error: String },
    /// The install lock uses an older schema and is reset on next write.
    LockOutdated { version: u64 },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HealthIssue {
    #[serde(flatten)]
    pub kind: IssueKind,
    pub skill: Option<String>,
    /// The Agent whose directory holds `path`, when it is one.
    pub agent_id: Option<String>,
    pub path: PathBuf,
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillHealthReport {
    pub canonical_root: PathBuf,
    /// Canonical entries that are healthy Skills.
    pub healthy_count: usize,
    pub issues: Vec<HealthIssue>,
    /// Whether the lock can be written; repair never touches it otherwise.
    pub lock_writable: bool,
    #[serde(skip)]
    pub(super) lock_entries: BTreeMap<String, SkillLockEntry>,
}

impl SkillHealthReport {
    pub fn is_healthy(&self) -> bool {
        self.issues.is_empty()
    }
}

fn issue(
    kind: IssueKind,
    skill: Option<&str>,
    agent_id: Option<&str>,
    path: PathBuf,
) -> HealthIssue {
    HealthIssue {
        kind,
        skill: skill.map(str::to_string),
        agent_id: agent_id.map(str::to_string),
        path,
    }
}

pub(super) fn is_transient(name: &str) -> bool {
    if name.starts_with(".skillstar-born-") {
        return false;
    }
    TRANSIENT_PREFIXES
        .iter()
        .any(|prefix| name.starts_with(prefix))
}

/// Scan the canonical root, the lock, every enabled Agent directory and its
/// mirrors, and the legacy storage locations.
pub fn scan() -> SkillHealthReport {
    let root = paths::hub_skills_dir();
    let mut report = SkillHealthReport {
        canonical_root: root.clone(),
        ..Default::default()
    };

    let lock_state = SkillLock::read_state(&crate::skill_lock::lock_path());
    let lock_path = crate::skill_lock::lock_path();
    let lock = match lock_state {
        LockFileState::Ready(lock) => Some(lock),
        LockFileState::Missing => Some(SkillLock::default()),
        LockFileState::Outdated(version) => {
            report.issues.push(issue(
                IssueKind::LockOutdated { version },
                None,
                None,
                lock_path,
            ));
            Some(SkillLock::default())
        }
        LockFileState::TooNew(version) => {
            report.issues.push(issue(
                IssueKind::LockTooNew { version },
                None,
                None,
                lock_path,
            ));
            None
        }
        LockFileState::Corrupt(error) => {
            report.issues.push(issue(
                IssueKind::LockCorrupt { error },
                None,
                None,
                lock_path,
            ));
            None
        }
    };
    report.lock_writable = lock.is_some();
    if let Some(lock) = &lock {
        let mut folders = BTreeSet::new();
        for key in lock.skills.keys() {
            if let Some(folder) = crate::skill_lock::folder_for_key(key) {
                folders.insert(folder);
            }
        }
        report.lock_entries = folders
            .iter()
            .filter_map(|folder| {
                lock.entry_for_folder(folder)
                    .map(|(_, entry)| (folder.clone(), entry.clone()))
            })
            .collect();
        for folder in &folders {
            if root.join(folder).symlink_metadata().is_err() {
                let entry = report.lock_entries.get(folder);
                report.issues.push(issue(
                    IssueKind::LockWithoutCanonical {
                        source_url: entry
                            .map(|entry| entry.source_url.clone())
                            .unwrap_or_default(),
                        updatable: entry.is_some_and(|entry| entry.source_type.is_updatable()),
                    },
                    Some(folder),
                    None,
                    root.join(folder),
                ));
            }
        }
    }
    let installed = scan_canonical(&root, lock.as_ref(), &mut report);
    scan_agents(&installed, &mut report);
    scan_legacy(&mut report);
    report
}

/// Classify canonical entries; returns the names of present Skills.
fn scan_canonical(
    root: &Path,
    lock: Option<&SkillLock>,
    report: &mut SkillHealthReport,
) -> BTreeSet<String> {
    let mut installed = BTreeSet::new();
    let Ok(entries) = std::fs::read_dir(root) else {
        return installed;
    };
    let local_root = fs_ops::canonicalize_existing_prefix(&paths::local_skills_dir());
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        let path = entry.path();
        if is_transient(&name) {
            report
                .issues
                .push(issue(IssueKind::TransientResidue, None, None, path));
            continue;
        }
        if name.starts_with('.') {
            continue;
        }
        if fs_ops::is_link(&path) {
            if is_self_link(&path) {
                report
                    .issues
                    .push(issue(IssueKind::SelfLink, Some(&name), None, path));
                continue;
            }
            if !path.exists() {
                let target = fs_ops::read_link_resolved(&path).unwrap_or_default();
                let kind = if ownership::owned_deployment(&path, &name).is_owned() {
                    IssueKind::BrokenLink { target }
                } else {
                    IssueKind::ForeignLink { target }
                };
                report.issues.push(issue(kind, Some(&name), None, path));
                continue;
            }
        } else if !path.is_dir() {
            continue;
        }
        installed.insert(name.clone());
        if !path.join("SKILL.md").is_file() {
            report
                .issues
                .push(issue(IssueKind::MissingSkillMd, Some(&name), None, path));
            continue;
        }
        let is_local = fs_ops::is_link(&path)
            && fs_ops::read_link_resolved(&path).is_ok_and(|target| {
                fs_ops::canonicalize_existing_prefix(&target).starts_with(&local_root)
            });
        if let Some(lock) = lock
            && !is_local
            && lock.entry_for_folder(&name).is_none()
        {
            report.issues.push(issue(
                IssueKind::CanonicalWithoutLock,
                Some(&name),
                None,
                path,
            ));
            continue;
        }
        report.healthy_count += 1;
    }
    installed
}

pub(super) fn is_self_link(path: &Path) -> bool {
    let Ok(target) = fs_ops::read_link_resolved(path) else {
        return false;
    };
    target == path
        || std::fs::canonicalize(path).is_err_and(|error| error.raw_os_error() == Some(eloop()))
}

#[cfg(target_os = "macos")]
const fn eloop() -> i32 {
    62
}

#[cfg(not(target_os = "macos"))]
const fn eloop() -> i32 {
    40
}

/// Enabled Agents with their own Global directory; Agents reading the
/// canonical root directly (Pi, Cline) and directories shared by several
/// profiles are visited once.
pub(super) fn agent_dirs() -> Vec<(String, PathBuf)> {
    let mut seen = BTreeSet::new();
    crate::agents::list_profiles()
        .into_iter()
        .filter(|profile| profile.enabled && profile.has_global_skills())
        .filter(|profile| !ownership::targets_canonical_root(&profile.global_skills_dir))
        .filter(|profile| {
            seen.insert(fs_ops::canonicalize_existing_prefix(
                &profile.global_skills_dir,
            ))
        })
        .map(|profile| (profile.id, profile.global_skills_dir))
        .collect()
}

fn scan_agents(installed: &BTreeSet<String>, report: &mut SkillHealthReport) {
    let root = paths::hub_skills_dir();
    for (agent_id, dir) in agent_dirs() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            let path = entry.path();
            let agent = Some(agent_id.as_str());
            if is_transient(&name) {
                report
                    .issues
                    .push(issue(IssueKind::TransientResidue, None, agent, path));
                continue;
            }
            if name.starts_with('.') {
                continue;
            }
            if fs_ops::is_link(&path) && is_self_link(&path) {
                report
                    .issues
                    .push(issue(IssueKind::SelfLink, Some(&name), agent, path));
                continue;
            }
            match ownership::owned_deployment(&path, &name) {
                Ownership::Link { alive: false } => {
                    let target = fs_ops::read_link_resolved(&path).unwrap_or_default();
                    report.issues.push(issue(
                        IssueKind::BrokenLink { target },
                        Some(&name),
                        agent,
                        path,
                    ));
                }
                Ownership::Copy => {
                    if let Some(kind) = stale_copy(&path, &root.join(&name)) {
                        report.issues.push(issue(kind, Some(&name), agent, path));
                    }
                }
                Ownership::Foreign if fs_ops::is_link(&path) => {
                    let target = fs_ops::read_link_resolved(&path).unwrap_or_default();
                    report.issues.push(issue(
                        IssueKind::ForeignLink { target },
                        Some(&name),
                        agent,
                        path,
                    ));
                }
                // A marker whose content hash no longer matches is not owned, so
                // repair will not refresh or delete it. It is still a stale copy.
                Ownership::Foreign
                    if path.is_dir() && ownership::marker_content_hash(&path).is_some() =>
                {
                    report.issues.push(issue(
                        IssueKind::StaleCopy {
                            modified: true,
                            canonical_missing: !root.join(&name).is_dir(),
                        },
                        Some(&name),
                        agent,
                        path,
                    ));
                }
                Ownership::Foreign if installed.contains(&name) && path.is_dir() => {
                    report.issues.push(issue(
                        IssueKind::UnmanagedDirectory,
                        Some(&name),
                        agent,
                        path,
                    ));
                }
                _ => {}
            }
        }
        for drifted in crate::deployment::mirror_drift(&agent_id, &dir) {
            let skill = drifted
                .file_name()
                .map(|name| name.to_string_lossy().into_owned());
            report.issues.push(issue(
                IssueKind::MirrorDrift,
                skill.as_deref(),
                Some(&agent_id),
                drifted,
            ));
        }
    }
}

fn stale_copy(copy: &Path, canonical: &Path) -> Option<IssueKind> {
    let current = ownership::dir_content_hash(copy).ok()?;
    let modified = ownership::marker_content_hash(copy).is_none_or(|marked| marked != current);
    if !canonical.is_dir() {
        return Some(IssueKind::StaleCopy {
            modified,
            canonical_missing: true,
        });
    }
    let source = ownership::dir_content_hash(canonical).ok()?;
    (source != current).then_some(IssueKind::StaleCopy {
        modified,
        canonical_missing: false,
    })
}

fn overrides_active() -> bool {
    ["SKILLSTAR_HUB_DIR", "SKILLSTAR_DATA_DIR"]
        .iter()
        .any(|key| {
            std::env::var(key)
                .map(|value| !value.trim().is_empty())
                .unwrap_or(false)
        })
}

fn scan_legacy(report: &mut SkillHealthReport) {
    for pending in crate::legacy_cleanup::pending_reinstalls() {
        let path = paths::hub_skills_dir().join(&pending.name);
        report.issues.push(issue(
            IssueKind::LegacyReinstallPending {
                git_url: pending.git_url,
                git_ref: pending.git_ref,
                source_folder: pending.source_folder,
            },
            Some(&pending.name),
            None,
            path,
        ));
    }
    // Under path overrides the legacy hub *is* the live hub root, so its
    // subtrees are not residue.
    if overrides_active() {
        return;
    }
    let legacy = paths::legacy_hub_root();
    for leftover in ["skills", "content", "local", "lock.json"] {
        let path = legacy.join(leftover);
        if path.symlink_metadata().is_ok() {
            report
                .issues
                .push(issue(IssueKind::MigrationResidue, None, None, path));
        }
    }
    let v1 = paths::data_root().join(".agents");
    if v1.symlink_metadata().is_ok() {
        report
            .issues
            .push(issue(IssueKind::MigrationResidue, None, None, v1));
    }
}
