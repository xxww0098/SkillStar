//! Presentation-agnostic facade for Git-backed Skill operations.
//!
//! GUI commands and future CLI surfaces use this entry so private credentials,
//! cancellation, and progress cannot be bypassed by calling lower layers.

use crate::git::transport::GitOperationSession;
use crate::git::transport::NoopGitProgressSink;
use crate::github_auth::{
    FileCredentialStore, GitHubAuthFacade, ProductionGitHubGateway, SystemClock,
};
use crate::installed_skill::{self, SkillUpdateState};
use crate::repo_scanner::{ScanResult, SkillInstallTarget};
use crate::skill_update::{SkillUpdateChannelManaged, SkillUpdateFailure, SkillUpdateReport, UpdateResult};
use crate::{Skill, local_skill, skill_install};
use skillstar_core::infra::error::AppError;
use anyhow::Context as _;
use std::path::Path;
use std::sync::Arc;

#[derive(Clone, Debug)]
pub struct GitSkillFacade {
    session: GitOperationSession,
}

impl GitSkillFacade {
    pub fn new(session: GitOperationSession) -> Self {
        Self { session }
    }

    pub fn from_file_store() -> Self {
        let auth = GitHubAuthFacade::new(
            ProductionGitHubGateway::from_environment(),
            FileCredentialStore::default(),
            SystemClock,
        );
        Self::new(GitOperationSession::new(
            uuid::Uuid::new_v4().to_string(),
            auth.git_auth_material().unwrap_or_else(|error| {
                crate::git::transport::GitAuthMaterial::unavailable(error.to_string())
            }),
            Arc::new(NoopGitProgressSink),
        ))
    }

    pub fn session(&self) -> &GitOperationSession {
        &self.session
    }

    pub fn cancel(&self) {
        self.session.cancel();
    }

    pub fn scan_repo(&self, input: &str, full_depth: bool) -> anyhow::Result<ScanResult> {
        ensure_generic_input_repository_mutable(input)?;
        let parsed =
            crate::source_resolver::Source::parse(input).context("Invalid repository URL")?;
        self.session.emit_stage(
            crate::git::transport::InstallStage::Fetching,
            &parsed.short,
            None,
        );
        let checkout = crate::fetch::fetch_source(&parsed, &self.session)?;
        self.session.emit_stage(
            crate::git::transport::InstallStage::Discovering,
            &parsed.short,
            None,
        );
        let (_, _, repo_dir, skills) =
            crate::skill_install::scan_parsed_checkout(&parsed, checkout.dir().to_path_buf(), full_depth);
        let plugin = crate::plugin_manifest::plugin_hint_for_repo(&repo_dir);
        Ok(ScanResult {
            spec: parsed,
            skills,
            plugin,
        })
    }

    pub fn fetch_repo_scanned(
        &self,
        input: &str,
        full_depth: bool,
    ) -> anyhow::Result<skill_install::FetchedScan> {
        skill_install::fetch_repo_scanned_in_session(input, full_depth, &self.session)
            .map_err(anyhow::Error::msg)
    }

    pub fn install_from_scan(
        &self,
        spec: &crate::source_resolver::Source,
        targets: &[SkillInstallTarget],
    ) -> anyhow::Result<Vec<String>> {
        self.session.emit_stage(
            crate::git::transport::InstallStage::Fetching,
            &spec.short,
            None,
        );
        let checkout = crate::fetch::fetch_source(spec, &self.session)?;
        self.session.emit_stage(
            crate::git::transport::InstallStage::Materializing,
            &spec.short,
            None,
        );
        let units: Vec<crate::installer::InstallUnit> = targets
            .iter()
            .map(|target| crate::installer::InstallUnit {
                id: target.id.clone(),
                folder_path: target.folder_path.clone(),
            })
            .collect();
        let installed = crate::installer::install_units(checkout.dir(), spec, &units)?;
        installed_skill::invalidate_cache();
        Ok(installed)
    }

