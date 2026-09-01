use crate::git::ops as git_ops;
use crate::lockfile::LockEntry;
pub use crate::update_state::SkillUpdateState;
use crate::{
    local_skill,
    lockfile::{self},
    repo_link, update_checker, update_state,
};
use anyhow::{Context, Result, anyhow};
use skillstar_agents::{self as agent_profile, AgentProfile};
use skillstar_core::types::{
    Skill, SkillCategory, extract_github_source_from_url, extract_skill_description,
};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::LazyLock;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, RwLock};
use tokio::sync::Semaphore;
use tokio::task::JoinSet;
use tracing::{debug, warn};

static SKILL_CACHE: LazyLock<RwLock<Option<Vec<Skill>>>> = LazyLock::new(|| RwLock::new(None));

pub fn invalidate_cache() {
    if let Ok(mut cache) = SKILL_CACHE.write() {
        *cache = None;
    }
}

/// Record that a skill was just updated, so no scan can re-assert its badge.
pub fn clear_update_state(name: &str) {
    update_state::set(name, false);
}

fn normalize_snapshot_component(raw: &str) -> Option<String> {
    let trimmed = raw.trim().to_ascii_lowercase();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed)
    }
}

fn build_snapshot_skill_key(source: &str, name: &str) -> Option<String> {
    Some(format!(
        "{}/{}",
        normalize_snapshot_component(source)?,
        normalize_snapshot_component(name)?
    ))
}

pub fn installed_snapshot_markers() -> HashSet<String> {
    let mut markers = HashSet::new();

    let hub_skills_dir = skillstar_core::infra::paths::hub_skills_dir();
    if let Ok(entries) = std::fs::read_dir(&hub_skills_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_dir() && !skillstar_core::infra::fs_ops::is_link(&path) {
                continue;
            }
            if let Some(name) = entry.file_name().to_str() {
                if name.starts_with('.') {
                    continue;
                }
                markers.insert(name.to_ascii_lowercase());
            }
        }
    }

    let lock_path = lockfile::lockfile_path();
    if let Ok(lockfile) = lockfile::Lockfile::load(&lock_path) {
        for entry in lockfile.skills {
            markers.insert(entry.name.to_ascii_lowercase());
            if let Some(source) = extract_github_source_from_url(&entry.git_url)
                && let Some(skill_key) = build_snapshot_skill_key(&source, &entry.name)
            {
                markers.insert(skill_key);
            }
        }
    }

    markers
}

fn apply_cached_update_states(mut skills: Vec<Skill>) -> Vec<Skill> {
    update_state::apply_to(&mut skills);
    let policy = crate::skill_mutation::policy();
    let mut lookup_failures = 0usize;
    let mut first_failure: Option<String> = None;
    for skill in &mut skills {
        match policy.managed_repository_for_skill(&skill.name) {
            // A shared channel owns it: its updates come from the channel flow,
            // never from the generic update badge.
            Ok(Some(_)) => {
                skill.update_available = false;
                skill.upstream_change = None;
            }
            Ok(None) => {}
            // Ownership is unknown — typically a corrupt or future-versioned
            // subscription registry. This used to be folded in with the line
            // above, so one unreadable file silently zeroed the update badge of
            // every Skill on the machine, channel-managed or not, with nothing
            // logged. The badge is display-only and every write path re-checks
            // the gate itself, so leaving it as computed cannot cause a wrong
            // mutation; losing every badge with no explanation can and did.
            Err(error) => {
                lookup_failures += 1;
                first_failure.get_or_insert_with(|| format!("{error:#}"));
            }
        }
    }
    if let Some(error) = first_failure {
        warn!(
            target: "skills",
            failures = lookup_failures,
            "Could not read shared-channel ownership while listing Skills; update badges are left as computed: {error}"
        );
    }
    skills
}

