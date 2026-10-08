//! Upstream check for locked Skills (D-081 tree-SHA comparison).
//!
//! Lock entries are grouped by `(source_url, ref)`. A `github.com` group asks
//! the REST fast path first: the Trees API at the ref (or `HEAD`, the same
//! default branch a clone checks out when the lock records no ref), one
//! request per nested subtree, and `git/commits/{sha}` for a root Skill — the
//! Trees API reports the *commit* SHA at a branch, while the lock stores
//! `HEAD^{tree}`. Anything the API cannot answer falls back to one trees-only
//! temp snapshot of the group (`blob:none`, no worktree) in the caller's Git
//! session, so private repositories work and no file content travels.
//!
//! Groups run with bounded concurrency. A GitHub rate limit is persisted with
//! its reset time; until then every check skips the API and goes straight to
//! the clone path.

use std::collections::{BTreeMap, HashMap};
use std::future::Future;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use crate::fetch;
use crate::git::transport::GitOperationSession;
use crate::skill_lock::{SkillLock, SkillLockEntry};
use crate::source_resolver::Source;
use crate::update::Upstream;
use crate::update_api::{ApiRemoteTree, FastPathFailure};

/// Source groups checked at the same time (API calls or temp clones).
const MAX_CONCURRENT_GROUPS: usize = 4;

/// The GitHub REST reads the check needs; tests substitute recorded bodies.
pub(crate) trait TreeApi: Send + Sync + 'static {
    /// `git/trees/{rev}` (branch, `HEAD`, commit or tree SHA), non-recursive.
    fn tree(
        &self,
        owner: &str,
        repo: &str,
        rev: &str,
    ) -> impl Future<Output = Result<ApiRemoteTree, FastPathFailure>> + Send;

    /// `git/commits/{sha}` → `tree.sha`.
    fn commit_tree(
        &self,
        owner: &str,
        repo: &str,
        commit: &str,
    ) -> impl Future<Output = Result<String, FastPathFailure>> + Send;
}

/// Production reads over `probe_http_client` / the anonymous accelerator chain.
pub(crate) struct GitHubTreeApi {
    pub(crate) token: Option<String>,
}

impl TreeApi for GitHubTreeApi {
    fn tree(
        &self,
        owner: &str,
        repo: &str,
        rev: &str,
    ) -> impl Future<Output = Result<ApiRemoteTree, FastPathFailure>> + Send {
        let (owner, repo, rev) = (owner.to_string(), repo.to_string(), rev.to_string());
        let token = self.token.clone();
        async move {
            crate::update_api::fetch_remote_subtree_hashes(&owner, &repo, &rev, token.as_deref())
                .await
        }
    }

    fn commit_tree(
        &self,
        owner: &str,
        repo: &str,
        commit: &str,
    ) -> impl Future<Output = Result<String, FastPathFailure>> + Send {
        let (owner, repo, commit) = (owner.to_string(), repo.to_string(), commit.to_string());
        let token = self.token.clone();
        async move {
            crate::update_api::fetch_commit_tree_sha(&owner, &repo, &commit, token.as_deref()).await
        }
    }
}

/// Check every entry; names missing from the result are unknown.
pub(crate) async fn check_upstream_with<A: TreeApi>(
    entries: &[(String, SkillLockEntry)],
    api: Arc<A>,
    session: &GitOperationSession,
) -> BTreeMap<String, Upstream> {
    let mut verdicts = BTreeMap::new();
    let api_open = Arc::new(AtomicBool::new(!cooldown_active(now_unix())));
    let permits = Arc::new(tokio::sync::Semaphore::new(MAX_CONCURRENT_GROUPS));
    let mut tasks = tokio::task::JoinSet::new();

    let normalized = entries
        .iter()
        .map(|(name, entry)| {
            let mut entry = entry.clone();
            entry.git_ref = crate::update::effective_git_ref(entry.git_ref.clone());
            (name.clone(), entry)
        })
        .collect::<Vec<_>>();
    for ((source_url, git_ref), group) in SkillLock::by_source_group(&normalized) {
        let (updatable, inert): (Vec<_>, Vec<_>) = group
            .into_iter()
            .partition(|(_, entry)| entry.source_type.is_updatable());
        for (name, _) in inert {
            verdicts.insert(name, Upstream::Unknown);
        }
        if updatable.is_empty() {
            continue;
        }
        let api = api.clone();
        let api_open = api_open.clone();
        let permits = permits.clone();
        let session = session.clone();
        tasks.spawn(async move {
            let _permit = permits.acquire_owned().await;
            check_group(&source_url, git_ref, updatable, &*api, &api_open, &session).await
        });
    }
    while let Some(joined) = tasks.join_next().await {
        match joined {
            Ok(group) => verdicts.extend(group),
            Err(error) => {
                tracing::warn!(target: "skill_update_check", "update check task failed: {error}");
            }
        }
    }
    verdicts
}

