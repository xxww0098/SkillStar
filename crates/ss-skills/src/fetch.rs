//! Source acquisition: cached sparse imports and temporary full snapshots.
//!
//! Ordinary discovery fetches manifests, then installation materializes
//! only selected folders. Full-snapshot consumers retain the full-fetch entry.
//! Unpinned local `file://` sources are read in place and never sparsified.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

use crate::git::ops as git_ops;
use crate::git::transport::GitOperationSession;
use crate::source_resolver::Source;

mod cache;
pub use cache::{clear_import_cache, import_cache_key};

/// A fetched checkout owns its temporary directory or a persistent cache lock.
pub struct Checkout {
    dir: PathBuf,
    _temp: Option<tempfile::TempDir>,
    _cache_lock: Option<std::fs::File>,
    pub(crate) revision: Option<String>,
    pub(crate) cache_hit: bool,
    pub(crate) cached_at: Option<String>,
}

impl Checkout {
    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// Complete the selected skill folders before copying their payload.
    pub(crate) fn materialize(
        &self,
        folders: &[&str],
        session: &GitOperationSession,
    ) -> Result<()> {
        let patterns = folder_patterns(folders)?;
        if self._temp.is_some() || self._cache_lock.is_some() {
            git_ops::apply_sparse_patterns_in_session(&self.dir, &patterns, session)?;
            self.complete_linked_payloads(folders, session)?;
        }
        Ok(())
    }

    fn complete_linked_payloads(
        &self,
        folders: &[&str],
        session: &GitOperationSession,
    ) -> Result<()> {
        if (self._temp.is_none() && self._cache_lock.is_none()) || folders.contains(&"") {
            return Ok(());
        }
        let mut pending = folders
            .iter()
            .map(|folder| self.dir.join(folder))
            .collect::<Vec<_>>();
        while let Some(dir) = pending.pop() {
            // A missing target is diagnosed by the install gate, as before.
            if !dir.is_dir() {
                continue;
            }
            for entry in std::fs::read_dir(dir)? {
                let entry = entry?;
                let kind = entry.file_type()?;
                if kind.is_symlink() {
                    // ponytail: linked payloads use a full checkout to preserve copy
                    // semantics; resolve a bounded link closure if these become common.
                    git_ops::apply_sparse_patterns_in_session(
                        &self.dir,
                        &["/**".to_string()],
                        session,
                    )?;
                    return Ok(());
                }
                if kind.is_dir() {
                    pending.push(entry.path());
                }
            }
        }
        Ok(())
    }
}

/// Fetch only discovery inputs. Plugin declarations at both the repository
/// and scoped roots remain available to the existing discovery pipeline.
pub(crate) fn fetch_for_scan(spec: &Source, session: &GitOperationSession) -> Result<Checkout> {
    fetch_for_scan_with_refresh(spec, false, session)
}

pub(crate) fn fetch_for_scan_with_refresh(
    spec: &Source,
    refresh: bool,
    session: &GitOperationSession,
) -> Result<Checkout> {
    let scope = literal_pattern_path(spec.subpath.as_deref().unwrap_or_default())?;
    let mut patterns = vec![
        format!("{scope}**/SKILL.md"),
        format!("{scope}.claude-plugin/plugin.json"),
        format!("{scope}.claude-plugin/marketplace.json"),
    ];
    patterns.extend([
        "/.claude-plugin/plugin.json".to_string(),
        "/.claude-plugin/marketplace.json".to_string(),
    ]);
    fetch_import(spec, &patterns, refresh, None, session)
}

pub(crate) fn fetch_for_install(
    spec: &Source,
    folders: &[&str],
    session: &GitOperationSession,
) -> Result<Checkout> {
    fetch_for_install_at_revision(spec, folders, None, session)
}

pub(crate) fn fetch_for_install_at_revision(
    spec: &Source,
    folders: &[&str],
    revision: Option<&str>,
    session: &GitOperationSession,
) -> Result<Checkout> {
    let checkout = fetch_import(spec, &folder_patterns(folders)?, false, revision, session)?;
    checkout.complete_linked_payloads(folders, session)?;
    Ok(checkout)
}