pub async fn list_installed_skills() -> Result<Vec<Skill>> {
    if let Ok(cache) = SKILL_CACHE.read()
        && let Some(skills) = &*cache
    {
        return Ok(apply_cached_update_states(skills.clone()));
    }

    // Ensure every skill in skills-local/ has a hub symlink before scanning
    local_skill::reconcile_hub_symlinks();

    let lock_map = load_lock_map();
    let profiles: Arc<[AgentProfile]> = Arc::from(agent_profile::list_profiles());
    let skill_dirs = collect_skill_dirs(&skillstar_core::infra::paths::hub_skills_dir(), None)?;

    if skill_dirs.is_empty() {
        return Ok(Vec::new());
    }

    let work: Vec<(PathBuf, Option<LockEntry>)> = skill_dirs
        .into_iter()
        .filter_map(|path| {
            let name = skill_name_from_path(&path)?;
            if name.starts_with('.') {
                return None;
            }
            let lock_entry = lock_map.get(&name).cloned();
            Some((path, lock_entry))
        })
        .collect();
    let built = tokio::task::spawn_blocking(move || {
        skillstar_core::infra::parallel::map_bounded(
            work,
            skillstar_core::infra::parallel::blocking_concurrency_limit(),
            |(path, lock_entry)| build_installed_skill(path, lock_entry, &profiles),
        )
    })
    .await
    .map_err(|err| anyhow!("installed-skill task failed: {}", err))?;

    let mut skills: Vec<Skill> = built.into_iter().collect::<Result<Vec<_>, _>>()?;

    skills.sort_by(|left, right| left.name.cmp(&right.name));

    let skills = apply_cached_update_states(skills);

    if let Ok(mut cache) = SKILL_CACHE.write() {
        *cache = Some(skills.clone());
    }

    Ok(skills)
}

pub async fn refresh_skill_updates_in_session(
    session: &crate::git::transport::GitOperationSession,
) -> Result<Vec<SkillUpdateState>> {
    // Taken before any checking starts: findings overtaken by an update that
    // lands while this scan runs are dropped when it commits.
    let scan_started = update_state::stamp();

    // Ownership lookups that fail skip their own Skill instead of aborting the
    // whole scan. The sibling folding bug in `apply_cached_update_states` had a
    // mirror image here: one unreadable subscription registry turned every
    // refresh into an error, so the user got neither update badges nor a reason.
    let policy = crate::skill_mutation::policy();
    let mut ownership_failures = 0usize;
    let mut first_ownership_failure: Option<String> = None;
    let skill_dirs = collect_skill_dirs(&skillstar_core::infra::paths::hub_skills_dir(), None)?
        .into_iter()
        .filter_map(|path| {
            let name = skill_name_from_path(&path)?;
            match policy.installed_skill_is_mutable(&name, &path) {
                Ok(true) => Some(path),
                Ok(false) => None,
                Err(error) => {
                    ownership_failures += 1;
                    first_ownership_failure.get_or_insert_with(|| format!("{error:#}"));
                    None
                }
            }
        })
        .collect::<Vec<_>>();
    if let Some(error) = first_ownership_failure {
        warn!(
            target: "skills",
            failures = ownership_failures,
            "Could not read shared-channel ownership while checking for updates; those Skills were skipped: {error}"
        );
    }

    if skill_dirs.is_empty() {
        return Ok(Vec::new());
    }

    // GitHub API fast path: one Trees API call per unique github.com repo
    // replaces the per-repo git fetch for update *detection*. Signed-in
    // sessions use the App token (authenticated budget); otherwise the call
    // is anonymous. Any failure (private repo, rate limit, network,
    // non-github source) falls back to the git fetch path below, so
    // correctness never depends on the API.
    let api_remote: Arc<
        std::collections::HashMap<std::path::PathBuf, crate::update_api::ApiRemoteTree>,
    > = Arc::new(api_prefetch_remote_trees(&skill_dirs).await);
    let api_ok_roots: std::collections::HashSet<std::path::PathBuf> =
        api_remote.keys().cloned().collect();

    // Pre-fetch unique repos (skipping GitHub API hits), then compare every
    // skill locally. One transaction lock covers the whole blocking section so
    // worker threads are not serialized on the non-reentrant mutex.
    let session = session.clone();
    let api_trees = Arc::clone(&api_remote);
    let skip = api_ok_roots;
    let states = tokio::task::spawn_blocking(move || {
        let Ok(_guard) = crate::skill_update::acquire_update_transaction_lock() else {
            return Ok(Vec::new());
        };
        let failed_fetch_roots =
            update_checker::prefetch_unique_repos_in_session_skipping(&skill_dirs, &session, &skip);
        let jobs = skill_dirs
            .into_iter()
            .filter_map(|path| {
                let name = skill_name_from_path(&path)?;
                if local_skill::is_local_skill(&name) {
                    return None;
                }
                Some((name, path))
            })
            .collect::<Vec<_>>();
        Ok::<_, anyhow::Error>(skillstar_core::infra::parallel::map_bounded(
            jobs,
            skillstar_core::infra::parallel::blocking_concurrency_limit(),
            |(name, path)| {
                if session.is_cancelled()
                    || !matches!(
                        crate::skill_mutation::policy().installed_skill_is_mutable(&name, &path),
                        Ok(true)
                    )
                {
                    return (name, None);
                }
                let update_available =
                    refresh_single_skill_update(&path, &failed_fetch_roots, &api_trees, &session);
                (name, update_available)
            },
        ))
    })
    .await
    .map_err(|err| anyhow!("skill-update task failed: {}", err))??;

    let mut states = states
        .into_iter()
        .filter_map(|(name, status)| {
            // None means "fetch failed, status unknown" — skip so the previous
            // cached value is preserved and the UI doesn't falsely clear the
            // update badge.
            Some(status?.into_state(name))
        })
        .collect::<Vec<_>>();

    states.sort_by(|left, right| left.name.cmp(&right.name));
    // Skills whose fetch failed were never pushed, so they keep their previous
    // state; the rest land unless an update overtook them mid-scan.
    Ok(update_state::commit_scan(scan_started, &states))
}

