//! Presentation-agnostic facade for Git-backed Skill operations.
//!
//! GUI commands and future CLI surfaces use this entry so private credentials,
//! cancellation, and progress cannot be bypassed by calling lower layers.

use crate::git::transport::GitOperationSession;
use crate::git::transport::NoopGitProgressSink;
use crate::github_auth::{
    GitHubAuthFacade, KeyringCredentialStore, ProductionGitHubGateway, SystemClock,
};
use crate::installed_skill::{self, SkillUpdateState};
use crate::repo_scanner::{self, ScanResult, SkillInstallTarget};
use crate::skill_update::{
    LocalDivergenceResolution, ResolveSkillUpdateResult, SkillUpdateReport, UpdateResult,
};
use crate::{Skill, skill_install, skill_update};
use std::path::{Path, PathBuf};
use std::sync::Arc;

#[derive(Clone, Debug)]
pub struct GitSkillFacade {
    session: GitOperationSession,
}

impl GitSkillFacade {
    pub fn new(session: GitOperationSession) -> Self {
        Self { session }
    }

    pub fn from_keyring() -> Self {
        let auth = GitHubAuthFacade::new(
            ProductionGitHubGateway::from_environment(),
            KeyringCredentialStore,
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
        repo_scanner::scan_repo_with_mode_in_session(input, full_depth, &self.session)
    }

    pub fn discover_skills(
        &self,
        input: &str,
        full_depth: bool,
    ) -> Result<Vec<repo_scanner::DiscoveredSkill>, String> {
        skill_install::fetch_repo_scanned_in_session(input, full_depth, &self.session)
            .map(|(_, _, _, skills)| skills)
    }

    pub fn fetch_repo_scanned(
        &self,
        input: &str,
        full_depth: bool,
    ) -> Result<(String, String, PathBuf, Vec<repo_scanner::DiscoveredSkill>), String> {
        skill_install::fetch_repo_scanned_in_session(input, full_depth, &self.session)
    }

    pub(crate) fn fetch_repo_scanned_detailed(
        &self,
        input: &str,
        full_depth: bool,
    ) -> anyhow::Result<(String, String, PathBuf, Vec<repo_scanner::DiscoveredSkill>)> {
        skill_install::fetch_repo_scanned_detailed_in_session(input, full_depth, &self.session)
    }

    pub fn install_from_scan(
        &self,
        source: &str,
        repo_url: &str,
        targets: &[SkillInstallTarget],
    ) -> anyhow::Result<Vec<String>> {
        repo_scanner::install_from_repo_in_session(source, repo_url, targets, &self.session)
    }

    /// Apply the ordinary staged repository installer to an already fetched,
    /// immutable checkout. Both sides of the transaction verify HEAD so a
    /// caller cannot record a requested commit that was not actually installed.
    pub(crate) fn install_verified_checkout(
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
        let installed =
            repo_scanner::install_from_repo_at(repo_dir, repo_url, Some(expected_commit), targets)?;
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
        skill_install::install_skill_in_session(url, name, &self.session)
    }

    pub fn install_skills_batch(&self, url: &str, names: &[String]) -> Result<Vec<Skill>, String> {
        skill_install::install_skills_batch_in_session(url, names, &self.session)
    }

    pub fn install_skill_pack(&self, url: String) -> Result<Vec<String>, String> {
        skill_install::install_skill_pack_in_session(url, &self.session)
    }

    pub fn update_skill(&self, name: &str) -> anyhow::Result<UpdateResult> {
        skill_update::update_skill_in_session(name, &self.session)
    }

    pub fn update_skills(&self, names: &[String]) -> SkillUpdateReport {
        skill_update::update_skills_in_session(names, &self.session)
    }

    pub fn resolve_skill_update(
        &self,
        name: &str,
        resolution: LocalDivergenceResolution,
    ) -> anyhow::Result<ResolveSkillUpdateResult> {
        skill_update::resolve_skill_update_in_session(name, resolution, &self.session)
    }

    pub async fn refresh_skill_updates(&self) -> anyhow::Result<Vec<SkillUpdateState>> {
        installed_skill::refresh_skill_updates_in_session(&self.session).await
    }
}
