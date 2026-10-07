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
use crate::skill_update::{
    SkillIdentityChange, SkillUpdateChannelManaged, SkillUpdateFailure, SkillUpdateReport,
    UpdateResult,
};
use crate::{Skill, local_skill, skill_install};
use anyhow::Context as _;
use ss_core::infra::error::AppError;
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
        Self::from_file_store_with_progress(Arc::new(NoopGitProgressSink))
    }

    /// Same credentials as from_file_store, with a caller-owned progress sink.
    /// The GPUI shell has no Tauri event bus, so the import dialog supplies
    /// one and paints the phase labels itself.
    pub fn from_file_store_with_progress(
        progress: Arc<dyn crate::git::transport::GitProgressSink>,
    ) -> Self {
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
            progress,
        ))
    }

    pub fn session(&self) -> &GitOperationSession {
        &self.session
    }

    pub fn cancel(&self) {
        self.session.cancel();
    }

    pub fn scan_repo(&self, input: &str, full_depth: bool) -> anyhow::Result<ScanResult> {
        self.scan_repo_with_refresh(input, full_depth, false)
    }

    pub fn scan_repo_with_refresh(
        &self,
        input: &str,
        full_depth: bool,
        refresh: bool,
    ) -> anyhow::Result<ScanResult> {
        ensure_generic_input_repository_mutable(input)?;
        let parsed =
            crate::source_resolver::Source::parse(input).context("Invalid repository URL")?;
        self.session.emit_stage(
            crate::git::transport::InstallStage::Fetching,
            &parsed.short,
            None,
        );
        let checkout = crate::fetch::fetch_for_scan_with_refresh(&parsed, refresh, &self.session)?;
        self.session.emit_stage(
            crate::git::transport::InstallStage::Discovering,
            &parsed.short,
            None,
        );
        let (_, _, repo_dir, skills) = crate::skill_install::scan_parsed_checkout(
            &parsed,
            checkout.dir().to_path_buf(),
            full_depth,
        );
        let plugin = crate::plugin_manifest::plugin_hint_for_repo(&repo_dir);
        Ok(ScanResult {
            spec: parsed,
            skills,
            plugin,
            revision: checkout.revision.clone(),
            cache_hit: checkout.cache_hit,
            cached_at: checkout.cached_at.clone(),
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

    /// Install the previewed commit, fetching only any missing skill payload.
    pub fn install_from_scan(
        &self,
        scan: &ScanResult,
        targets: &[SkillInstallTarget],
    ) -> anyhow::Result<Vec<String>> {
        let spec = &scan.spec;
        let revision = scan.revision.as_deref();
        self.session.emit_stage(
            crate::git::transport::InstallStage::Fetching,
            &spec.short,
            None,
        );
        let folders = targets
            .iter()
            .map(|target| target.folder_path.as_str())
            .collect::<Vec<_>>();
        let checkout =
            crate::fetch::fetch_for_install_at_revision(spec, &folders, revision, &self.session)?;
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
        // Read and prove the local Skill under the transaction, then drop it
        // before any remote git. A slow fetch must not pin every other install.
        let snapshot = {
            let _guard = crate::skill_update::acquire_update_transaction_lock()
                .context("Unable to lock Skill graduation")?;
            crate::skill_mutation::policy().ensure_skill_mutation_allowed(skill_name)?;
            crate::skill_mutation::policy().ensure_repository_mutation_allowed(&spec.repo_url)?;
            crate::content::snapshot(skill_name)?
        };
        let checkout =
            crate::fetch::fetch_for_install(spec, &[&target.folder_path], &self.session)?;
        let _guard = crate::skill_update::acquire_update_transaction_lock()
            .context("Unable to lock Skill graduation")?;
        crate::skill_mutation::policy().ensure_skill_mutation_allowed(skill_name)?;
        crate::skill_mutation::policy().ensure_repository_mutation_allowed(&spec.repo_url)?;
        let current = crate::content::snapshot(skill_name)?;
        if current.content_hash != snapshot.content_hash {
            anyhow::bail!(
                "Skill '{skill_name}' changed while its published source was being fetched; retry"
            );
        }
        local_skill::graduate(skill_name)?;
        let install = (|| -> anyhow::Result<Vec<String>> {
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
            let expected = crate::installer::canonical_skill_name(&target.id)?;
            installed
                .iter()
                .any(|installed| *installed == expected)
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

    /// Apply the staged repository installer to an already fetched, immutable
    /// checkout for a shared channel. Both sides of the transaction verify
    /// HEAD so a caller cannot record a requested commit that was not
    /// actually installed. Install-and-track, upgrade and rollback share it.
    pub(crate) fn install_verified_channel_checkout(
        &self,
        repo_dir: &Path,
        repo_url: &str,
        expected_commit: &str,
        targets: &[SkillInstallTarget],
        authority: &crate::channels::shared_channels::ChannelInstallAuthority,
    ) -> anyhow::Result<Vec<String>> {
        let before = crate::git::ops::rev_parse(repo_dir, "HEAD")?;
        if !before.eq_ignore_ascii_case(expected_commit) {
            anyhow::bail!(
                "repository checkout is at {before}, expected immutable commit {expected_commit}"
            );
        }
        let installed = install_targets_at(
            repo_dir,
            repo_url,
            Some(expected_commit),
            targets,
            authority,
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
                managed.name,
                managed.repository_id
            ));
        }
        if report.updated.is_empty() {
            return Err(anyhow::anyhow!("{}", first_failure_text(&report)));
        }
        Ok(report
            .updated
            .into_iter()
            .next()
            .expect("checked non-empty"))
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
        let mut refreshed = Vec::new();
        for applied in crate::update::apply_updates(&generic, &self.session) {
            let name = applied.name;
            match applied.result {
                crate::update::UpdateResult::Updated { .. } => {
                    // Refresh every existing link/copy onto the new content.
                    let agent_link_failures = match crate::deployment::resync_existing_links(&name)
                    {
                        Ok(resync) => resync.failures,
                        Err(error) => vec![format!("{error:#}")],
                    };
                    crate::update_state::set(&name, false);
                    refreshed.push(name.clone());
                    match crate::skill_install::load_skill_dto(&name) {
                        Ok(skill) => report.updated.push(UpdateResult {
                            skill,
                            siblings_cleared: Vec::new(),
                            agent_link_failures,
                        }),
                        Err(error) => report.failed.push(SkillUpdateFailure { name, error }),
                    }
                }
                crate::update::UpdateResult::Removed => {
                    crate::update_state::record(
                        &name,
                        false,
                        Some(crate::update_state::UpstreamChange::Removed {
                            suggested_local_name:
                                crate::skill_update::divergence::suggested_local_name(&name),
                            successor: None,
                        }),
                    );
                    report.skipped.push(name);
                }
                crate::update::UpdateResult::IdentityChanged { upstream_name } => {
                    crate::update_state::record(
                        &name,
                        true,
                        Some(crate::update_state::UpstreamChange::IdentityChanged {
                            upstream_name: upstream_name.clone(),
                        }),
                    );
                    report.identity_changed.push(SkillIdentityChange {
                        name,
                        upstream_name,
                    });
                }
                crate::update::UpdateResult::NotUpdatable => report.not_updatable.push(name),
                crate::update::UpdateResult::Failed(error) => {
                    report.failed.push(SkillUpdateFailure { name, error })
                }
            }
        }
        if !refreshed.is_empty() {
            report.project_failures =
                crate::projects::cascade_skill_update_to_projects(&refreshed).failures;
        }
        installed_skill::invalidate_cache();
        report
    }

    pub async fn refresh_skill_updates(&self) -> anyhow::Result<Vec<SkillUpdateState>> {
        installed_skill::refresh_skill_updates_in_session(&self.session).await
    }

    /// Background auto-update: check upstream, then overwrite-reinstall every
    /// Skill whose tree changed. See crate::update::auto_update_locked_skills.
    pub async fn auto_update_skills(&self) -> crate::update::AutoUpdateReport {
        crate::update::auto_update_locked_skills(&self.session).await
    }
}

/// Install `targets` out of an already-verified checkout dir (channels seam).
fn install_targets_at(
    repo_dir: &Path,
    repo_url: &str,
    git_ref: Option<&str>,
    targets: &[SkillInstallTarget],
    authority: &crate::channels::shared_channels::ChannelInstallAuthority,
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
    crate::installer::install_units_for_channel(repo_dir, &spec, &units, authority)
}

/// Readable outcome for an upstream frontmatter rename; nothing was changed.
pub fn identity_changed_message(name: &str, upstream_name: &str) -> String {
    format!(
        "Upstream renamed Skill '{name}' to '{upstream_name}'; '{name}' was left unchanged. Install '{upstream_name}' and uninstall '{name}' to follow the rename."
    )
}

fn first_failure_text(report: &SkillUpdateReport) -> String {
    report
        .failed
        .first()
        .map(|failure| format!("{}: {}", failure.name, failure.error))
        .or_else(|| {
            report
                .identity_changed
                .first()
                .map(|change| identity_changed_message(&change.name, &change.upstream_name))
        })
        .or_else(|| {
            report
                .skipped
                .first()
                .map(|name| format!("'{name}' is no longer available upstream"))
        })
        .or_else(|| {
            report.not_updatable.first().map(|name| {
                format!(
                    "'{name}' was installed from a local or unknown source and cannot be updated from upstream"
                )
            })
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
        let plugin = scan
            .plugin
            .as_ref()
            .expect("impeccable declares a plugin manifest");
        assert!(plugin.hooks);
        assert!(plugin.agents);

        let target = SkillInstallTarget {
            id: scan.skills[0].id.clone(),
            folder_path: scan.skills[0].folder_path.clone(),
            pinned: false,
        };
        assert_eq!(
            facade
                .install_from_scan(&scan, std::slice::from_ref(&target))
                .unwrap(),
            vec!["impeccable"]
        );
        let canonical = ss_core::infra::paths::agents_skill_dir("impeccable");
        assert!(canonical.join("scripts/impeccable").is_file());
        assert!(canonical.join("reference/craft.md").is_file());
        assert!(!canonical.join("README.md").exists());
        let lock = crate::skill_lock::load();
        assert_eq!(lock.skills["impeccable"].git_ref.as_deref(), Some("main"));
        assert_eq!(
            lock.skills["impeccable"].skill_folder_hash,
            crate::fetch::folder_tree_hash(fixture.dir.path(), Some(&target.folder_path))
        );

        facade
            .install_skills_batch(
                "https://github.com/pbakaus/impeccable/tree/main/.claude/skills/impeccable",
                &["impeccable".to_string()],
            )
            .unwrap();
        assert!(canonical.join("scripts/impeccable").is_file());
        assert!(canonical.join("reference/craft.md").is_file());
    }

    /// Remote git for graduation must run with the transaction released, the
    /// same way publish records depth around clone/pull/push.
    #[test]
    fn graduate_remote_git_runs_with_no_transaction_held() {
        let sandbox = crate::test_sandbox::Sandbox::new();
        let probe =
            crate::fetch::RemoteGitLockProbe::arm(sandbox.root().join("graduate-lock-probe"));
        let repo = sandbox.root().join("origin");
        let source = repo.join("skills/demo");
        std::fs::create_dir_all(&source).unwrap();
        std::fs::write(
            source.join("SKILL.md"),
            "---\nname: demo\ndescription: Published.\n---\n# git\n",
        )
        .unwrap();
        let output = |args: &[&str]| {
            let output = ss_core::infra::path_env::command_with_path("git")
                .args(args)
                .current_dir(&repo)
                .env("GIT_AUTHOR_NAME", "Test")
                .env("GIT_COMMITTER_NAME", "Test")
                .env("GIT_AUTHOR_EMAIL", "t@t")
                .env("GIT_COMMITTER_EMAIL", "t@t")
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "git {args:?} failed: {}",
                String::from_utf8_lossy(&output.stderr)
            );
            String::from_utf8(output.stdout).unwrap().trim().to_string()
        };
        output(&["init", "-q"]);
        output(&["add", "."]);
        output(&["commit", "-q", "-m", "publish", "--no-gpg-sign"]);
        let branch = output(&["branch", "--show-current"]);

        let hub = ss_core::infra::paths::hub_skills_dir().join("demo");
        std::fs::create_dir_all(&hub).unwrap();
        std::fs::write(
            hub.join("SKILL.md"),
            "---\nname: demo\ndescription: Local.\n---\n# local\n",
        )
        .unwrap();

        let spec = crate::source_resolver::Source {
            repo_url: crate::git::ops::local_file_url(&repo),
            short: "local/demo".into(),
            git_ref: Some(branch),
            subpath: None,
            skill_filter: None,
        };
        let target = SkillInstallTarget {
            id: "demo".into(),
            folder_path: "skills/demo".into(),
            pinned: false,
        };
        GitSkillFacade::new(GitOperationSession::public())
            .graduate_local_skill_from_scan("demo", &spec, &target)
            .unwrap();
        assert_eq!(
            probe.recorded_depth().as_deref(),
            Some("0"),
            "clone must run after the skill transaction is released"
        );
    }
}
