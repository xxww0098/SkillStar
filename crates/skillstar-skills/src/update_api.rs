//! GitHub API fast path for update detection.
//!
//! Patrol currently `git fetch`es every unique repo every cycle just to learn
//! whether any tracked skill moved. For `github.com` sources that whole fetch
//! can be replaced by one Trees API call per repo
//! (`GET /repos/{owner}/{repo}/git/trees/{ref}`): the response carries the
//! commit SHA at the ref plus the tree SHA of every top-level directory.
//! The commit SHA answers "did upstream move?"; once it matches the local
//! tracked ref, deeper folder hashes resolve from the local object store
//! (`git rev-parse <ref>:<folder>`) bit-for-bit identically.
//!
//! Credentials never enter the git session (D-014). When the user already has
//! a SkillStar GitHub App session, that token is sent as an HTTP Bearer so
//! the request uses the authenticated 5 000/hour budget instead of the
//! anonymous 60/hour IP budget. Otherwise the call stays anonymous. Private
//! repositories, rate limits, network failures, and truncated trees all fall
//! back to the authenticated git fetch; a broken or unavailable API therefore
//! never degrades update correctness — only speed.

use std::collections::HashMap;
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result, anyhow};
use reqwest::header::HeaderMap;

/// Per-call HTTP timeout.
pub const API_TIMEOUT: Duration = Duration::from_secs(10);
/// Unauthenticated GitHub API budget is 60 requests/hour per IP. Cap the
/// per-cycle API usage below that so a single patrol never burns the budget
/// (repos beyond the cap fall back to the git fetch path).
pub const MAX_API_REPOS_PER_CYCLE: usize = 40;
/// Concurrency for the API pre-pass.
pub const API_CONCURRENCY: usize = 8;

/// Unix timestamp until which the fast path should not be attempted, because
/// the last Trees call exhausted the GitHub rate limit. `0` means "not
/// blocked". Concurrent 403s keep the furthest reset via `fetch_max`.
static RATE_LIMIT_RESET_UNIX: AtomicU64 = AtomicU64::new(0);

/// Remote subtree hashes for one repository, obtained from the Trees API.
///
/// `folders[""]` holds the Trees API top-level `sha`. When the API is
/// queried with a branch or commit (the production path), GitHub puts that
/// **commit SHA** in `sha`, not `commit.tree.sha`. Every other key is a
/// top-level directory name and a real git tree SHA — the same value
/// `git rev-parse <ref>:<folder>` produces locally. Deeper folders are not
/// listed (the fetch is non-recursive); consumers fall back to that local
/// `rev-parse`, valid because the tracked-tip gate has already proven the
/// remote tip is the local tracked commit.
#[derive(Debug, Clone, Default)]
pub struct ApiRemoteTree {
    pub folders: HashMap<String, String>,
}

impl ApiRemoteTree {
    /// Subtree hash for `source_folder` (`None` = repo root skill).
    pub fn subtree_hash(&self, source_folder: Option<&str>) -> Option<&str> {
        match source_folder {
            Some(folder) if !folder.is_empty() => self.folders.get(folder).map(String::as_str),
            _ => self.folders.get("").map(String::as_str),
        }
    }
}

/// Why one Trees API call could not replace `git fetch`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FastPathFailure {
    pub owner: String,
    pub repo: String,
    pub git_ref: String,
    pub kind: FastPathFailureKind,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FastPathFailureKind {
    /// GitHub `X-RateLimit-Remaining: 0` (or HTTP 429). The fast path should
    /// stay off until `reset_unix`.
    RateLimited {
        reset_unix: u64,
    },
    /// HTTP failure that is a designed fallback (private repo, missing ref).
    Http {
        status: u16,
    },
    Transport(String),
    Response(String),
}

impl FastPathFailure {
    fn transport(owner: &str, repo: &str, git_ref: &str, message: String) -> Self {
        Self {
            owner: owner.to_string(),
            repo: repo.to_string(),
            git_ref: git_ref.to_string(),
            kind: FastPathFailureKind::Transport(message),
        }
    }

    fn response(owner: &str, repo: &str, git_ref: &str, message: String) -> Self {
        Self {
            owner: owner.to_string(),
            repo: repo.to_string(),
            git_ref: git_ref.to_string(),
            kind: FastPathFailureKind::Response(message),
        }
    }

    /// Private repos, missing refs, and rate limits are the documented
    /// fallback — they are not operational faults.
    pub fn is_expected(&self) -> bool {
        match self.kind {
            FastPathFailureKind::RateLimited { .. } => true,
            FastPathFailureKind::Http { status } => matches!(status, 401 | 403 | 404),
            FastPathFailureKind::Transport(_) | FastPathFailureKind::Response(_) => false,
        }
    }