fn load_lock_map() -> HashMap<String, LockEntry> {
    let lock_path = lockfile::lockfile_path();
    let lockfile = lockfile::Lockfile::load(&lock_path).unwrap_or_default();
    lockfile
        .skills
        .into_iter()
        .map(|entry| (entry.name.clone(), entry))
        .collect()
}

fn collect_skill_dirs(skills_dir: &Path, names: Option<&HashSet<String>>) -> Result<Vec<PathBuf>> {
    let entries = match std::fs::read_dir(skills_dir) {
        Ok(entries) => entries,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(err) => {
            return Err(err).with_context(|| {
                format!(
                    "Failed to read installed skills directory {}",
                    skills_dir.display()
                )
            });
        }
    };

    let mut paths = Vec::new();
    for entry in entries {
        let entry = entry.with_context(|| {
            format!(
                "Failed to read installed-skill entry in {}",
                skills_dir.display()
            )
        })?;
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }

        let Some(name) = skill_name_from_path(&path) else {
            continue;
        };
        // Staging residue resolves through to a real directory, so `is_dir()`
        // alone would list it as an installed Skill.
        if !crate::hub_entry::is_managed_hub_entry(&name) {
            continue;
        }
        if names.is_some_and(|values| !values.contains(&name)) {
            continue;
        }

        paths.push(path);
    }

    paths.sort_by_key(|left| skill_name_from_path(left));
    Ok(paths)
}

fn build_installed_skill(
    path: PathBuf,
    lock_entry: Option<LockEntry>,
    profiles: &[AgentProfile],
) -> Result<Skill> {
    // For repo-cached skills (symlinks into .repos/), resolve the actual path
    let is_repo_skill = repo_link::is_repo_cached(&path);

    if !is_repo_skill {
        let _ = git_ops::ensure_worktree_checked_out(&path);
    }

    let name = skill_name_from_path(&path).unwrap_or_default();

    // For symlinked skills, read SKILL.md from the link target.
    let effective_path = if is_repo_skill {
        skillstar_core::infra::fs_ops::read_link_resolved(&path).unwrap_or_else(|_| path.clone())
    } else {
        path.clone()
    };

    let description = extract_skill_description(&effective_path);
    let localized_description = None;

    let tree_hash = git_ops::compute_tree_hash(&effective_path)
        .ok()
        .or_else(|| lock_entry.as_ref().map(|entry| entry.tree_hash.clone()));
    let agent_links = detect_agent_links(&name, profiles);

    // Derive source from git_url whenever possible (also works for root-level skills).
    let source = lock_entry
        .as_ref()
        .and_then(|entry| extract_github_source_from_url(&entry.git_url));

    // Determine skill type: "local" if symlink points into skills-local/
    let skill_type = if local_skill::is_local_skill(&name) {
        skillstar_core::types::SkillType::Local
    } else {
        skillstar_core::types::SkillType::Hub
    };

    Ok(Skill {
        name,
        description,
        localized_description,
        skill_type,
        stars: 0,
        installed: true,
        update_available: false,
        upstream_change: None,
        last_updated: lock_entry
            .as_ref()
            .map(|entry| entry.installed_at.clone())
            .unwrap_or_else(|| chrono::Utc::now().to_rfc3339()),
        git_url: lock_entry
            .as_ref()
            .map(|entry| entry.git_url.clone())
            .unwrap_or_default(),
        tree_hash,
        category: SkillCategory::None,
        author: None,
        topics: Vec::new(),
        agent_links: Some(agent_links),
        rank: None,
        source,
    })
}

