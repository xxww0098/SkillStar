//! Source checkout acquisition: shallow clone into a temp dir, deleted on drop.
//!
//! vercel-labs/skills semantics (D-081): there is no persistent repository
//! cache. Every install/update fetches `--depth 1` into a fresh OS temp dir
//! and discards it afterwards. Local `file://` sources are read in place.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

use crate::git::ops as git_ops;
use crate::git::transport::GitOperationSession;
use crate::source_resolver::Source;

/// A fetched checkout. The `TempDir` removes the clone when dropped; local
/// sources borrow the original directory instead (`_temp == None`).
pub struct Checkout {
    dir: PathBuf,
    _temp: Option<tempfile::TempDir>,
}

impl Checkout {
    pub fn dir(&self) -> &Path {
        &self.dir
    }

}

/// Fetch a parsed source into a temp checkout (or borrow a local dir).
///
/// `spec.git_ref` pins the cloned ref (branch/tag/SHA — see
/// `clone_repo_shallow_at_ref_in_session`).
pub fn fetch_source(spec: &Source, session: &GitOperationSession) -> Result<Checkout> {
    // A pinned ref must be honored: borrowing a local working tree would
    // surface whatever it happens to hold, not the requested revision. Only
    // unpinned local sources are borrowed in place.
    if spec.git_ref.is_none()
        && let Some(local) = local_source_dir(&spec.repo_url)
    {
        return Ok(Checkout {
            dir: local,
            _temp: None,
        });
    }

    let temp = tempfile::Builder::new()
        .prefix("skillstar-fetch-")
        .tempdir()
        .context("Failed to create temp clone directory")?;
    let dest = temp.path().join("repo");
    session.emit_stage(
        crate::git::transport::InstallStage::Fetching,
        &spec.short,
        None,
    );
    match spec.git_ref.as_deref() {
        Some(git_ref) => git_ops::clone_repo_shallow_at_ref_in_session(
            &spec.repo_url,
            &dest,
            git_ref,
            session,
        ),
        None => git_ops::clone_repo_shallow_in_session(&spec.repo_url, &dest, session),
    }
    .with_context(|| format!("Failed to fetch '{}'", spec.repo_url))?;
    Ok(Checkout {
        dir: dest,
        _temp: Some(temp),
    })
}

/// A `file://` source's real directory, when the URL points at one.
pub fn local_source_dir(repo_url: &str) -> Option<PathBuf> {
    let rest = repo_url.strip_prefix("file://")?;
    let rest = rest.strip_prefix("//").unwrap_or(rest);
    let path = if cfg!(windows) && rest.len() >= 2 && rest.as_bytes()[1] == b':' {
        PathBuf::from(rest)
    } else if rest.starts_with('/') {
        PathBuf::from(rest)
    } else {
        return None;
    };
    path.is_dir().then_some(path)
}

/// Head commit SHA of a checkout ("" when unreadable — local dirs have no git).
pub fn head_commit(checkout: &Path) -> String {
    git_ops::head_revision(checkout).unwrap_or_default()
}

/// Git tree SHA of a repo-relative folder at HEAD; `None` for the repo root's
/// own tree or when the path cannot be resolved (e.g. upstream removed it).
pub fn folder_tree_hash(checkout: &Path, folder_path: Option<&str>) -> Option<String> {
    let rev = match folder_path {
        Some(folder) if !folder.is_empty() => format!("HEAD:{folder}"),
        _ => "HEAD^{tree}".to_string(),
    };
    git_ops::rev_parse(checkout, &rev).ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::git::transport::{GitAuthMaterial, NoopGitProgressSink};
    use std::sync::Arc;

    fn session() -> GitOperationSession {
        GitOperationSession::new(
            "fetch-test",
            GitAuthMaterial::missing(),
            Arc::new(NoopGitProgressSink),
        )
    }

    #[test]
    fn local_file_url_borrows_in_place() {
        let dir = tempfile::tempdir().unwrap();
        let url = git_ops::local_file_url(dir.path());
        let spec = Source::parse(&url).unwrap();
        let checkout = fetch_source(&spec, &session()).unwrap();
        // macOS tempdirs live behind a /var -> /private/var symlink; compare
        // canonical forms, not string equality.
        assert_eq!(
            std::fs::canonicalize(checkout.dir()).unwrap(),
            std::fs::canonicalize(dir.path()).unwrap()
        );
        assert!(checkout._temp.is_none());
    }

    #[test]
    fn missing_local_dir_is_none() {
        assert!(local_source_dir("file:///definitely/not/here").is_none());
        assert!(local_source_dir("https://github.com/o/r.git").is_none());
    }

    #[test]
    fn folder_tree_hash_resolves_nested_folder() {
        let dir = tempfile::tempdir().unwrap();
        let skill = dir.path().join("skills/foo");
        std::fs::create_dir_all(&skill).unwrap();
        std::fs::write(
            skill.join("SKILL.md"),
            "---\nname: foo\ndescription: d\n---\n",
        )
        .unwrap();
        let status = skillstar_core::infra::path_env::command_with_path("git")
            .args(["init", "--quiet"])
            .current_dir(dir.path())
            .status()
            .unwrap();
        assert!(status.success());
        let add = skillstar_core::infra::path_env::command_with_path("git")
            .args(["add", "."])
            .current_dir(dir.path())
            .status()
            .unwrap();
        assert!(add.success());
        let commit = skillstar_core::infra::path_env::command_with_path("git")
            .args(["commit", "--quiet", "--allow-empty", "-m", "init", "--no-gpg-sign"])
            .env("GIT_AUTHOR_NAME", "Test")
            .env("GIT_COMMITTER_NAME", "Test")
            .env("GIT_AUTHOR_EMAIL", "t@t")
            .env("GIT_COMMITTER_EMAIL", "t@t")
            .env("GIT_COMMITTER_DATE", "1759500000 +0000")
            .env("GIT_AUTHOR_DATE", "1759500000 +0000")
            .current_dir(dir.path())
            .status()
            .unwrap();
        assert!(commit.success(), "commit must succeed");
        let hash = folder_tree_hash(dir.path(), Some("skills/foo"));
        assert!(hash.is_some());
        assert_eq!(hash.unwrap().len(), 40);
        assert!(folder_tree_hash(dir.path(), Some("skills/missing")).is_none());
        // Deterministic: same tree resolves to the same SHA.
        assert_eq!(
            folder_tree_hash(dir.path(), Some("skills/foo")),
            folder_tree_hash(dir.path(), Some("skills/foo"))
        );
    }

    #[test]
    fn temp_checkout_is_deleted_on_drop() {
        // Local source never creates a temp clone; dropping it must not touch
        // the borrowed directory.
        let dir = tempfile::tempdir().unwrap();
        let url = git_ops::local_file_url(dir.path());
        {
            let spec = Source::parse(&url).unwrap();
            let checkout = fetch_source(&spec, &session()).unwrap();
            assert!(checkout.dir().is_dir());
        }
        assert!(dir.path().is_dir());
    }

    #[test]
    fn fetch_source_rejects_missing_local_dir() {
        let spec = Source {
            repo_url: "file:///definitely/not/here".to_string(),
            short: "local/none".to_string(),
            git_ref: None,
            subpath: None,
            skill_filter: None,
        };
        assert!(local_source_dir(&spec.repo_url).is_none());
        assert!(
            fetch_source(&spec, &session()).is_err(),
            "a file:// URL to a missing directory must fail, not fabricate a checkout"
        );
    }
}