    /// Replace a published local Skill with its Git-backed installation while
    /// preserving the local snapshot if the staged install fails.
    pub fn graduate_local_skill_from_scan(
        &self,
        skill_name: &str,
        spec: &crate::source_resolver::Source,
        target: &SkillInstallTarget,
    ) -> anyhow::Result<()> {
        let _guard = crate::skill_update::acquire_update_transaction_lock()?;
        crate::skill_mutation::policy().ensure_skill_mutation_allowed(skill_name)?;
        crate::skill_mutation::policy().ensure_repository_mutation_allowed(&spec.repo_url)?;
        let snapshot = crate::content::snapshot(skill_name)?;
        local_skill::graduate(skill_name)?;
        let install = (|| -> anyhow::Result<Vec<String>> {
            let checkout = crate::fetch::fetch_source(spec, &self.session)?;
            crate::installer::install_units(
                checkout.dir(),
                spec,
                std::slice::from_ref(&crate::installer::InstallUnit {
                    id: target.id.clone(),
                    folder_path: target.folder_path.clone(),
                }),
            )
        })()
        .and_then(|installed| {
            installed
                .iter()
                .any(|installed| installed.eq_ignore_ascii_case(&target.id))
                .then_some(())
                .ok_or_else(|| {
                    anyhow::anyhow!(
                        "Published repository no longer contains Skill '{}'",
                        target.id
                    )
                })
        });
        if let Err(error) = install {
            let content_rollback = local_skill::create_from_snapshot(skill_name, &snapshot).err();
            installed_skill::invalidate_cache();
            return Err(match content_rollback {
                None => error,
                Some(content) => anyhow::anyhow!(
                    "Git-backed installation failed ({error:#}); local Skill restore also failed: {content:#}"
                ),
            });
        }
        installed_skill::invalidate_cache();
        Ok(())
    }

    /// Apply the ordinary staged repository installer to an already fetched,
    /// immutable checkout. Both sides of the transaction verify HEAD so a
    /// caller cannot record a requested commit that was not actually installed.
    pub fn install_verified_checkout(
        &self,
        repo_dir: &Path,
        repo_url: &str,
        expected_commit: &str,
        targets: &[SkillInstallTarget],
    ) -> anyhow::Result<Vec<String>> {
        let before = crate::git::ops::rev_parse(repo_dir, "HEAD")?;
        if !before.eq_ignore_ascii_case(expected_commit) {
            anyhow::bail!(
                "repository checkout is at {before}, expected immutable commit {expected_commit}"
            );
        }
        let installed = install_targets_at(repo_dir, repo_url, Some(expected_commit), targets)?;
        let after = crate::git::ops::rev_parse(repo_dir, "HEAD")?;
        if !after.eq_ignore_ascii_case(expected_commit) {
            anyhow::bail!(
                "repository checkout changed to {after} while installing immutable commit {expected_commit}"
            );
        }
        installed_skill::invalidate_cache();
        Ok(installed)
    }

    pub fn replace_verified_channel_checkout(
        &self,
        repo_dir: &Path,
        repo_url: &str,
        previous_repo_url: &str,
        expected_commit: &str,
        targets: &[SkillInstallTarget],
    ) -> anyhow::Result<Vec<String>> {
        let before = crate::git::ops::rev_parse(repo_dir, "HEAD")?;
        if !before.eq_ignore_ascii_case(expected_commit) {
            anyhow::bail!(
                "repository checkout is at {before}, expected immutable commit {expected_commit}"
            );
        }
        let _ = previous_repo_url;
        let installed = install_targets_at(repo_dir, repo_url, Some(expected_commit), targets)?;
        let after = crate::git::ops::rev_parse(repo_dir, "HEAD")?;
        if !after.eq_ignore_ascii_case(expected_commit) {
            anyhow::bail!(
                "repository checkout changed to {after} while installing immutable commit {expected_commit}"
            );
        }
        installed_skill::invalidate_cache();
        Ok(installed)
    }

    pub fn install_skill(&self, url: String, name: Option<String>) -> Result<Skill, String> {
        skill_install::install_skill_in_session(url, name, None, &self.session)
    }

    pub fn install_skill_for_agent(
        &self,
        url: String,
        name: Option<String>,
        agent_id: &str,
    ) -> Result<Skill, AppError> {
        skill_install::install_skill_in_session(url, name, Some(agent_id), &self.session)
            .map_err(AppError::from)
    }

    pub fn install_skills_batch(&self, url: &str, names: &[String]) -> Result<Vec<Skill>, String> {
        skill_install::install_skills_batch_in_session(url, names, &self.session)
    }

    pub fn install_skills_batch_for_agent(
        &self,
        url: &str,
        names: &[String],
        _agent_id: &str,
    ) -> Result<Vec<Skill>, AppError> {
        skill_install::install_skills_batch_in_session(url, names, &self.session)
            .map_err(AppError::from)
    }

    /// D-081 overwrite-update: refetch the locked source and reinstall.
    pub fn update_skill(&self, name: &str) -> anyhow::Result<UpdateResult> {
        let report = self.update_skills(std::slice::from_ref(&name.to_string()));
        if let Some(managed) = report.channel_managed.first() {
            return Err(anyhow::anyhow!(
                "'{}' is managed by shared channel {}; update it from the shared-channel view",
                managed.name, managed.repository_id
            ));
        }
        if report.updated.is_empty() {
            return Err(anyhow::anyhow!("{}", first_failure_text(&report)));
        }
        Ok(report.updated.into_iter().next().expect("checked non-empty"))
    }