fn refresh_single_skill_update(
    path: &Path,
    failed_fetch_roots: &std::collections::HashSet<std::path::PathBuf>,
    api_remote: &std::collections::HashMap<std::path::PathBuf, crate::update_api::ApiRemoteTree>,
    session: &crate::git::transport::GitOperationSession,
) -> Option<update_checker::UpstreamStatus> {
    // For repo-cached skills, the repo has already been fetched by
    // prefetch_unique_repos (or answered by the GitHub API fast path); only
    // compare local HEAD vs the remote. Returns None when the prefetch failed
    // for this skill's repo.
    if repo_link::is_repo_cached(path) {
        let api = repo_link::repo_root_of(path).and_then(|root| api_remote.get(&root));
        return update_checker::check_upstream_status(path, failed_fetch_roots, api, session);
    }
    let _ = git_ops::ensure_worktree_checked_out_in_session(path, session);
    // `Err` means the check itself failed (offline, broken .git, git missing) —
    // that is "unknown", not "up to date". Returning `Some(false)` here would
    // let one offline scan overwrite a real badge with `false` and persist it,
    // which is exactly what the repo-cached path above refuses to do.
    git_ops::check_update_in_session(path, session)
        .ok()
        .map(update_checker::UpstreamStatus::from_available)
}

/// One GitHub Trees API call per unique github.com repo, replacing the
/// per-repo `git fetch` for update *detection*. Repos that cannot use the
/// API (non-github host, no lock entry, no resolvable remote ref, or an
/// active rate-limit cooldown) are simply absent from the result and fall
/// back to the fetch path.
async fn api_prefetch_remote_trees(
    skill_dirs: &[PathBuf],
) -> std::collections::HashMap<std::path::PathBuf, crate::update_api::ApiRemoteTree> {
    use crate::update_api::{ApiRemoteTree, MAX_API_REPOS_PER_CYCLE};

    if crate::update_api::api_fast_path_blocked() {
        debug!(
            target: "update_checker",
            "GitHub API rate limit cooldown active — skipping fast path"
        );
        return std::collections::HashMap::new();
    }

    // A missing or unreadable lockfile simply disables the fast path.
    let Ok(lock) = crate::lockfile::Lockfile::load(&crate::lockfile::lockfile_path()) else {
        return std::collections::HashMap::new();
    };

    let mut candidates: Vec<(PathBuf, String, String, String)> = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for path in skill_dirs {
        let Some(root) = crate::repo_link::repo_root_of(path) else {
            continue;
        };
        if !seen.insert(root.clone()) {
            continue;
        }
        let Some(name) = skill_name_from_path(path) else {
            continue;
        };
        let Some(entry) = lock.skills.iter().find(|entry| entry.name == name) else {
            continue;
        };
        let Some((owner, repo)) = crate::update_api::owner_repo_from_git_url(&entry.git_url) else {
            continue;
        };
        let pinned = entry
            .git_ref
            .as_deref()
            .filter(|git_ref| !git_ref.is_empty());
        let Some(git_ref) = crate::update_api::remote_ref_for(&root, pinned) else {
            continue;
        };
        candidates.push((root, owner, repo, git_ref));
    }

    // Stay well under the unauthenticated API budget; the rest fall back to
    // the git fetch path.
    if candidates.len() > MAX_API_REPOS_PER_CYCLE {
        candidates.truncate(MAX_API_REPOS_PER_CYCLE);
    }
    if candidates.is_empty() {
        return std::collections::HashMap::new();
    }

    let token: Option<Arc<str>> = crate::update_api::optional_github_api_token().map(Arc::from);
    let abort = Arc::new(AtomicBool::new(false));
    let semaphore = Arc::new(Semaphore::new(crate::update_api::API_CONCURRENCY));
    let mut tasks = JoinSet::new();
    for (root, owner, repo, git_ref) in candidates {
        let permit = semaphore
            .clone()
            .acquire_owned()
            .await
            .expect("semaphore permits are never released elsewhere");
        let token = token.clone();
        let abort = Arc::clone(&abort);
        tasks.spawn(async move {
            let _permit = permit;
            if abort.load(Ordering::Relaxed) {
                return (root, None);
            }
            let result = crate::update_api::fetch_remote_subtree_hashes(
                &owner,
                &repo,
                &git_ref,
                token.as_deref(),
            )
            .await;
            if result.as_ref().is_err_and(|error| error.is_rate_limited()) {
                abort.store(true, Ordering::Relaxed);
            }
            (root, Some(result))
        });
    }

    let mut out: std::collections::HashMap<PathBuf, ApiRemoteTree> =
        std::collections::HashMap::new();
    let mut logged_rate_limit = false;
    while let Some(joined) = tasks.join_next().await {
        match joined {
            Ok((root, Some(Ok(tree)))) => {
                if api_tree_is_at_tracked_tip(&root, &tree) {
                    out.insert(root, tree);
                } else {
                    debug!(
                        target: "update_checker",
                        path = %root.display(),
                        "remote tip moved past the tracked ref — taking the git fetch path so it catches up"
                    );
                }
            }
            Ok((_, Some(Err(error)))) if error.is_rate_limited() => {
                if !logged_rate_limit {
                    logged_rate_limit = true;
                    debug!(
                        target: "update_checker",
                        error = %error,
                        "GitHub API rate limit reached — falling back to git fetch"
                    );
                }
            }
            Ok((_, Some(Err(error)))) if error.is_expected() => {
                debug!(
                    target: "update_checker",
                    error = %error,
                    "GitHub API update-check fast path unavailable — falling back to git fetch"
                );
            }
            Ok((_, Some(Err(error)))) => {
                warn!(
                    target: "update_checker",
                    error = %error,
                    "GitHub API update-check fast path failed — falling back to git fetch"
                );
            }
            Ok((_, None)) => {}
            Err(error) => {
                warn!(
                    target: "update_checker",
                    error = %error,
                    "GitHub API update-check task failed — falling back to git fetch"
                );
            }
        }
    }
    out
}