async fn check_group<A: TreeApi>(
    source_url: &str,
    git_ref: Option<String>,
    group: Vec<(String, SkillLockEntry)>,
    api: &A,
    api_open: &AtomicBool,
    session: &GitOperationSession,
) -> BTreeMap<String, Upstream> {
    let mut verdicts = BTreeMap::new();
    let mut unresolved: Vec<(String, String)> = Vec::new();
    let paths = group
        .into_iter()
        .map(|(name, entry)| (name, entry.skill_path.unwrap_or_default()))
        .collect::<Vec<_>>();

    let github = crate::update_api::owner_repo_from_git_url(source_url);
    match github {
        Some((owner, repo)) if api_open.load(Ordering::SeqCst) => {
            let rev = git_ref.clone().unwrap_or_else(|| "HEAD".to_string());
            match api.tree(&owner, &repo, &rev).await {
                Ok(top) => {
                    let mut subtrees = HashMap::new();
                    for (name, path) in paths {
                        let resolved =
                            resolve_path(api, &owner, &repo, &top, &path, &mut subtrees, api_open)
                                .await;
                        match resolved {
                            Resolution::Hash(hash) => {
                                verdicts.insert(name, Upstream::Hash(hash));
                            }
                            Resolution::Missing => {
                                verdicts.insert(name, Upstream::Removed);
                            }
                            Resolution::Unresolved => unresolved.push((name, path)),
                        }
                    }
                }
                Err(failure) => {
                    note_failure(&failure, api_open);
                    unresolved = paths;
                }
            }
        }
        _ => unresolved = paths,
    }

    if !unresolved.is_empty() {
        verdicts.extend(clone_fallback(source_url, git_ref, unresolved, session).await);
    }
    verdicts
}

enum Resolution {
    Hash(String),
    /// The ref resolved and the path is not a directory in it.
    Missing,
    /// The API could not answer; the clone fallback decides.
    Unresolved,
}

async fn resolve_path<A: TreeApi>(
    api: &A,
    owner: &str,
    repo: &str,
    top: &ApiRemoteTree,
    path: &str,
    subtrees: &mut HashMap<String, ApiRemoteTree>,
    api_open: &AtomicBool,
) -> Resolution {
    if path.is_empty() {
        let Some(commit) = top.folders.get("") else {
            return Resolution::Unresolved;
        };
        return match api.commit_tree(owner, repo, commit).await {
            Ok(tree) => Resolution::Hash(tree),
            Err(failure) => {
                note_failure(&failure, api_open);
                Resolution::Unresolved
            }
        };
    }
    let parts = path
        .split('/')
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>();
    let mut current = top.clone();
    for (index, part) in parts.iter().enumerate() {
        let Some(sha) = current.folders.get(*part).cloned() else {
            return Resolution::Missing;
        };
        if index + 1 == parts.len() {
            return Resolution::Hash(sha);
        }
        if let Some(cached) = subtrees.get(&sha) {
            current = cached.clone();
            continue;
        }
        if !api_open.load(Ordering::SeqCst) {
            return Resolution::Unresolved;
        }
        match api.tree(owner, repo, &sha).await {
            Ok(tree) => {
                subtrees.insert(sha, tree.clone());
                current = tree;
            }
            Err(failure) => {
                note_failure(&failure, api_open);
                return Resolution::Unresolved;
            }
        }
    }
    Resolution::Unresolved
}

/// One trees-only temp snapshot (`blob:none`, no worktree) for the group's
/// unresolved paths, in the caller's session — the comparison reads tree
/// SHAs and never needs file content.
async fn clone_fallback(
    source_url: &str,
    git_ref: Option<String>,
    unresolved: Vec<(String, String)>,
    session: &GitOperationSession,
) -> BTreeMap<String, Upstream> {
    let unknown = |names: &[(String, String)]| {
        names
            .iter()
            .map(|(name, _)| (name.clone(), Upstream::Unknown))
            .collect::<BTreeMap<_, _>>()
    };
    let Ok(mut spec) = Source::parse(source_url) else {
        return unknown(&unresolved);
    };
    spec.git_ref = git_ref;
    spec.subpath = None;
    spec.skill_filter = None;
    let session = session.clone();
    let names = unresolved.clone();
    let measured = tokio::task::spawn_blocking(move || {
        let checkout = fetch::fetch_trees_only(&spec, &session)?;
        Ok::<_, anyhow::Error>(
            unresolved
                .into_iter()
                .map(|(name, path)| {
                    let hash = fetch::folder_tree_hash(
                        checkout.dir(),
                        (!path.is_empty()).then_some(path.as_str()),
                    );
                    let verdict = match hash {
                        Some(hash) => Upstream::Hash(hash),
                        None => Upstream::Removed,
                    };
                    (name, verdict)
                })
                .collect::<BTreeMap<_, _>>(),
        )
    })
    .await;
    match measured {
        Ok(Ok(verdicts)) => verdicts,
        Ok(Err(error)) => {
            tracing::debug!(target: "skill_update_check", source = %source_url, "update check clone failed: {error:#}");
            unknown(&names)
        }
        Err(error) => {
            tracing::warn!(target: "skill_update_check", "update check clone task failed: {error}");
            unknown(&names)
        }
    }
}

fn note_failure(failure: &FastPathFailure, api_open: &AtomicBool) {
    if let crate::update_api::FastPathFailureKind::RateLimited { reset_unix } = failure.kind {
        api_open.store(false, Ordering::SeqCst);
        record_cooldown(reset_unix);
    }
    if !failure.is_expected() {
        tracing::debug!(target: "skill_update_check", "GitHub fast path failed: {failure}");
    }
}

// ── rate-limit cooldown ─────────────────────────────────────────────

fn now_unix() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| elapsed.as_secs())
        .unwrap_or(0)
}

pub(crate) fn cooldown_active(now: u64) -> bool {
    ss_core::infra::github_api_cooldown::active(now)
}

fn record_cooldown(reset_unix: u64) {
    ss_core::infra::github_api_cooldown::record(reset_unix);
}

#[cfg(test)]
#[path = "update_check_tests.rs"]
mod tests;
