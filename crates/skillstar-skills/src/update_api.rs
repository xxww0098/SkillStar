//! GitHub API fast path for update detection.
//!
//! Patrol currently `git fetch`es every unique repo every cycle just to learn
//! whether any tracked skill moved. For `github.com` sources that whole fetch
//! can be replaced by one unauthenticated Trees API call per repo
//! (`GET /repos/{owner}/{repo}/git/trees/{ref}?recursive=1`): the API returns
//! the tree SHA of every directory, which is exactly the subtree hash the
//! local update check compares against the lockfile (the same trick `npx
//! skills` uses via `getSkillFolderHashFromTree`).
//!
//! This path is **anonymous only** — credentials never leave the git session.
//! Private repositories get a 404/401 from the API and fall back to the
//! authenticated git fetch; any other failure (rate limit, network, truncated
//! tree) does the same. A broken or unavailable API therefore never degrades
//! update correctness — only speed.

use std::collections::HashMap;
use std::path::Path;
use std::time::Duration;

use anyhow::{Context, Result, anyhow};

/// Per-call HTTP timeout.
pub const API_TIMEOUT: Duration = Duration::from_secs(10);
/// Unauthenticated GitHub API budget is 60 requests/hour per IP. Cap the
/// per-cycle API usage below that so a single patrol never burns the budget
/// (repos beyond the cap fall back to the git fetch path).
pub const MAX_API_REPOS_PER_CYCLE: usize = 40;
/// Concurrency for the API pre-pass.
pub const API_CONCURRENCY: usize = 8;

/// Remote subtree hashes for one repository, obtained from the Trees API.
///
/// `folders[""]` holds the Trees API top-level `sha`. When the API is
/// queried with a branch or commit (the production path), GitHub puts that
/// **commit SHA** in `sha`, not `commit.tree.sha`. Every other key is a
/// directory path relative to the repo root and a real git tree SHA — the
/// same value `git rev-parse <ref>:<folder>` produces locally.
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

/// Fetch the recursive tree of one GitHub repository at `git_ref`.
///
/// Unauthenticated: only useful for public repositories. Every failure
/// (private repo, rate limit, network, truncation) returns an error so the
/// caller can fall back to the git fetch path.
pub async fn fetch_remote_subtree_hashes(
    owner: &str,
    repo: &str,
    git_ref: &str,
) -> Result<ApiRemoteTree> {
    let client = skillstar_core::infra::http_client::probe_http_client(API_TIMEOUT)
        .context("Failed to build GitHub API HTTP client")?;
    let url = format!(
        "https://api.github.com/repos/{owner}/{repo}/git/trees/{}?recursive=1",
        urlencode_ref(git_ref)
    );
    let response = client
        .get(&url)
        .header(
            "User-Agent",
            concat!("SkillStar/", env!("CARGO_PKG_VERSION")),
        )
        .header("Accept", "application/vnd.github+json")
        .send()
        .await
        .with_context(|| format!("Failed to call GitHub Trees API for {owner}/{repo}"))?;
    if !response.status().is_success() {
        return Err(anyhow!(
            "GitHub Trees API returned {} for {owner}/{repo}@{git_ref}",
            response.status()
        ));
    }
    let body = response
        .text()
        .await
        .with_context(|| format!("Failed to read GitHub Trees API response for {owner}/{repo}"))?;
    parse_tree_response(&body)
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
}
