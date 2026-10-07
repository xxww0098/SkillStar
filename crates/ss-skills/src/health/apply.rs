//! Execute a [`RepairPlan`]. Every step re-checks its precondition, so a plan
//! applied twice (or applied after the user fixed things by hand) reports
//! `AlreadyDone` or `Skipped` instead of acting on stale evidence.
//!
//! [`apply_with`] with [`ApplyOptions::dry_run`] holds the same transaction
//! lock and runs the same checks, then reports `WouldApply` without writing.

use anyhow::{Context, Result};
use serde::Serialize;
use ss_core::infra::{fs_ops, paths};

use super::plan::{RepairAction, RepairPlan, RepairStep};
use super::scan::{is_self_link, is_transient};
use crate::deployment::ownership::{self, Ownership};
use crate::git_skill::GitSkillFacade;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum StepStatus {
    Applied,
    /// Dry-run: the checks passed and a real apply would write.
    WouldApply,
    AlreadyDone,
    Skipped {
        reason: String,
    },
    Failed {
        error: String,
    },
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StepOutcome {
    pub step: RepairStep,
    #[serde(flatten)]
    pub status: StepStatus,
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RepairOutcome {
    pub steps: Vec<StepOutcome>,
}

/// Whether [`apply_with`] writes. The default applies the plan.
#[derive(Debug, Clone, Copy, Default)]
pub struct ApplyOptions {
    pub dry_run: bool,
}

impl RepairOutcome {
    pub fn applied_count(&self) -> usize {
        self.count(|status| matches!(status, StepStatus::Applied))
    }

    pub fn would_apply_count(&self) -> usize {
        self.count(|status| matches!(status, StepStatus::WouldApply))
    }

    pub fn failed(&self) -> impl Iterator<Item = &StepOutcome> {
        self.steps
            .iter()
            .filter(|outcome| matches!(outcome.status, StepStatus::Failed { .. }))
    }

    fn count(&self, pred: impl Fn(&StepStatus) -> bool) -> usize {
        self.steps
            .iter()
            .filter(|outcome| pred(&outcome.status))
            .count()
    }
}

fn skipped(reason: impl Into<String>) -> StepStatus {
    StepStatus::Skipped {
        reason: reason.into(),
    }
}

/// Run `plan` step by step under the update transaction lock. One failing
/// step never stops the rest; its error is recorded on its outcome.
pub fn apply(plan: &RepairPlan) -> Result<RepairOutcome> {
    apply_with(plan, ApplyOptions::default())
}

/// Same as [`apply`]. `options.dry_run` reports what would change and writes nothing.
pub fn apply_with(plan: &RepairPlan, options: ApplyOptions) -> Result<RepairOutcome> {
    let _transaction_guard = crate::skill_update::acquire_update_transaction_lock()?;
    let git = GitSkillFacade::from_file_store();
    let mut outcome = RepairOutcome::default();
    for step in &plan.steps {
        let status = run(step, &git, options.dry_run).unwrap_or_else(|error| StepStatus::Failed {
            error: format!("{error:#}"),
        });
        if !matches!(status, StepStatus::Applied | StepStatus::WouldApply) {
            tracing::info!(target: "health", path = %step.path.display(), ?status, "repair step not applied");
        }
        outcome.steps.push(StepOutcome {
            step: step.clone(),
            status,
        });
    }
    if !options.dry_run {
        crate::installed_skill::invalidate_cache();
    }
    Ok(outcome)
}

/// Commit `write` after the caller has checked that the step is still valid.
fn commit(dry_run: bool, write: impl FnOnce() -> Result<()>) -> Result<StepStatus> {
    if dry_run {
        return Ok(StepStatus::WouldApply);
    }
    write()?;
    Ok(StepStatus::Applied)
}

fn canonical(step: &RepairStep) -> Option<std::path::PathBuf> {
    step.skill
        .as_deref()
        .map(|name| paths::hub_skills_dir().join(name))
}

fn run(step: &RepairStep, git: &GitSkillFacade, dry_run: bool) -> Result<StepStatus> {
    let name = step.skill.as_deref().unwrap_or_default();
    match &step.action {
        RepairAction::Relink | RepairAction::RefreshCopy => redeploy(step, name, dry_run),
        RepairAction::RemoveOwnedResidue => remove_residue(step, name, dry_run),
        RepairAction::RemoveSelfLink => {
            if !fs_ops::is_link(&step.path) {
                return Ok(StepStatus::AlreadyDone);
            }
            if !is_self_link(&step.path) {
                return Ok(skipped("the link resolves somewhere now"));
            }
            commit(dry_run, || fs_ops::remove_symlink(&step.path))
        }
        RepairAction::RestoreBackup { dest } => {
            if step.path.symlink_metadata().is_err() {
                return Ok(StepStatus::AlreadyDone);
            }
            if dest.symlink_metadata().is_ok() {
                return Ok(skipped(format!("{} exists again", dest.display())));
            }
            let dest = dest.clone();
            commit(dry_run, || {
                std::fs::rename(&step.path, &dest)
                    .with_context(|| format!("Failed to restore {}", dest.display()))
            })
        }
        RepairAction::ReinstallFromLock { .. } => {
            if canonical(step).is_some_and(|path| path.exists()) {
                return Ok(StepStatus::AlreadyDone);
            }
            let Some(entry) = crate::skill_lock::load()
                .entry_for_folder(name)
                .map(|(_, entry)| entry.clone())
            else {
                return Ok(skipped("the install lock no longer lists this Skill"));
            };
            let folder = entry
                .skill_path
                .as_deref()
                .map(|path| path.trim_end_matches("SKILL.md").trim_end_matches('/'))
                .unwrap_or_default();
            let url = entry.source_url.clone();
            let git_ref = entry.git_ref.clone();
            commit(dry_run, || {
                reinstall(git, name, &url, git_ref.as_deref(), Some(folder))
            })
        }
        RepairAction::ReinstallLegacy {
            git_url,
            git_ref,
            source_folder,
        } => {
            let missing = !canonical(step).is_some_and(|path| path.exists());
            let pending = crate::legacy_cleanup::pending_reinstalls()
                .iter()
                .any(|entry| entry.name == name);
            if !missing && !pending {
                return Ok(StepStatus::AlreadyDone);
            }
            let git_url = git_url.clone();
            let git_ref = git_ref.clone();
            let source_folder = source_folder.clone();
            commit(dry_run, || {
                if missing {
                    reinstall(
                        git,
                        name,
                        &git_url,
                        git_ref.as_deref(),
                        source_folder.as_deref(),
                    )?;
                }
                crate::legacy_cleanup::forget_pending(name)
            })
        }
        RepairAction::PruneLockEntry => {
            if step.path.symlink_metadata().is_ok() && step.path.exists() {
                return Ok(skipped("the Skill folder exists again"));
            }
            if crate::skill_lock::load().entry_for_folder(name).is_none() {
                return Ok(StepStatus::AlreadyDone);
            }
            commit(dry_run, || {
                crate::skill_lock::mutate(|lock| {
                    lock.remove(name);
                })?;
                Ok(())
            })
        }
        RepairAction::AdoptAsLocal => {
            if fs_ops::is_link(&step.path) {
                return Ok(StepStatus::AlreadyDone);
            }
            if crate::skill_lock::load().entry_for_folder(name).is_some() {
                return Ok(skipped("the install lock lists this Skill now"));
            }
            commit(dry_run, || crate::local_skill::adopt_canonical(name))
        }
        RepairAction::ResyncMirror => {
            let agent_id = step.agent_id.as_deref().unwrap_or_default();
            let Some(dir) = agent_dir(agent_id) else {
                return Ok(skipped("the Agent is no longer enabled"));
            };
            if crate::deployment::mirror_drift(agent_id, &dir).is_empty() {
                return Ok(StepStatus::AlreadyDone);
            }
            let agent_id = agent_id.to_string();
            commit(dry_run, || {
                crate::deployment::sync_mirrors(&agent_id, &dir);
                let left = crate::deployment::mirror_drift(&agent_id, &dir);
                if !left.is_empty() {
                    anyhow::bail!(
                        "{} mirror entries are still out of sync (first: {})",
                        left.len(),
                        left[0].display()
                    );
                }
                Ok(())
            })
        }
        RepairAction::RerunLegacyCleanup => {
            if step.path.symlink_metadata().is_err() {
                return Ok(StepStatus::AlreadyDone);
            }
            commit(dry_run, || crate::legacy_cleanup::rerun())
        }
        RepairAction::RerunLocalMigration => {
            if step.path.symlink_metadata().is_err() {
                return Ok(StepStatus::AlreadyDone);
            }
            commit(dry_run, || {
                crate::storage_migration::migrate_local_skills();
                if step.path.symlink_metadata().is_ok() {
                    anyhow::bail!(
                        "{} still holds Skills that conflict with newer local copies; compare and remove them by hand",
                        step.path.display()
                    );
                }
                Ok(())
            })
        }
    }
}

fn agent_dir(agent_id: &str) -> Option<std::path::PathBuf> {
    super::scan::agent_dirs()
        .into_iter()
        .find(|(id, _)| id == agent_id)
        .map(|(_, dir)| dir)
}

fn resync_agent_mirror(step: &RepairStep) {
    if let Some(agent_id) = step.agent_id.as_deref()
        && let Some(dir) = step.path.parent()
    {
        crate::deployment::sync_mirrors(agent_id, dir);
    }
}

fn redeploy(step: &RepairStep, name: &str, dry_run: bool) -> Result<StepStatus> {
    let Some(source) = canonical(step).filter(|path| path.is_dir()) else {
        return Ok(skipped("the canonical Skill folder is missing"));
    };
    let unchanged_since_deploy = || {
        matches!(
            (ownership::marker_content_hash(&step.path), ownership::dir_content_hash(&step.path)),
            (Some(marked), Ok(current)) if marked == current
        )
    };
    match ownership::owned_deployment(&step.path, name) {
        Ownership::Foreign => return Ok(skipped("the entry is no longer SkillStar's")),
        Ownership::Link { alive: true }
            if std::fs::canonicalize(&step.path).ok() == std::fs::canonicalize(&source).ok() =>
        {
            return Ok(StepStatus::AlreadyDone);
        }
        Ownership::Copy => {
            if matches!(
                (ownership::dir_content_hash(&step.path), ownership::dir_content_hash(&source)),
                (Ok(a), Ok(b)) if a == b
            ) {
                return Ok(StepStatus::AlreadyDone);
            }
            if !unchanged_since_deploy() {
                return Ok(skipped("the copy was edited after deployment"));
            }
        }
        _ => {}
    }
    commit(dry_run, || {
        crate::deployment::swap_in_fresh_deploy(&source, &step.path, name)?;
        resync_agent_mirror(step);
        Ok(())
    })
}

fn remove_residue(step: &RepairStep, name: &str, dry_run: bool) -> Result<StepStatus> {
    let path = &step.path;
    if path.symlink_metadata().is_err() {
        return Ok(StepStatus::AlreadyDone);
    }
    let file_name = path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();
    let still_owned = is_transient(&file_name)
        || match ownership::owned_deployment(path, name) {
            Ownership::Link { alive } => !alive,
            Ownership::Copy => {
                !canonical_present(name)
                    && matches!(
                        (ownership::marker_content_hash(path), ownership::dir_content_hash(path)),
                        (Some(marked), Ok(current)) if marked == current
                    )
            }
            Ownership::Missing | Ownership::Foreign => false,
        };
    if !still_owned {
        return Ok(skipped("no longer provably SkillStar's"));
    }
    commit(dry_run, || {
        crate::materialize::remove_entry(path)?;
        resync_agent_mirror(step);
        Ok(())
    })
}

fn canonical_present(name: &str) -> bool {
    paths::hub_skills_dir().join(name).is_dir()
}

fn reinstall(
    git: &GitSkillFacade,
    name: &str,
    url: &str,
    git_ref: Option<&str>,
    folder: Option<&str>,
) -> Result<()> {
    let Some(folder) = folder else {
        crate::skill_install::install_skills_batch_in_session(
            url,
            &[name.to_string()],
            git.session(),
        )
        .map_err(anyhow::Error::msg)?;
        return Ok(());
    };
    let mut spec = crate::source_resolver::Source::parse(url)?;
    if git_ref.is_some() {
        spec.git_ref = git_ref.map(str::to_string);
    }
    let policy = crate::skill_mutation::policy();
    policy.ensure_repository_mutation_allowed(&spec.repo_url)?;
    policy.ensure_skill_mutation_allowed(name)?;
    let checkout = crate::fetch::fetch_for_install(&spec, &[folder], git.session())?;
    crate::installer::install_units(
        checkout.dir(),
        &spec,
        &[crate::installer::InstallUnit {
            id: name.to_string(),
            folder_path: folder.to_string(),
        }],
    )?;
    Ok(())
}
