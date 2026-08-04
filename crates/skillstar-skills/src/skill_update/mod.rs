mod plan;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use skillstar_core::types::{
    Skill, SkillCategory, SkillType, extract_github_source_from_url, extract_skill_description,
};
use std::collections::VecDeque;
use std::path::Path;
use std::sync::Mutex;

use crate::git::ops as git_ops;
use crate::lockfile::LockEntry;
use crate::{
    content, deployment, installed_skill, local_skill, lockfile, projects, repo_link, repo_scanner,
};
use plan::{SiblingState, UpdatePlan};

/// Result of a hub skill update, including any project-level cascade work.
#[derive(Debug, Clone)]
struct UpdateOutcome {
    tree_hash: String,
    git_url: String,
    sibling_names: Vec<String>,
    agent_links: Vec<String>,
    /// Per-agent re-link failures ("Agent: error"). The update itself
    /// succeeded; these tell the UI which agent deployments need attention.
    agent_link_failures: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct UpdateResult {
    pub skill: Skill,
    pub siblings_cleared: Vec<String>,
    pub agent_link_failures: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SkillUpdateFailure {
    pub name: String,
    pub error: String,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum LocalDivergenceReason {
    ContentChanged,
    BaselineMissing,
    SnapshotFailed,
}

#[derive(Debug, Clone, Serialize)]
pub struct SkillUpdateBlocked {
    pub name: String,
    pub reason: LocalDivergenceReason,
    pub suggested_local_name: String,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum LocalDivergenceResolution {
    Preserve { local_name: String },
    Discard,
}

#[derive(Debug, Clone, Serialize)]
pub struct ResolveSkillUpdateResult {
    pub update: UpdateResult,
    pub local_copy: Option<Skill>,
}

/// Outcome of a batch update.
///
/// `skipped` names were not updated because a skill sharing their repository
/// was — their content moved anyway. A failed update reports every name it
/// would have covered, so nothing is quietly counted as done.
#[derive(Debug, Clone, Default, Serialize)]
pub struct SkillUpdateReport {
    pub updated: Vec<UpdateResult>,
    pub blocked: Vec<SkillUpdateBlocked>,
    pub failed: Vec<SkillUpdateFailure>,
    pub skipped: Vec<String>,
}

/// Update several skills, pulling each repository at most once.
///
/// This is where "skills sharing a repository need one update between them"
/// lives. It used to live in the UI, alongside a second copy of the sibling
/// fan-out it was mirroring.
pub fn update_skills(names: &[String]) -> SkillUpdateReport {
    let entries = lockfile::Lockfile::load(&lockfile::lockfile_path())
        .map(|lockfile| lockfile.skills)
        .unwrap_or_default();
    let groups = plan::group_by_repo(&entries, names);

    let mut report = SkillUpdateReport::default();
    let mut runnable = Vec::new();
    for group in groups {
        let blocked = local_divergences_for_group(&entries, &group);
        if blocked.is_empty() {
            runnable.push(group);
        } else {
            report.blocked.extend(blocked);
        }
    }

    for (group, outcome) in run_groups(runnable) {
        match outcome {
            Ok(result) => {
                report.updated.push(result);
                report.skipped.extend(group.covered);
            }
            Err(err) => {
                let error = format!("{err:#}");
                report.failed.push(SkillUpdateFailure {
                    name: group.representative,
                    error: error.clone(),
                });
                // The rest of the repository did not move either.
                for name in group.covered {
                    report.failed.push(SkillUpdateFailure {
                        name,
                        error: error.clone(),
                    });
                }
            }
        }
    }
    report
}

fn local_divergences_for_group(
    entries: &[LockEntry],
    group: &plan::RepoGroup,
) -> Vec<SkillUpdateBlocked> {
    let representative = entries
        .iter()
        .find(|entry| entry.name == group.representative);

    let candidates: Vec<&LockEntry> = match representative {
        Some(entry) if !entry.git_url.trim().is_empty() => entries
            .iter()
            .filter(|candidate| candidate.git_url == entry.git_url)
            .collect(),
        Some(entry) => vec![entry],
        None => return Vec::new(),
    };

    candidates
        .into_iter()
        .filter(|entry| {
            let path = skillstar_core::infra::paths::hub_skills_dir().join(&entry.name);
            path.symlink_metadata().is_ok()
        })
        .filter_map(|entry| match content::snapshot(&entry.name) {
            Ok(snapshot) if entry.content_hash.as_deref() == Some(&snapshot.content_hash) => None,
            Ok(_) => Some(SkillUpdateBlocked {
                name: entry.name.clone(),
                reason: if entry.content_hash.is_some() {
                    LocalDivergenceReason::ContentChanged
                } else {
                    LocalDivergenceReason::BaselineMissing
                },
                suggested_local_name: suggested_local_name(&entry.name),
                error: None,
            }),
            Err(error) => Some(SkillUpdateBlocked {
                name: entry.name.clone(),
                reason: LocalDivergenceReason::SnapshotFailed,
                suggested_local_name: suggested_local_name(&entry.name),
                error: Some(error.to_string()),
            }),
        })
        .collect()
}

fn suggested_local_name(name: &str) -> String {
    let hub = skillstar_core::infra::paths::hub_skills_dir();
    let local = skillstar_core::infra::paths::local_skills_dir();
    let base = format!("{name}.local");
    for suffix in 1u32.. {
        let candidate = if suffix == 1 {
            base.clone()
        } else {
            format!("{base}.{suffix}")
        };
        if hub.join(&candidate).symlink_metadata().is_err()
            && local.join(&candidate).symlink_metadata().is_err()
        {
            return candidate;
        }
    }
    unreachable!("the local-copy name space cannot be exhausted")
}

/// Run one update per repository, a few at a time, and restore request order.
///
/// Separate repositories are separate checkouts, so they pull independently;
/// the bound keeps a large "update all" from opening a fetch per repository at
/// once.
fn run_groups(groups: Vec<plan::RepoGroup>) -> Vec<(plan::RepoGroup, Result<UpdateResult>)> {
    if groups.len() <= 1 {
        return groups
            .into_iter()
            .map(|group| {
                let outcome = update_skill(&group.representative);
                (group, outcome)
            })
            .collect();
    }

    let queue: Mutex<VecDeque<(usize, plan::RepoGroup)>> =
        Mutex::new(groups.into_iter().enumerate().collect());
    let done: Mutex<Vec<(usize, plan::RepoGroup, Result<UpdateResult>)>> = Mutex::new(Vec::new());
    let workers = update_concurrency_limit().min(lock(&queue).len());

    std::thread::scope(|scope| {
        for _ in 0..workers {
            scope.spawn(|| {
                loop {
                    let Some((index, group)) = lock(&queue).pop_front() else {
                        return;
                    };
                    let outcome = update_skill(&group.representative);
                    lock(&done).push((index, group, outcome));
                }
            });
        }
    });

    let mut results = done.into_inner().unwrap_or_else(|err| err.into_inner());
    results.sort_by_key(|(index, _, _)| *index);
    results
        .into_iter()
        .map(|(_, group, outcome)| (group, outcome))
        .collect()
}

/// A panicking worker leaves the queue intact — its entries are plain data
/// with no invariant to corrupt — so recover rather than poisoning the batch.
fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(|err| err.into_inner())
}

fn update_concurrency_limit() -> usize {
    std::thread::available_parallelism()
        .map(|parallelism| parallelism.get().clamp(2, 4))
        .unwrap_or(3)
}

fn compute_hash_for_skill_entry(skill_path: &Path, source_folder: Option<&str>) -> Option<String> {
    if let Some(folder) = source_folder.filter(|folder| !folder.is_empty()) {
        let repo_root = repo_link::repo_root_of(skill_path)?;
        return git_ops::compute_subtree_hash(&repo_root, folder).ok();
    }

    if let Some(repo_root) = repo_link::repo_root_of(skill_path) {
        return git_ops::compute_tree_hash(&repo_root)
            .ok()
            .or_else(|| git_ops::compute_tree_hash(skill_path).ok());
    }

    git_ops::compute_tree_hash(skill_path).ok()
}

pub fn update_skill(name: &str) -> Result<UpdateResult> {
    let entries = lockfile::Lockfile::load(&lockfile::lockfile_path())
        .map(|lockfile| lockfile.skills)
        .unwrap_or_default();
    let group = plan::RepoGroup {
        representative: name.to_string(),
        covered: Vec::new(),
    };
    if let Some(blocked) = local_divergences_for_group(&entries, &group)
        .into_iter()
        .next()
    {
        anyhow::bail!(
            "Skill '{}' has local divergence; preserve it as '{}' or explicitly discard it before updating",
            blocked.name,
            blocked.suggested_local_name
        );
    }
    update_skill_unchecked(name)
}

/// Resolve a previously reported local divergence and continue through the
/// same update transaction. Preservation finishes before the source tree is
/// allowed to move.
pub fn resolve_skill_update(
    name: &str,
    resolution: LocalDivergenceResolution,
) -> Result<ResolveSkillUpdateResult> {
    let local_copy = match resolution {
        LocalDivergenceResolution::Preserve { local_name } => {
            let snapshot = content::snapshot(name)
                .with_context(|| format!("failed to capture local divergence for '{name}'"))?;
            let local_copy = local_skill::create_from_snapshot(&local_name, &snapshot)?;
            installed_skill::invalidate_cache();
            Some(local_copy)
        }
        LocalDivergenceResolution::Discard => None,
    };
    Ok(ResolveSkillUpdateResult {
        update: update_skill_unchecked(name)?,
        local_copy,
    })
}

fn update_skill_unchecked(name: &str) -> Result<UpdateResult> {
    let outcome = apply_update(name)?;
    let path = skillstar_core::infra::paths::hub_skills_dir().join(name);
    let description = content::resolve_content_dir(name)
        .map(|dir| extract_skill_description(&dir))
        .unwrap_or_else(|| extract_skill_description(&path));
    let source = extract_github_source_from_url(&outcome.git_url);
    let skill_type = if local_skill::is_local_skill(name) {
        SkillType::Local
    } else {
        SkillType::Hub
    };

    Ok(UpdateResult {
        skill: Skill {
            name: name.to_string(),
            description,
            localized_description: None,
            skill_type,
            stars: 0,
            installed: true,
            update_available: false,
            last_updated: chrono::Utc::now().to_rfc3339(),
            git_url: outcome.git_url,
            tree_hash: Some(outcome.tree_hash),
            category: SkillCategory::None,
            author: None,
            topics: Vec::new(),
            agent_links: Some(outcome.agent_links),
            rank: None,
            source,
        },
        siblings_cleared: outcome.sibling_names,
        agent_link_failures: outcome.agent_link_failures,
    })
}

/// Look up what a sibling of the updated skill looks like on disk.
///
/// This is the one thing [`plan::plan_update`] cannot decide on its own, so it
/// is injected — which is also what lets the planning tests run without a hub
/// directory or a git remote.
fn sibling_state_on_disk(skills_dir: &Path, entry: &LockEntry) -> SiblingState {
    let sibling_path = skills_dir.join(&entry.name);
    if !sibling_path.exists() {
        return SiblingState::Absent;
    }
    SiblingState::Present(compute_hash_for_skill_entry(
        &sibling_path,
        entry.source_folder.as_deref(),
    ))
}

fn apply_hash_writes(lockfile: &mut lockfile::Lockfile, writes: &[(String, String)]) {
    for (name, hash) in writes {
        if let Some(entry) = lockfile.skills.iter_mut().find(|entry| entry.name == *name) {
            entry.tree_hash = hash.clone();
        }
    }
}

fn refresh_content_baselines(lockfile: &mut lockfile::Lockfile, names: &[String]) -> Result<()> {
    for name in names {
        let snapshot = content::snapshot(name)
            .with_context(|| format!("failed to capture updated content baseline for '{name}'"))?;
        if let Some(entry) = lockfile.skills.iter_mut().find(|entry| entry.name == *name) {
            entry.content_hash = Some(snapshot.content_hash);
        }
    }
    Ok(())
}

fn apply_update(name: &str) -> Result<UpdateOutcome> {
    let skills_dir = skillstar_core::infra::paths::hub_skills_dir();
    let path = skills_dir.join(name);

    if !path.exists() && !skillstar_core::infra::fs_ops::is_link(&path) {
        anyhow::bail!("Skill '{}' not found in hub", name);
    }

    let is_repo_skill = repo_link::is_repo_cached(&path);
    let lock_path = lockfile::lockfile_path();

    // Read unlocked to learn which subfolder to pull. The planning read below
    // happens again under the mutex: holding it across a network fetch would
    // serialise every concurrent update, and another update may land meanwhile.
    let source_folder = lockfile::Lockfile::load(&lock_path)
        .ok()
        .and_then(|lf| lf.skills.into_iter().find(|entry| entry.name == name))
        .and_then(|entry| entry.source_folder);

    let tree_hash = if is_repo_skill {
        repo_scanner::pull_repo_skill_update(&path, source_folder.as_deref())
            .context("failed to pull repo-cached skill update")?
    } else {
        git_ops::pull_repo(&path).context("failed to pull hub skill update")?;
        git_ops::compute_tree_hash(&path).context("failed to compute updated tree hash")?
    };

    let plan: UpdatePlan = {
        let _lock = lockfile::get_mutex()
            .lock()
            .map_err(|_| anyhow::anyhow!("Lockfile mutex poisoned"))?;
        let mut lockfile = lockfile::Lockfile::load(&lock_path)
            .with_context(|| format!("Failed to load lockfile '{}'", lock_path.display()))?;

        let plan = plan::plan_update(&lockfile.skills, name, &tree_hash, is_repo_skill, |entry| {
            sibling_state_on_disk(&skills_dir, entry)
        });

        apply_hash_writes(&mut lockfile, &plan.hash_writes);
        refresh_content_baselines(&mut lockfile, &plan.affected)?;
        lockfile
            .save(&lock_path)
            .with_context(|| format!("Failed to save lockfile '{}'", lock_path.display()))?;
        plan
    };

    installed_skill::invalidate_cache();

    for skill_name in &plan.affected {
        installed_skill::clear_update_state(skill_name);
    }

    let mut agent_links = Vec::new();
    let mut agent_link_failures = Vec::new();
    for skill_name in &plan.affected {
        match deployment::resync_existing_links(skill_name) {
            Ok(report) => {
                for failure in report.failures {
                    agent_link_failures.push(if skill_name == name {
                        failure
                    } else {
                        format!("{skill_name} → {failure}")
                    });
                }
                if skill_name == name {
                    agent_links = report.linked_to;
                }
            }
            Err(err) => {
                // The update itself succeeded — report the re-link problem
                // instead of failing the whole update or silently dropping it.
                agent_link_failures.push(format!("{skill_name}: {err:#}"));
            }
        }
    }

    projects::cascade_skill_update_to_projects(&plan.affected);

    Ok(UpdateOutcome {
        tree_hash,
        git_url: plan.git_url,
        sibling_names: plan.siblings_cleared,
        agent_links,
        agent_link_failures,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command;

    struct TestHub {
        previous_hub: Option<std::ffi::OsString>,
        _temp: tempfile::TempDir,
    }

    impl TestHub {
        fn new() -> Self {
            let previous_hub = std::env::var_os("SKILLSTAR_HUB_DIR");
            let temp = tempfile::tempdir().unwrap();
            unsafe {
                std::env::set_var("SKILLSTAR_HUB_DIR", temp.path());
            }
            Self {
                previous_hub,
                _temp: temp,
            }
        }
    }

    impl Drop for TestHub {
        fn drop(&mut self) {
            unsafe {
                match self.previous_hub.take() {
                    Some(value) => std::env::set_var("SKILLSTAR_HUB_DIR", value),
                    None => std::env::remove_var("SKILLSTAR_HUB_DIR"),
                }
            }
        }
    }

    fn run_git(repo: &Path, args: &[&str]) {
        let output = Command::new("git")
            .current_dir(repo)
            .args(args)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "git {args:?} failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    fn committed_remote() -> tempfile::TempDir {
        let remote = tempfile::tempdir().unwrap();
        run_git(remote.path(), &["init", "--initial-branch=main"]);
        run_git(remote.path(), &["config", "user.email", "test@example.com"]);
        run_git(remote.path(), &["config", "user.name", "SkillStar Tests"]);
        std::fs::create_dir_all(remote.path().join("scripts")).unwrap();
        std::fs::write(
            remote.path().join("SKILL.md"),
            "---\ndescription: v1\n---\n",
        )
        .unwrap();
        std::fs::write(remote.path().join("scripts/run.sh"), "echo remote-v1\n").unwrap();
        run_git(remote.path(), &["add", "."]);
        run_git(remote.path(), &["commit", "-m", "v1"]);
        remote
    }

    fn committed_multi_skill_remote() -> tempfile::TempDir {
        let remote = tempfile::tempdir().unwrap();
        run_git(remote.path(), &["init", "--initial-branch=main"]);
        run_git(remote.path(), &["config", "user.email", "test@example.com"]);
        run_git(remote.path(), &["config", "user.name", "SkillStar Tests"]);
        for name in ["alpha", "beta"] {
            let directory = remote.path().join("skills").join(name);
            std::fs::create_dir_all(&directory).unwrap();
            std::fs::write(
                directory.join("SKILL.md"),
                format!("---\nname: {name}\ndescription: {name} v1\n---\n"),
            )
            .unwrap();
        }
        run_git(remote.path(), &["add", "."]);
        run_git(remote.path(), &["commit", "-m", "v1"]);
        remote
    }

    #[test]
    fn local_divergence_blocks_update_before_any_file_changes() {
        let _guard = crate::lock_test_env();
        let _hub = TestHub::new();
        let remote = committed_remote();

        crate::skill_install::install_skill(
            remote.path().to_string_lossy().into_owned(),
            Some("demo".to_string()),
        )
        .unwrap();

        let installed = skillstar_core::infra::paths::hub_skills_dir().join("demo");
        std::fs::write(installed.join("scripts/run.sh"), "echo local-change\n").unwrap();

        std::fs::write(remote.path().join("scripts/run.sh"), "echo remote-v2\n").unwrap();
        run_git(remote.path(), &["add", "."]);
        run_git(remote.path(), &["commit", "-m", "v2"]);

        let report = update_skills(&["demo".to_string()]);

        assert!(report.updated.is_empty());
        assert!(report.failed.is_empty());
        assert_eq!(report.blocked.len(), 1);
        assert_eq!(report.blocked[0].name, "demo");
        assert_eq!(report.blocked[0].suggested_local_name, "demo.local");
        assert_eq!(
            std::fs::read_to_string(installed.join("scripts/run.sh")).unwrap(),
            "echo local-change\n",
            "detecting divergence must not fetch/reset or rewrite the Skill"
        );
    }

    #[test]
    fn skillstar_state_and_temporary_files_do_not_block_a_clean_update() {
        let _guard = crate::lock_test_env();
        let _hub = TestHub::new();
        let remote = committed_remote();

        crate::skill_install::install_skill(
            remote.path().to_string_lossy().into_owned(),
            Some("demo".to_string()),
        )
        .unwrap();

        let installed = skillstar_core::infra::paths::hub_skills_dir().join("demo");
        std::fs::create_dir_all(installed.join(".skillstar")).unwrap();
        std::fs::write(installed.join(".skillstar/update.json"), "transient").unwrap();
        std::fs::write(installed.join(".DS_Store"), "finder").unwrap();
        std::fs::write(installed.join("notes.md~"), "editor backup").unwrap();

        std::fs::write(remote.path().join("scripts/run.sh"), "echo remote-v2\n").unwrap();
        run_git(remote.path(), &["add", "."]);
        run_git(remote.path(), &["commit", "-m", "v2"]);

        let report = update_skills(&["demo".to_string()]);

        assert!(report.blocked.is_empty());
        assert!(report.failed.is_empty());
        assert_eq!(report.updated.len(), 1);
        assert_eq!(
            std::fs::read_to_string(installed.join("scripts/run.sh")).unwrap(),
            "echo remote-v2\n"
        );
    }

    #[test]
    fn preserving_divergence_copies_the_full_tree_then_updates_the_subscription() {
        let _guard = crate::lock_test_env();
        let _hub = TestHub::new();
        let remote = committed_remote();

        crate::skill_install::install_skill(
            remote.path().to_string_lossy().into_owned(),
            Some("demo".to_string()),
        )
        .unwrap();

        let installed = skillstar_core::infra::paths::hub_skills_dir().join("demo");
        std::fs::write(installed.join("SKILL.md"), "---\ndescription: local\n---\n").unwrap();
        std::fs::write(installed.join("scripts/run.sh"), "echo local-change\n").unwrap();
        std::fs::create_dir_all(installed.join("assets")).unwrap();
        std::fs::write(installed.join("assets/prompt.txt"), "local asset\n").unwrap();
        #[cfg(unix)]
        std::os::unix::fs::symlink("assets/prompt.txt", installed.join("prompt-link")).unwrap();

        std::fs::write(remote.path().join("scripts/run.sh"), "echo remote-v2\n").unwrap();
        run_git(remote.path(), &["add", "."]);
        run_git(remote.path(), &["commit", "-m", "v2"]);

        let result = resolve_skill_update(
            "demo",
            LocalDivergenceResolution::Preserve {
                local_name: "custom-demo".to_string(),
            },
        )
        .unwrap();

        assert_eq!(result.update.skill.name, "demo");
        assert_eq!(
            result.local_copy.as_ref().map(|skill| skill.name.as_str()),
            Some("custom-demo")
        );
        assert_eq!(
            std::fs::read_to_string(installed.join("scripts/run.sh")).unwrap(),
            "echo remote-v2\n"
        );

        let local = skillstar_core::infra::paths::hub_skills_dir().join("custom-demo");
        assert!(crate::local_skill::is_local_skill("custom-demo"));
        assert_eq!(
            std::fs::read_to_string(local.join("scripts/run.sh")).unwrap(),
            "echo local-change\n"
        );
        assert_eq!(
            std::fs::read_to_string(local.join("assets/prompt.txt")).unwrap(),
            "local asset\n"
        );
        #[cfg(unix)]
        assert_eq!(
            std::fs::read_link(local.join("prompt-link")).unwrap(),
            Path::new("assets/prompt.txt")
        );
    }

    #[test]
    fn discarding_divergence_updates_without_creating_a_local_copy() {
        let _guard = crate::lock_test_env();
        let _hub = TestHub::new();
        let remote = committed_remote();

        crate::skill_install::install_skill(
            remote.path().to_string_lossy().into_owned(),
            Some("demo".to_string()),
        )
        .unwrap();
        let installed = skillstar_core::infra::paths::hub_skills_dir().join("demo");
        std::fs::write(installed.join("scripts/run.sh"), "echo throw-away\n").unwrap();
        std::fs::write(remote.path().join("scripts/run.sh"), "echo remote-v2\n").unwrap();
        run_git(remote.path(), &["add", "."]);
        run_git(remote.path(), &["commit", "-m", "v2"]);

        resolve_skill_update("demo", LocalDivergenceResolution::Discard).unwrap();

        assert_eq!(
            std::fs::read_to_string(installed.join("scripts/run.sh")).unwrap(),
            "echo remote-v2\n"
        );
        assert!(!skillstar_core::infra::paths::local_skills_dir().exists());
    }

    #[test]
    fn blocked_update_suggests_a_non_destructive_local_copy_name() {
        let _guard = crate::lock_test_env();
        let _hub = TestHub::new();
        let remote = committed_remote();

        crate::skill_install::install_skill(
            remote.path().to_string_lossy().into_owned(),
            Some("demo".to_string()),
        )
        .unwrap();
        crate::local_skill::create("demo.local", None).unwrap();
        let installed = skillstar_core::infra::paths::hub_skills_dir().join("demo");
        std::fs::write(installed.join("scripts/run.sh"), "echo local-change\n").unwrap();

        let report = update_skills(&["demo".to_string()]);

        assert_eq!(report.blocked[0].suggested_local_name, "demo.local.2");
        assert!(crate::local_skill::is_local_skill("demo.local"));
    }

    #[test]
    fn editable_local_copy_name_cannot_escape_the_managed_local_root() {
        let _guard = crate::lock_test_env();
        let _hub = TestHub::new();
        let remote = committed_remote();

        crate::skill_install::install_skill(
            remote.path().to_string_lossy().into_owned(),
            Some("demo".to_string()),
        )
        .unwrap();
        let installed = skillstar_core::infra::paths::hub_skills_dir().join("demo");
        std::fs::write(installed.join("scripts/run.sh"), "echo local-change\n").unwrap();

        let error = resolve_skill_update(
            "demo",
            LocalDivergenceResolution::Preserve {
                local_name: "../escape".to_string(),
            },
        )
        .unwrap_err();

        assert!(error.to_string().contains("Invalid local Skill name"));
        assert_eq!(
            std::fs::read_to_string(installed.join("scripts/run.sh")).unwrap(),
            "echo local-change\n"
        );
    }

    #[test]
    fn divergence_in_a_repo_sibling_blocks_the_shared_checkout_before_reset() {
        let _guard = crate::lock_test_env();
        let _hub = TestHub::new();
        let remote = committed_multi_skill_remote();
        let repos = skillstar_core::infra::paths::repos_cache_dir();
        std::fs::create_dir_all(&repos).unwrap();
        let cache = repos.join("multi");
        run_git(
            &repos,
            &[
                "clone",
                remote.path().to_str().unwrap(),
                cache.to_str().unwrap(),
            ],
        );
        let targets = ["alpha", "beta"].map(|name| crate::repo_scanner::SkillInstallTarget {
            id: name.to_string(),
            folder_path: format!("skills/{name}"),
        });
        crate::repo_scanner::install_from_repo_at(
            &cache,
            &remote.path().to_string_lossy(),
            None,
            &targets,
        )
        .unwrap();

        let skills = skillstar_core::infra::paths::hub_skills_dir();
        std::fs::write(
            skills.join("alpha/SKILL.md"),
            "---\nname: alpha\ndescription: locally edited\n---\n",
        )
        .unwrap();
        std::fs::write(
            remote.path().join("skills/beta/SKILL.md"),
            "---\nname: beta\ndescription: beta v2\n---\n",
        )
        .unwrap();
        run_git(remote.path(), &["add", "."]);
        run_git(remote.path(), &["commit", "-m", "v2"]);

        let report = update_skills(&["beta".to_string()]);

        assert!(report.updated.is_empty());
        assert_eq!(
            report
                .blocked
                .iter()
                .map(|blocked| blocked.name.as_str())
                .collect::<Vec<_>>(),
            vec!["alpha"]
        );
        assert!(
            std::fs::read_to_string(skills.join("beta/SKILL.md"))
                .unwrap()
                .contains("beta v1"),
            "a clean sibling cannot move when that would overwrite a divergent shared checkout"
        );
    }
}