/// The API tree may stand in for a fetch only while the remote tip is the
/// commit the tracked ref already holds. Once upstream moves, the repository
/// takes the git fetch path instead, so `origin/HEAD` (or the pinned
/// `FETCH_HEAD`) and everything that reads it — new-skill detection above
/// all — catch up; comparing against the API alone would leave them stale
/// forever.
fn api_tree_is_at_tracked_tip(repo_root: &Path, tree: &crate::update_api::ApiRemoteTree) -> bool {
    let tracked = update_checker::tracked_update_ref(repo_root);
    let Ok(local_tip) = git_ops::rev_parse(repo_root, &format!("{tracked}^{{commit}}")) else {
        return false;
    };
    tree.subtree_hash(None) == Some(local_tip.as_str())
}

/// Agent links for one Skill, read from disk.
///
/// A freshly installed `Skill` carries no links yet, so callers that deploy and
/// then return the Skill must re-read them here instead of guessing.
pub fn agent_links_for(skill_name: &str) -> Vec<String> {
    detect_agent_links(skill_name, &agent_profile::list_profiles())
}

fn detect_agent_links(skill_name: &str, profiles: &[AgentProfile]) -> Vec<String> {
    let mut links = Vec::with_capacity(2); // most skills link to 1-2 agents
    for profile in profiles {
        if !profile.has_global_skills() {
            continue;
        }
        let link_path = profile.global_skills_dir.join(skill_name);
        // Check symlinks/junctions: is_link() AND exists() (follows target — broken = false)
        if skillstar_core::infra::fs_ops::is_link(&link_path) && link_path.exists() {
            links.push(profile.display_name.clone());
        } else if link_path.is_dir() && link_path.join("SKILL.md").exists() {
            // Also detect copy-based deployment (Windows fallback)
            links.push(profile.display_name.clone());
        }
    }
    links
}

