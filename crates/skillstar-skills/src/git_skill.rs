//! Presentation-agnostic facade for Git-backed Skill operations.
//!
//! GUI commands and future CLI surfaces use this entry so private credentials,
//! cancellation, and progress cannot be bypassed by calling lower layers.

use crate::git::transport::GitOperationSession;
use crate::git::transport::NoopGitProgressSink;
use crate::installed_skill::{self, SkillUpdateState};
use crate::repo_scanner::{self, ScanResult, SkillInstallTarget};
use crate::skill_update::{
    LocalDivergenceResolution, ResolveSkillUpdateResult, SkillUpdateReport, UpdateResult,
};
use crate::{Skill, local_skill, skill_install, skill_update};
use skillstar_github_auth::{
    FileCredentialStore, GitHubAuthFacade, ProductionGitHubGateway, SystemClock,
};
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
        let _guard = crate::skill_update::acquire_update_transaction_lock()?;
        ensure_generic_input_repository_mutable(input)?;
        repo_scanner::scan_repo_with_mode_in_session(input, full_depth, &self.session)
    }

    pub fn fetch_repo_scanned(
        &self,
        input: &str,
        full_depth: bool,
    ) -> Result<(String, String, PathBuf, Vec<repo_scanner::DiscoveredSkill>), String> {
        skill_install::fetch_repo_scanned_in_session(input, full_depth, &self.session)
    }

    pub fn fetch_repo_scanned_detailed(
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
        let _guard = crate::skill_update::acquire_update_transaction_lock()?;
        repo_scanner::install_from_repo_in_session(source, repo_url, targets, &self.session)
    }

    /// Replace a published local Skill with its Git-backed installation while
    /// preserving the local snapshot if the staged install fails.
    pub fn graduate_local_skill_from_scan(
        &self,
        skill_name: &str,
        source: &str,
        repo_url: &str,
        target: &SkillInstallTarget,
    ) -> anyhow::Result<()> {
        let _guard = crate::skill_update::acquire_update_transaction_lock()?;
        crate::skill_mutation::policy().ensure_skill_mutation_allowed(skill_name)?;
        crate::skill_mutation::policy().ensure_repository_mutation_allowed(repo_url)?;
        let snapshot = crate::content::snapshot(skill_name)?;
        let previous_lock_entry = load_skill_lock_entry(skill_name)?;
        local_skill::graduate(skill_name)?;
        let install = repo_scanner::install_from_repo_in_session(
            source,
            repo_url,
            std::slice::from_ref(target),
            &self.session,
        )
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
            let lock_rollback = restore_skill_lock_entry(skill_name, previous_lock_entry).err();
            installed_skill::invalidate_cache();
            return Err(match (content_rollback, lock_rollback) {
                (None, None) => error,
                (Some(content), None) => anyhow::anyhow!(
                    "Git-backed installation failed ({error:#}); local Skill restore also failed: {content:#}"
                ),
                (None, Some(lock)) => anyhow::anyhow!(
                    "Git-backed installation failed ({error:#}); lockfile restore also failed: {lock:#}"
                ),
                (Some(content), Some(lock)) => anyhow::anyhow!(
                    "Git-backed installation failed ({error:#}); local Skill restore failed: {content:#}; lockfile restore failed: {lock:#}"
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
        let installed = repo_scanner::scan_install::install_from_repo_at_with_source_migrations(
            repo_dir,
            repo_url,
            Some(expected_commit),
            targets,
            &[],
        )?;
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
        let authorized = targets
            .iter()
            .map(|target| (target.id.clone(), previous_repo_url.to_string()))
            .collect::<Vec<_>>();
        let installed = repo_scanner::scan_install::install_from_repo_at_with_source_migrations(
            repo_dir,
            repo_url,
            Some(expected_commit),
            targets,
            &authorized,
        )?;
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

fn load_skill_lock_entry(skill_name: &str) -> anyhow::Result<Option<crate::lockfile::LockEntry>> {
    let _lock = crate::lockfile::get_mutex()
        .lock()
        .map_err(|_| anyhow::anyhow!("Lockfile mutex poisoned"))?;
    let lockfile = crate::lockfile::Lockfile::load(&crate::lockfile::lockfile_path())?;
    Ok(lockfile
        .skills
        .into_iter()
        .find(|entry| skill_lock_names_equal(&entry.name, skill_name)))
}

fn restore_skill_lock_entry(
    skill_name: &str,
    previous: Option<crate::lockfile::LockEntry>,
) -> anyhow::Result<()> {
    let _lock = crate::lockfile::get_mutex()
        .lock()
        .map_err(|_| anyhow::anyhow!("Lockfile mutex poisoned"))?;
    let path = crate::lockfile::lockfile_path();
    let mut lockfile = crate::lockfile::Lockfile::load(&path)?;
    lockfile.remove(skill_name);
    if let Some(entry) = previous {
        lockfile.upsert(entry);
    }
    lockfile.save(&path)
}

fn skill_lock_names_equal(left: &str, right: &str) -> bool {
    if cfg!(windows) {
        left.eq_ignore_ascii_case(right)
    } else {
        left == right
    }
}

fn ensure_generic_input_repository_mutable(input: &str) -> anyhow::Result<()> {
    let source = crate::source_resolver::Source::parse(input)?;
    crate::skill_mutation::policy().ensure_repository_mutation_allowed(&source.repo_url)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::git::transport::{GitAuthMaterial, NoopGitProgressSink};

    #[test]
    fn failed_local_graduation_restores_content_and_previous_lock_entry() {
        let _guard = crate::lock_test_env();
        let temp = tempfile::tempdir().unwrap();
        let previous_data = std::env::var_os("SKILLSTAR_DATA_DIR");
        let previous_hub = std::env::var_os("SKILLSTAR_HUB_DIR");
        unsafe {
            std::env::set_var("SKILLSTAR_DATA_DIR", temp.path().join("data"));
            std::env::set_var("SKILLSTAR_HUB_DIR", temp.path().join("hub"));
        }

        let result = (|| -> anyhow::Result<()> {
            local_skill::create("writer", Some("# Writer\n"))?;
            let old_entry = crate::lockfile::LockEntry {
                name: "writer".into(),
                git_url: "legacy-local".into(),
                git_ref: None,
                tree_hash: "legacy-tree".into(),
                content_hash: None,
                content_hash_version: None,
                installed_at: "2026-08-05T00:00:00Z".into(),
                source_folder: None,
            };
            let lock_path = crate::lockfile::lockfile_path();
            let mut lockfile = crate::lockfile::Lockfile::default();
            lockfile.upsert(old_entry);
            lockfile.save(&lock_path)?;

            let source = "missing-graduation-cache";
            let repo = skillstar_core::infra::paths::repos_cache_dir()
                .join(crate::repo_scanner::cache_dir_name(source));
            std::fs::create_dir_all(&repo)?;
            let status = std::process::Command::new("git")
                .args(["init", "-q"])
                .current_dir(&repo)
                .status()?;
            assert!(status.success());
            let facade = GitSkillFacade::new(GitOperationSession::new(
                "failed-graduation",
                GitAuthMaterial::missing(),
                Arc::new(NoopGitProgressSink),
            ));

            let error = facade
                .graduate_local_skill_from_scan(
                    "writer",
                    source,
                    "https://github.com/acme/channel.git",
                    &SkillInstallTarget {
                        id: "writer".into(),
                        folder_path: "skills/writer".into(),
                    },
                )
                .expect_err("the cache has no remote and graduation must fail");

            assert!(error.to_string().contains("fetch"));
            assert!(local_skill::is_local_skill("writer"));
            assert_eq!(
                std::fs::read_to_string(
                    skillstar_core::infra::paths::hub_skills_dir().join("writer/SKILL.md")
                )?,
                "# Writer\n"
            );
            let restored = crate::lockfile::Lockfile::load(&lock_path)?;
            let restored = restored
                .skills
                .iter()
                .find(|entry| entry.name == "writer")
                .expect("the previous lock entry must be restored");
            assert_eq!(restored.git_url, "legacy-local");
            assert_eq!(restored.tree_hash, "legacy-tree");
            assert!(restored.source_folder.is_none());
            Ok(())
        })();

        unsafe {
            match previous_data {
                Some(value) => std::env::set_var("SKILLSTAR_DATA_DIR", value),
                None => std::env::remove_var("SKILLSTAR_DATA_DIR"),
            }
            match previous_hub {
                Some(value) => std::env::set_var("SKILLSTAR_HUB_DIR", value),
                None => std::env::remove_var("SKILLSTAR_HUB_DIR"),
            }
        }
        result.unwrap();
    }
}