fn fetch_import(
    spec: &Source,
    patterns: &[String],
    refresh: bool,
    revision: Option<&str>,
    session: &GitOperationSession,
) -> Result<Checkout> {
    if spec.git_ref.is_none() && local_source_dir(&spec.repo_url).is_some() {
        return fetch_source(spec, session);
    }
    cache::fetch(spec, patterns, refresh, revision, session)
}

fn folder_patterns(folders: &[&str]) -> Result<Vec<String>> {
    anyhow::ensure!(!folders.is_empty(), "No skills selected for installation");
    folders
        .iter()
        .map(|folder| {
            let prefix = literal_pattern_path(folder)?;
            Ok(format!("{prefix}**"))
        })
        .collect()
}

/// Anchor literal repository paths; never interpret input as a glob or Git
/// option. Reject control/traversal paths even for borrowed local sources.
fn literal_pattern_path(path: &str) -> Result<String> {
    if path.is_empty() {
        return Ok("/".to_string());
    }
    anyhow::ensure!(
        !path.contains(['\\', '\n', '\r', '\0'])
            && path.split('/').all(|part| {
                !part.is_empty()
                    && part != "."
                    && part != ".."
                    && !part.eq_ignore_ascii_case(".git")
            }),
        "Unsafe repository path '{path}'"
    );
    let mut pattern = String::from("/");
    for ch in path.chars() {
        if matches!(ch, '*' | '?' | '[' | ']' | '!' | '#' | ' ') {
            pattern.push('\\');
        }
        pattern.push(ch);
    }
    pattern.push('/');
    Ok(pattern)
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
            _cache_lock: None,
            revision: None,
            cache_hit: false,
            cached_at: None,
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
    note_remote_git_lock_depth();
    match spec.git_ref.as_deref() {
        Some(git_ref) => {
            git_ops::clone_repo_shallow_at_ref_in_session(&spec.repo_url, &dest, git_ref, session)
        }
        None => git_ops::clone_repo_shallow_in_session(&spec.repo_url, &dest, session),
    }
    .with_context(|| format!("Failed to fetch '{}'", spec.repo_url))?;
    Ok(Checkout {
        dir: dest,
        _temp: Some(temp),
        _cache_lock: None,
        revision: None,
        cache_hit: false,
        cached_at: None,
    })
}

/// Test seam: record the update-transaction depth at a remote git call.
/// Production is a no-op. Graduation, channel install, and channel update
/// assert this is `0`.
fn note_remote_git_lock_depth() {
    #[cfg(test)]
    if let Ok(probe) = std::env::var(REMOTE_GIT_LOCK_PROBE) {
        let depth = crate::skill_update::transaction::transaction_depth_for_test();
        let _ = std::fs::write(probe, depth.to_string());
    }
}

#[cfg(test)]
pub(crate) const REMOTE_GIT_LOCK_PROBE: &str = "SKILLSTAR_TEST_REMOTE_GIT_LOCK_PROBE";

#[cfg(test)]
pub(crate) struct RemoteGitLockProbe {
    path: std::path::PathBuf,
    previous: Option<std::ffi::OsString>,
}

#[cfg(test)]
impl RemoteGitLockProbe {
    pub(crate) fn arm(path: impl Into<std::path::PathBuf>) -> Self {
        let path = path.into();
        let previous = std::env::var_os(REMOTE_GIT_LOCK_PROBE);
        unsafe {
            std::env::set_var(REMOTE_GIT_LOCK_PROBE, &path);
        }
        Self { path, previous }
    }

    pub(crate) fn recorded_depth(&self) -> Option<String> {
        std::fs::read_to_string(&self.path).ok()
    }
}

#[cfg(test)]
impl Drop for RemoteGitLockProbe {
    fn drop(&mut self) {
        unsafe {
            match self.previous.take() {
                Some(value) => std::env::set_var(REMOTE_GIT_LOCK_PROBE, value),
                None => std::env::remove_var(REMOTE_GIT_LOCK_PROBE),
            }
        }
    }
}