    pub fn is_rate_limited(&self) -> bool {
        matches!(self.kind, FastPathFailureKind::RateLimited { .. })
    }
}

impl std::fmt::Display for FastPathFailure {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self.kind {
            FastPathFailureKind::RateLimited { .. } => write!(
                formatter,
                "GitHub Trees API rate limited for {}/{}@{}",
                self.owner, self.repo, self.git_ref
            ),
            FastPathFailureKind::Http { status } => write!(
                formatter,
                "GitHub Trees API returned {status} for {}/{}@{}",
                self.owner, self.repo, self.git_ref
            ),
            FastPathFailureKind::Transport(message) | FastPathFailureKind::Response(message) => {
                write!(
                    formatter,
                    "{message} ({}/{}@{})",
                    self.owner, self.repo, self.git_ref
                )
            }
        }
    }
}

impl std::error::Error for FastPathFailure {}

/// `true` while GitHub has told us the current token/IP is out of quota.
pub fn api_fast_path_blocked() -> bool {
    RATE_LIMIT_RESET_UNIX.load(Ordering::Relaxed) > now_unix()
}

fn note_rate_limited_until(reset_unix: u64) {
    RATE_LIMIT_RESET_UNIX.fetch_max(reset_unix, Ordering::Relaxed);
}

fn now_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0)
}

/// Load the SkillStar GitHub App access token when a non-expired session
/// already exists. Missing, unreadable, or expired credentials yield `None`
/// and the caller stays anonymous — this path never starts a login.
pub(crate) fn optional_github_api_token() -> Option<String> {
    use skillstar_github_auth::{CredentialStore, FileCredentialStore};

    let credential = FileCredentialStore::default().load().ok().flatten()?;
    if credential
        .access_expires_at()
        .is_some_and(|expires_at| chrono::Utc::now() >= expires_at)
    {
        return None;
    }
    let token = credential.access_token();
    (!token.is_empty()).then(|| token.to_string())
}

/// Extract `(owner, repo)` from a GitHub clone URL.
///
/// Accepts `https://github.com/owner/repo(.git)`,
/// `git@github.com:owner/repo.git`, and `ssh://git@github.com/owner/repo.git`.
/// Returns `None` for every other host so the API fast path never sends
/// non-GitHub URLs to the GitHub API.
pub fn owner_repo_from_git_url(url: &str) -> Option<(String, String)> {
    let trimmed = url.trim().trim_end_matches('/');
    let (authority, path) = if let Some(rest) = trimmed.strip_prefix("https://") {
        let (authority, path) = rest.split_once('/')?;
        (authority.to_ascii_lowercase(), path)
    } else if let Some(rest) = trimmed.strip_prefix("http://") {
        let (authority, path) = rest.split_once('/')?;
        (authority.to_ascii_lowercase(), path)
    } else if let Some(rest) = trimmed.strip_prefix("git@") {
        let (authority, path) = rest.split_once(':')?;
        (authority.to_ascii_lowercase(), path)
    } else {
        let rest = trimmed.strip_prefix("ssh://")?;
        let rest = rest.trim_start_matches("git@");
        let (authority, path) = rest.split_once('/')?;
        (authority.to_ascii_lowercase(), path)
    };

    if authority != "github.com" {
        return None;
    }
    let mut parts = path.trim_end_matches(".git").split('/');
    let owner = parts.next()?.to_string();
    let repo = parts.next()?.to_string();
    if owner.is_empty() || repo.is_empty() {
        return None;
    }
    Some((owner, repo))
}