    pub fn update_skills(&self, names: &[String]) -> SkillUpdateReport {
        let policy = crate::skill_mutation::policy();
        let mut report = SkillUpdateReport::default();
        let mut generic: Vec<String> = Vec::new();
        for name in names {
            match policy.managed_repository_for_skill(name) {
                Ok(Some(repository_id)) => {
                    report.channel_managed.push(SkillUpdateChannelManaged {
                        name: name.clone(),
                        repository_id,
                    });
                }
                Ok(None) => generic.push(name.clone()),
                Err(error) => report.failed.push(SkillUpdateFailure {
                    name: name.clone(),
                    error: format!("{error:#}"),
                }),
            }
        }
        for applied in crate::update::apply_updates(&generic, &self.session) {
            match applied.result {
                crate::update::UpdateResult::Updated { .. } => {
                    // Refresh every existing link/copy onto the new content.
                    let _ = crate::deployment::resync_existing_links(&applied.name);
                    crate::update_state::set(&applied.name, false);
                    match crate::skill_install::load_skill_dto(&applied.name) {
                        Ok(skill) => report.updated.push(UpdateResult {
                            skill,
                            siblings_cleared: Vec::new(),
                            agent_link_failures: Vec::new(),
                        }),
                        Err(error) => report.failed.push(SkillUpdateFailure {
                            name: applied.name.clone(),
                            error,
                        }),
                    }
                }
                crate::update::UpdateResult::Removed => report.skipped.push(applied.name.clone()),
                crate::update::UpdateResult::Failed(error) => report.failed.push(SkillUpdateFailure {
                    name: applied.name.clone(),
                    error,
                }),
            }
        }
        installed_skill::invalidate_cache();
        report
    }

    pub async fn refresh_skill_updates(&self) -> anyhow::Result<Vec<SkillUpdateState>> {
        installed_skill::refresh_skill_updates_in_session(&self.session).await
    }
}

/// Install `targets` out of an already-verified checkout dir (channels seam).
fn install_targets_at(
    repo_dir: &Path,
    repo_url: &str,
    git_ref: Option<&str>,
    targets: &[SkillInstallTarget],
) -> anyhow::Result<Vec<String>> {
    let spec = crate::source_resolver::Source {
        repo_url: repo_url.to_string(),
        short: crate::source_resolver::cache_dir_name(repo_url),
        git_ref: git_ref.map(str::to_string),
        subpath: None,
        skill_filter: None,
    };
    let units: Vec<crate::installer::InstallUnit> = targets
        .iter()
        .map(|target| crate::installer::InstallUnit {
            id: target.id.clone(),
            folder_path: target.folder_path.clone(),
        })
        .collect();
    crate::installer::install_units(repo_dir, &spec, &units)
}

fn first_failure_text(report: &SkillUpdateReport) -> String {
    report
        .failed
        .first()
        .map(|failure| format!("{}: {}", failure.name, failure.error))
        .or_else(|| {
            report
                .skipped
                .first()
                .map(|name| format!("'{name}' is no longer available upstream"))
        })
        .unwrap_or_else(|| "update produced no result".to_string())
}

fn ensure_generic_input_repository_mutable(input: &str) -> anyhow::Result<()> {
    let source = crate::source_resolver::Source::parse(input)?;
    crate::skill_mutation::policy().ensure_repository_mutation_allowed(&source.repo_url)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The GUI scan preview (`scan_github_repo` → `GitSkillFacade::scan_repo`)
    /// must resolve a tree URL's ref and honor its subpath the same way the
    /// install pipeline does, so what the user previews is what gets pinned.
    #[test]
    fn scan_repo_honors_tree_url_ref_and_subpath() {
        let _sandbox = crate::pack_fixture::Sandbox::new();
        let fixture = crate::pack_fixture::impeccable_like();
        let github_url = "https://github.com/pbakaus/impeccable.git";
        _sandbox.map_github_url(github_url, fixture.dir.path());

        let facade = GitSkillFacade::new(GitOperationSession::public());
        let scan = facade
            .scan_repo(
                "https://github.com/pbakaus/impeccable/tree/main/.claude/skills/impeccable",
                false,
            )
            .unwrap();

        assert_eq!(scan.spec.git_ref.as_deref(), Some("main"));
        assert_eq!(
            scan.spec.subpath.as_deref(),
            Some(".claude/skills/impeccable")
        );
        assert_eq!(scan.skills.len(), 1, "{:?}", scan.skills);
        assert_eq!(scan.skills[0].folder_path, ".claude/skills/impeccable");
        let plugin = scan.plugin.expect("impeccable declares a plugin manifest");
        assert!(plugin.hooks);
        assert!(plugin.agents);
    }
}