/// A `file://` source's real directory, when the URL points at one.
pub fn local_source_dir(repo_url: &str) -> Option<PathBuf> {
    let rest = repo_url.strip_prefix("file://")?;
    let rest = rest.strip_prefix("//").unwrap_or(rest);
    let absolute =
        (cfg!(windows) && rest.len() >= 2 && rest.as_bytes()[1] == b':') || rest.starts_with('/');
    if !absolute {
        return None;
    }
    let path = PathBuf::from(rest);
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
        let status = ss_core::infra::path_env::command_with_path("git")
            .args(["init", "--quiet"])
            .current_dir(dir.path())
            .status()
            .unwrap();
        assert!(status.success());
        let add = ss_core::infra::path_env::command_with_path("git")
            .args(["add", "."])
            .current_dir(dir.path())
            .status()
            .unwrap();
        assert!(add.success());
        let commit = ss_core::infra::path_env::command_with_path("git")
            .args([
                "commit",
                "--quiet",
                "--allow-empty",
                "-m",
                "init",
                "--no-gpg-sign",
            ])
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

    #[test]
    fn sparse_fetch_only_reads_manifests_then_selected_payload_at_each_ref() {
        let _sandbox = crate::pack_fixture::Sandbox::new();
        let fixture = crate::pack_fixture::impeccable_like();
        let remote = "https://github.com/test/sparse.git";
        _sandbox.map_github_url(remote, fixture.dir.path());
        let sha = head_commit(fixture.dir.path());
        let unrelated = git_ops::rev_parse(fixture.dir.path(), "HEAD:README.md").unwrap();
        let folder = ".claude/skills/impeccable";
        let selected_payload = git_ops::rev_parse(
            fixture.dir.path(),
            &format!("HEAD:{folder}/scripts/impeccable"),
        )
        .unwrap();
        let has_blob = |repo: &Path, blob: &str| {
            ss_core::infra::path_env::command_with_path("git")
                .current_dir(repo)
                .env("GIT_NO_LAZY_FETCH", "1")
                .args(["cat-file", "-e", blob])
                .output()
                .unwrap()
                .status
                .success()
        };
        for git_ref in [None, Some("main"), Some(sha.as_str())] {
            let mut spec = Source::parse(remote).unwrap();
            spec.git_ref = git_ref.map(str::to_string);
            let checkout = fetch_for_scan(&spec, &session()).unwrap();
            let path = checkout.dir().to_path_buf();
            assert!(path.join(format!("{folder}/SKILL.md")).is_file());
            assert!(path.join(".claude-plugin/plugin.json").is_file());
            assert!(!path.join("README.md").exists());
            assert!(
                !has_blob(&path, &unrelated),
                "root payload must stay remote"
            );
            assert!(
                !has_blob(&path, &selected_payload),
                "scan must not fetch skill payload"
            );
            for full_depth in [false, true] {
                let identities = |dir: &Path| {
                    crate::discovery::discover_skills(dir, full_depth)
                        .into_iter()
                        .map(|s| (s.id, s.folder_path))
                        .collect::<Vec<_>>()
                };
                assert_eq!(identities(&path), identities(fixture.dir.path()));
            }
            checkout.materialize(&[folder], &session()).unwrap();
            assert!(path.join(format!("{folder}/scripts/impeccable")).is_file());
            assert!(path.join(format!("{folder}/reference/craft.md")).is_file());
            assert!(!path.join(".agents/skills/impeccable/SKILL.md").exists());
            assert!(!has_blob(&path, &unrelated));
            assert_eq!(head_commit(&path), sha);
            drop(checkout);
            assert!(path.exists(), "imports survive the scan operation");
        }
    }

    #[test]
    fn scoped_scan_and_direct_install_do_not_fetch_sibling_skills() {
        let _sandbox = crate::pack_fixture::Sandbox::new();
        let fixture = crate::pack_fixture::impeccable_like();
        let remote = "https://github.com/test/sparse.git";
        _sandbox.map_github_url(remote, fixture.dir.path());
        let mut spec = Source::parse(remote).unwrap();
        let folder = ".claude/skills/impeccable";
        spec.subpath = Some(folder.to_string());
        let checkout = fetch_for_scan(&spec, &session()).unwrap();
        assert!(checkout.dir().join(format!("{folder}/SKILL.md")).is_file());
        assert!(!checkout.dir().join(".agents").exists());
        assert!(crate::plugin_manifest::plugin_hint_for_repo(checkout.dir()).is_some());
        drop(checkout);
        let checkout = fetch_for_install(&spec, &[folder], &session()).unwrap();
        assert!(
            checkout
                .dir()
                .join(format!("{folder}/scripts/impeccable"))
                .is_file()
        );
        assert!(!checkout.dir().join(".agents").exists());
        assert!(!checkout.dir().join("README.md").exists());
    }

    #[test]
    fn sparse_paths_are_literal_and_cannot_escape_scope() {
        for path in [
            "../foo",
            "/foo",
            "a/../b",
            "a/./b",
            "a//b",
            "a\\b",
            "a\nb",
            ".git/config",
        ] {
            assert!(literal_pattern_path(path).is_err(), "{path}");
        }
        assert_eq!(
            literal_pattern_path("skills/[a]* ?").unwrap(),
            "/skills/\\[a\\]\\*\\ \\?/"
        );
        assert!(folder_patterns(&[]).is_err());
    }

    #[test]
    fn literal_folder_and_root_installs_work_without_server_filter_support() {
        let _sandbox = crate::pack_fixture::Sandbox::new();
        let fixture = crate::pack_fixture::impeccable_like();
        let folder = "skills/[demo] name";
        let dir = fixture.dir.path().join(folder);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("SKILL.md"),
            "---\nname: demo\ndescription: d\n---\n",
        )
        .unwrap();
        std::fs::write(dir.join("payload.txt"), "payload").unwrap();
        std::fs::write(
            fixture.dir.path().join("SKILL.md"),
            "---\nname: root\ndescription: d\n---\n",
        )
        .unwrap();
        crate::pack_fixture::git(fixture.dir.path(), &["add", "."]);
        crate::pack_fixture::git(fixture.dir.path(), &["commit", "-m", "extra skills"]);
        crate::pack_fixture::git(
            fixture.dir.path(),
            &["config", "uploadpack.allowFilter", "false"],
        );
        let remote = "https://github.com/test/sparse.git";
        _sandbox.map_github_url(remote, fixture.dir.path());
        let spec = Source::parse(remote).unwrap();
        let checkout = fetch_for_install(&spec, &[folder], &session()).unwrap();
        assert!(
            checkout
                .dir()
                .join(format!("{folder}/payload.txt"))
                .is_file()
        );
        assert!(!checkout.dir().join("README.md").exists());
        assert!(!checkout.dir().join(".agents").exists());
        drop(checkout);
        let checkout = fetch_for_scan(&spec, &session()).unwrap();
        let found = crate::discovery::discover_skills(checkout.dir(), false);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].id, "root");
        checkout.materialize(&[""], &session()).unwrap();
        assert!(checkout.dir().join("README.md").is_file());
        assert!(
            checkout
                .dir()
                .join(format!("{folder}/payload.txt"))
                .is_file()
        );
    }
    #[cfg(unix)]
    #[test]
    fn selected_skill_keeps_payload_linked_from_elsewhere_in_the_repository() {
        let _sandbox = crate::pack_fixture::Sandbox::new();
        let fixture = crate::pack_fixture::impeccable_like();
        let folder = ".claude/skills/impeccable";
        std::os::unix::fs::symlink(
            "../../../README.md",
            fixture.dir.path().join(folder).join("linked.md"),
        )
        .unwrap();
        crate::pack_fixture::git(fixture.dir.path(), &["add", "."]);
        crate::pack_fixture::git(fixture.dir.path(), &["commit", "-m", "linked payload"]);
        let remote = "https://github.com/test/sparse.git";
        _sandbox.map_github_url(remote, fixture.dir.path());
        let spec = Source::parse(remote).unwrap();
        let expected = std::fs::read(fixture.dir.path().join("README.md")).unwrap();
        let checkout = fetch_for_install(&spec, &[folder], &session()).unwrap();
        assert_eq!(
            std::fs::read(checkout.dir().join(folder).join("linked.md")).unwrap(),
            expected
        );
        drop(checkout);
        let checkout = fetch_for_scan(&spec, &session()).unwrap();
        checkout.materialize(&[folder], &session()).unwrap();
        crate::installer::install_units(
            checkout.dir(),
            &spec,
            &[crate::installer::InstallUnit {
                id: "impeccable".to_string(),
                folder_path: folder.to_string(),
            }],
        )
        .unwrap();
        assert_eq!(
            std::fs::read(ss_core::infra::paths::agents_skill_dir("impeccable").join("linked.md"))
                .unwrap(),
            expected
        );
    }
}