/// Resolve the remote ref to ask the API about: the pinned ref when the
/// checkout is pinned, otherwise the default branch from the local
/// `origin/HEAD` symbolic ref. `None` means the ref cannot be determined
/// locally — the caller falls back to the git fetch path.
pub fn remote_ref_for(repo_root: &Path, pinned_ref: Option<&str>) -> Option<String> {
    if let Some(pinned) = pinned_ref
        && !pinned.is_empty()
    {
        return Some(pinned.to_string());
    }
    let output = skillstar_core::infra::path_env::command_with_path("git")
        .current_dir(repo_root)
        .args(["symbolic-ref", "--quiet", "refs/remotes/origin/HEAD"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let resolved = String::from_utf8_lossy(&output.stdout).trim().to_string();
    resolved
        .strip_prefix("refs/remotes/origin/")
        .map(str::to_string)
        .filter(|branch| !branch.is_empty())
}

/// Parse the recursive Trees API response body into subtree hashes.
pub fn parse_tree_response(body: &str) -> Result<ApiRemoteTree> {
    let value: serde_json::Value =
        serde_json::from_str(body).with_context(|| "Failed to parse GitHub Trees API response")?;
    if value["truncated"].as_bool().unwrap_or(false) {
        return Err(anyhow!(
            "GitHub Trees API response was truncated (repository too large for the fast path)"
        ));
    }
    let mut folders = HashMap::new();
    if let Some(root_sha) = value["sha"].as_str() {
        folders.insert(String::new(), root_sha.to_string());
    }
    if let Some(entries) = value["tree"].as_array() {
        for entry in entries {
            if entry["type"].as_str() == Some("tree")
                && let (Some(path), Some(sha)) = (entry["path"].as_str(), entry["sha"].as_str())
            {
                folders.insert(path.to_string(), sha.to_string());
            }
        }
    }
    if folders.is_empty() {
        return Err(anyhow!(
            "GitHub Trees API response contained no tree entries"
        ));
    }
    Ok(ApiRemoteTree { folders })
}

/// Fetch the tree of one GitHub repository at `git_ref`.
///
/// `token` is the SkillStar GitHub App access token when the user is signed
/// in, or `None` for the anonymous budget. Every failure (private repo, rate
/// limit, network, truncation) returns an error so the caller can fall back
/// to the git fetch path.
pub async fn fetch_remote_subtree_hashes(
    owner: &str,
    repo: &str,
    git_ref: &str,
    token: Option<&str>,
) -> Result<ApiRemoteTree, FastPathFailure> {
    let client = skillstar_core::infra::http_client::probe_http_client(API_TIMEOUT)
        .map_err(|error| FastPathFailure::transport(owner, repo, git_ref, error.to_string()))?;
    // Non-recursive on purpose: `?recursive=1` returns the whole repo tree
    // (multi-MB for large repos — stably/orca is ~5 MB), which blows the
    // per-call timeout on slow or proxied networks. The shallow listing is a
    // few KB and still carries the commit SHA plus every top-level dir; the
    // consumer resolves deeper folders from the local object store, which the
    // tracked-tip gate guarantees holds identical hashes.
    let url = format!(
        "https://api.github.com/repos/{owner}/{repo}/git/trees/{}",
        urlencode_ref(git_ref)
    );
    let mut request = client
        .get(&url)
        .header(
            "User-Agent",
            concat!("SkillStar/", env!("CARGO_PKG_VERSION")),
        )
        .header("Accept", "application/vnd.github+json")
        .header("X-GitHub-Api-Version", "2022-11-28");
    if let Some(token) = token {
        request = request.bearer_auth(token);
    }
    let response = request.send().await.map_err(|error| {
        FastPathFailure::transport(
            owner,
            repo,
            git_ref,
            format!("Failed to call GitHub Trees API: {error}"),
        )
    })?;
    let status = response.status();
    if !status.is_success() {
        let failure = classify_http_failure(
            status.as_u16(),
            header_u64(response.headers(), "x-ratelimit-remaining"),
            header_u64(response.headers(), "x-ratelimit-reset"),
            owner,
            repo,
            git_ref,
            now_unix(),
        );
        if let FastPathFailureKind::RateLimited { reset_unix } = failure.kind {
            note_rate_limited_until(reset_unix);
        }
        return Err(failure);
    }
    let body = response.text().await.map_err(|error| {
        FastPathFailure::transport(
            owner,
            repo,
            git_ref,
            format!("Failed to read GitHub Trees API response: {error}"),
        )
    })?;
    parse_tree_response(&body)
        .map_err(|error| FastPathFailure::response(owner, repo, git_ref, error.to_string()))
}

pub(crate) fn classify_http_failure(
    status: u16,
    rate_limit_remaining: Option<u64>,
    rate_limit_reset: Option<u64>,
    owner: &str,
    repo: &str,
    git_ref: &str,
    now_unix: u64,
) -> FastPathFailure {
    let rate_limited = status == 429 || rate_limit_remaining == Some(0);
    let kind = if rate_limited {
        FastPathFailureKind::RateLimited {
            reset_unix: rate_limit_reset.unwrap_or(now_unix.saturating_add(3600)),
        }
    } else {
        FastPathFailureKind::Http { status }
    };
    FastPathFailure {
        owner: owner.to_string(),
        repo: repo.to_string(),
        git_ref: git_ref.to_string(),
        kind,
    }
}

fn header_u64(headers: &HeaderMap, name: &str) -> Option<u64> {
    headers.get(name)?.to_str().ok()?.trim().parse().ok()
}

/// Percent-encode a ref for the URL path. GitHub refs are usually safe
/// (`main`, `v1.2.3`) but tags can contain slashes; keep it minimal and
/// correct for the common cases.
fn urlencode_ref(git_ref: &str) -> String {
    let mut encoded = String::with_capacity(git_ref.len());
    for byte in git_ref.bytes() {
        match byte {
            b'a'..=b'z' | b'A'..=b'Z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'/' => {
                encoded.push(byte as char);
            }
            _ => encoded.push_str(&format!("%{byte:02X}")),
        }
    }
    encoded
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn owner_repo_parsing() {
        for (url, expected) in [
            ("https://github.com/owner/repo", Some(("owner", "repo"))),
            ("https://github.com/owner/repo.git", Some(("owner", "repo"))),
            ("https://github.com/owner/repo/", Some(("owner", "repo"))),
            ("git@github.com:owner/repo.git", Some(("owner", "repo"))),
            (
                "ssh://git@github.com/owner/repo.git",
                Some(("owner", "repo")),
            ),
            ("https://gitlab.com/owner/repo", None),
            ("https://github.com/owner", None),
            ("https://github.com/", None),
            ("", None),
            (
                "https://github.com/owner/repo/tree/main/skills",
                Some(("owner", "repo")),
            ),
        ] {
            let got = owner_repo_from_git_url(url);
            let expected = expected.map(|(o, r)| (o.to_string(), r.to_string()));
            assert_eq!(got, expected, "url {url:?}");
        }
    }

    #[test]
    fn tree_response_parses_subtree_hashes() {
        let body = r#"{
            "sha": "root-tree-sha",
            "truncated": false,
            "tree": [
                {"path": "skills", "mode": "040000", "type": "tree", "sha": "skills-tree"},
                {"path": "skills/demo", "mode": "040000", "type": "tree", "sha": "demo-tree"},
                {"path": "skills/demo/SKILL.md", "mode": "100644", "type": "blob", "sha": "blob-sha"},
                {"path": "README.md", "mode": "100644", "type": "blob", "sha": "readme-sha"}
            ]
        }"#;

        let tree = parse_tree_response(body).expect("valid response parses");
        // Production queries use a branch/commit, and GitHub then puts that
        // commit SHA in `sha`. The parser still stores whatever the API
        // returned at the root; the checker accepts either object type.
        assert_eq!(tree.subtree_hash(None), Some("root-tree-sha"));
        assert_eq!(tree.subtree_hash(Some("skills")), Some("skills-tree"));
        assert_eq!(tree.subtree_hash(Some("skills/demo")), Some("demo-tree"));
        // Blobs are not subtrees.
        assert_eq!(tree.subtree_hash(Some("skills/demo/SKILL.md")), None);
        // Missing folder → unknown (caller preserves the badge).
        assert_eq!(tree.subtree_hash(Some("gone")), None);
    }

    #[test]
    fn truncated_response_is_rejected() {
        let body = r#"{"sha":"x","truncated":true,"tree":[]}"#;
        assert!(parse_tree_response(body).is_err());
    }

    #[test]
    fn url_encoding_keeps_safe_refs() {
        assert_eq!(urlencode_ref("main"), "main");
        assert_eq!(urlencode_ref("release/v1.2.3"), "release/v1.2.3");
        assert_eq!(urlencode_ref("a b"), "a%20b");
    }

    #[test]
    fn exhausted_rate_limit_is_expected_and_names_the_reset() {
        let failure = classify_http_failure(403, Some(0), Some(1_700_000_000), "o", "r", "main", 1);
        assert!(failure.is_expected());
        assert!(failure.is_rate_limited());
        assert_eq!(
            failure.kind,
            FastPathFailureKind::RateLimited {
                reset_unix: 1_700_000_000
            }
        );
        assert_eq!(
            failure.to_string(),
            "GitHub Trees API rate limited for o/r@main"
        );
    }

    #[test]
    fn rate_limit_without_reset_header_cools_down_for_an_hour() {
        let failure = classify_http_failure(429, None, None, "o", "r", "main", 100);
        assert_eq!(
            failure.kind,
            FastPathFailureKind::RateLimited { reset_unix: 3700 }
        );
    }

    #[test]
    fn private_or_missing_repos_are_expected_fallbacks() {
        for status in [401_u16, 403, 404] {
            let failure = classify_http_failure(status, Some(12), None, "o", "r", "main", 1);
            assert!(failure.is_expected(), "status {status}");
            assert!(!failure.is_rate_limited(), "status {status}");
            assert_eq!(failure.kind, FastPathFailureKind::Http { status });
        }
    }

    #[test]
    fn unexpected_http_failures_stay_loud() {
        let failure = classify_http_failure(500, Some(12), None, "o", "r", "main", 1);
        assert!(!failure.is_expected());
        assert!(!failure.is_rate_limited());
    }
}