fn skill_name_from_path(path: &Path) -> Option<String> {
    path.file_name()
        .map(|name| name.to_string_lossy().to_string())
        .filter(|name| !name.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;
    use skillstar_core::types::{Skill, SkillType};

    fn test_skill(name: &str) -> Skill {
        Skill {
            name: name.to_string(),
            description: String::new(),
            localized_description: None,
            skill_type: SkillType::Hub,
            stars: 0,
            installed: true,
            update_available: false,
            upstream_change: None,
            last_updated: String::new(),
            git_url: String::new(),
            tree_hash: None,
            category: SkillCategory::None,
            author: None,
            topics: Vec::new(),
            agent_links: Some(Vec::new()),
            rank: None,
            source: None,
        }
    }

    #[test]
    fn listed_skills_carry_the_recorded_update_state() {
        let _guard = crate::lock_test_env();
        let temp = tempfile::tempdir().unwrap();
        unsafe {
            std::env::set_var("SKILLSTAR_DATA_DIR", temp.path());
            std::env::remove_var("SKILLSTAR_HUB_DIR");
        }
        update_state::reset_for_test();

        update_state::commit_scan(
            update_state::stamp(),
            &[
                update_state::SkillUpdateState {
                    name: "recorded-update".to_string(),
                    update_available: true,
                    upstream_change: None,
                },
                update_state::SkillUpdateState {
                    name: "recorded-current".to_string(),
                    update_available: false,
                    upstream_change: None,
                },
            ],
        );

        let skills =
            apply_cached_update_states(vec![test_skill("recorded-update"), test_skill("unknown")]);

        assert!(skills[0].update_available);
        assert!(
            !skills[1].update_available,
            "unscanned skills stay as built"
        );
    }

    #[cfg(unix)]
    fn profile_at(id: &str, dir: &Path) -> AgentProfile {
        AgentProfile {
            id: id.to_string(),
            display_name: id.to_string(),
            icon: String::new(),
            global_skills_dir: dir.to_path_buf(),
            project_skills_rel: String::new(),
            installed: true,
            enabled: true,
            synced_count: 0,
        }
    }

    /// Regression: `install_skill` used to return only the just-deployed Agent,
    /// so the card carousel behaved like a radio group.
    #[cfg(unix)]
    #[test]
    fn every_deployed_agent_is_detected_not_just_one() {
        let temp = tempfile::tempdir().unwrap();
        let payload = temp.path().join("payload");
        std::fs::create_dir_all(&payload).unwrap();
        std::fs::write(payload.join("SKILL.md"), "# s").unwrap();

        let profiles: Vec<AgentProfile> = ["a", "b", "c"]
            .iter()
            .map(|id| {
                let dir = temp.path().join(id);
                std::fs::create_dir_all(&dir).unwrap();
                if *id != "c" {
                    std::os::unix::fs::symlink(&payload, dir.join("demo")).unwrap();
                }
                profile_at(id, &dir)
            })
            .collect();

        assert_eq!(detect_agent_links("demo", &profiles), vec!["a", "b"]);
    }
}

#[cfg(test)]
mod api_tip_tests {
    use super::api_tree_is_at_tracked_tip;
    use crate::update_api::ApiRemoteTree;
    use std::collections::HashMap;
    use std::path::Path;
    use std::process::Command;

    fn git(dir: &Path, args: &[&str]) -> String {
        let output = Command::new("git")
            .current_dir(dir)
            .args(args)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "git {args:?} failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8_lossy(&output.stdout).trim().to_string()
    }

    #[test]
    fn api_tree_only_replaces_the_fetch_while_upstream_sits_on_the_tracked_tip() {
        let temp = tempfile::tempdir().unwrap();
        let remote = temp.path().join("remote");
        std::fs::create_dir_all(&remote).unwrap();
        git(&remote, &["init", "-q", "--initial-branch=main"]);
        git(&remote, &["config", "user.email", "test@example.com"]);
        git(&remote, &["config", "user.name", "SkillStar Tests"]);
        std::fs::write(remote.join("SKILL.md"), "v1").unwrap();
        git(&remote, &["add", "."]);
        git(&remote, &["commit", "-q", "-m", "v1"]);
        let clone = temp.path().join("clone");
        git(
            temp.path(),
            &[
                "clone",
                "-q",
                remote.to_str().unwrap(),
                clone.to_str().unwrap(),
            ],
        );
        let tracked_tip = git(&clone, &["rev-parse", "origin/HEAD"]);

        let at_tip = ApiRemoteTree {
            folders: HashMap::from([(String::new(), tracked_tip)]),
        };
        assert!(api_tree_is_at_tracked_tip(&clone, &at_tip));

        let moved = ApiRemoteTree {
            folders: HashMap::from([(String::new(), "a".repeat(40))]),
        };
        assert!(
            !api_tree_is_at_tracked_tip(&clone, &moved),
            "a moved upstream must fall back to the git fetch"
        );
    }
}
