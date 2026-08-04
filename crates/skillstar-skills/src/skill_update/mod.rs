mod plan;

use anyhow::{Context, Result};
use serde::Serialize;
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

/// Outcome of a batch update.
///
/// `skipped` names were not updated because a skill sharing their repository
/// was — their content moved anyway. A failed update reports every name it
/// would have covered, so nothing is quietly counted as done.
#[derive(Debug, Clone, Default, Serialize)]
pub struct SkillUpdateReport {
    pub updated: Vec<UpdateResult>,
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
    for (group, outcome) in run_groups(groups) {
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
