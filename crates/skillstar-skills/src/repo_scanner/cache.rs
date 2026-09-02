use crate::git::ops as git_ops;
use anyhow::{Context, Result};
use skillstar_core::infra::{path_env::command_with_path, paths};
use std::path::{Path, PathBuf};
use tracing::warn;

pub use crate::source_resolver::cache_dir_name;

pub fn clone_or_fetch_repo_in_session(
    repo_url: &str,
    source: &str,
    session: &crate::git::transport::GitOperationSession,
) -> Result<PathBuf> {
    clone_or_fetch_repo_at_in_session(repo_url, source, None, session)
}

/// Cache directory key used by [`clone_or_fetch_repo_at_in_session`].
pub fn cache_key_for(source: &str, git_ref: Option<&str>) -> Result<String> {
    match git_ref {
        Some(git_ref) => {
            validate_git_ref(git_ref)?;
            Ok(format!(
                "{}--ref--{}",
                cache_dir_name(source),
                cache_dir_name(git_ref)
            ))
        }
        None => Ok(cache_dir_name(source)),
    }
}

/// Existing repo-cache checkout, if a previous install already cloned it.
///
/// Does not fetch, reset, or create directories. Callers that only need to
/// retarget or deploy from a hub-installed pack should use this instead of
/// [`clone_or_fetch_repo_at_in_session`].
pub fn cached_repo_dir_if_present(source: &str, git_ref: Option<&str>) -> Option<PathBuf> {
    let cache_key = cache_key_for(source, git_ref).ok()?;
    let repo_dir = paths::repos_cache_dir().join(cache_key);
    repo_dir.join(".git").exists().then_some(repo_dir)
}

/// Local checkout for a source, including leftover `--ref--<branch>` caches.
///
/// A lock that still names a deleted PR branch keeps the clone under
/// `{source}--ref--{branch}`. Carousel / cache-local install must find that
/// directory even when the URL no longer carries a ref.
pub fn existing_repo_cache_dir(source: &str, git_ref: Option<&str>) -> Option<PathBuf> {
    if let Some(repo_dir) = cached_repo_dir_if_present(source, git_ref) {
        return Some(repo_dir);
    }
    if git_ref.is_some()
        && let Some(repo_dir) = cached_repo_dir_if_present(source, None)
    {
        return Some(repo_dir);
    }
    find_ref_variant_caches(source).into_iter().next()
}

/// Hub-linked checkout for an already-installed skill from `repo_url`.
///
/// The hub symlink is the source of truth: origin may be poisoned or still
/// pinned to a deleted ref, but the files are already local.
pub fn existing_hub_checkout(repo_url: &str, skill_name: Option<&str>) -> Option<PathBuf> {
    let hub = paths::hub_skills_dir();
    if let Some(name) = skill_name.filter(|name| !name.is_empty())
        && let Some(root) = crate::repo_link::repo_root_of(&hub.join(name))
    {
        return Some(root);
    }
    let lock = crate::lockfile::Lockfile::load(&crate::lockfile::lockfile_path()).ok()?;
    for entry in lock.skills {
        if !crate::source_resolver::same_remote_url(&entry.git_url, repo_url) {
            continue;
        }
        if let Some(root) = crate::repo_link::repo_root_of(&hub.join(&entry.name)) {
            return Some(root);
        }
    }
    None
}

fn find_ref_variant_caches(source: &str) -> Vec<PathBuf> {
    let Ok(prefix) = cache_key_for(source, None) else {
        return Vec::new();
    };
    let Ok(entries) = std::fs::read_dir(paths::repos_cache_dir()) else {
        return Vec::new();
    };
    let pin_prefix = format!("{prefix}--ref--");
    let mut matches: Vec<PathBuf> = entries
        .flatten()
        .filter(|entry| {
            let name = entry.file_name();
            let Some(name) = name.to_str() else {
                return false;
            };
            (name == prefix || name.starts_with(&pin_prefix)) && entry.path().join(".git").exists()
        })
        .map(|entry| entry.path())
        .collect();
    matches.sort();
    matches
}

/// True when the checkout still has at least one `SKILL.md` to install from.
pub(crate) fn checkout_has_skill_payload(repo_dir: &Path) -> bool {
    if !repo_dir.join(".git").exists() {
        return false;
    }
    crate::discovery::discover_skills_without_dedup(repo_dir, true, None)
        .iter()
        .any(|skill| {
            let manifest = if skill.folder_path.is_empty() {
                repo_dir.join("SKILL.md")
            } else {
                repo_dir.join(&skill.folder_path).join("SKILL.md")
            };
            manifest.is_file()
        })
}

pub fn clone_or_fetch_repo_at_in_session(
    repo_url: &str,
    source: &str,
    git_ref: Option<&str>,
    session: &crate::git::transport::GitOperationSession,
) -> Result<PathBuf> {
    let cache_dir = paths::repos_cache_dir();
    std::fs::create_dir_all(&cache_dir).context("Failed to create repo cache directory")?;

    let cache_key = cache_key_for(source, git_ref)?;
    let repo_dir = cache_dir.join(cache_key);

    if repo_dir.join(".git").exists() {
        ensure_installed_checkout_is_clean(&repo_dir)?;
        if let Some(git_ref) = git_ref {
            fetch_and_reset_ref(&repo_dir, git_ref, session)?;
            return Ok(repo_dir);
        }

        git_ops::run_git_shallow_fetch_in_session(
            &repo_dir,
            &["fetch", "--depth", "1", "--quiet"],
            session,
        )
        .context("Failed to execute git fetch")?;

        // The fetch is a network operation lasting up to seconds; a user edit
        // landing in that window must not be destroyed by the reset below.
        // Fail closed instead — the update caller preserves the worktree.
        if !git_ops::worktree_is_clean(&repo_dir)? {
            return Err(git_ops::WorktreeDirty.into());
        }

        git_ops::checkout_in_session(&repo_dir, &["reset", "--hard", "origin/HEAD"], session)
            .context("Failed to reset cached repository")?;

        // The reset just moved the files of every other Skill installed from
        // this checkout. `ensure_installed_checkout_is_clean` and
        // `worktree_is_clean` above proved none of them had user edits, so the
        // new content is upstream's and their baselines must follow it —
        // otherwise they look locally diverged forever and lock this repository
        // out of further installs, scans and updates.
        crate::skill_update::refresh_baselines_after_checkout_reset(&repo_dir)
            .context("Failed to refresh content baselines after resetting the cached repository")?;

        if is_sparse_checkout(&repo_dir)
            && let Ok(dirs) = discover_skill_dirs_from_tree(&repo_dir)
        {
            if dirs.is_empty() {
                let _ = git_ops::checkout_in_session(
                    &repo_dir,
                    &["sparse-checkout", "disable"],
                    session,
                );
                let _ = git_ops::checkout_in_session(&repo_dir, &["checkout"], session);
            } else {
                let dir_refs: Vec<&str> = dirs.iter().map(|s| s.as_str()).collect();
                let _ = git_ops::apply_sparse_checkout_in_session(&repo_dir, &dir_refs, session);
            }
        }

        Ok(repo_dir)
    } else if let Some(git_ref) = git_ref {
        // Ref-pinned sources use an isolated cache entry. A shallow worktree is
        // intentionally preferred here: after fetching an arbitrary ref we
        // need its files available before applying any subpath filter.
        git_ops::clone_repo_shallow_in_session(repo_url, &repo_dir, session)
            .with_context(|| format!("Failed to shallow-clone {}", repo_url))?;
        fetch_and_reset_ref(&repo_dir, git_ref, session)?;
        Ok(repo_dir)
    } else {
        match clone_sparse_with_skills(repo_url, &repo_dir, session) {
            Ok(()) => Ok(repo_dir),
            Err(sparse_err) => {
                warn!(target: "repo_scanner", error = %sparse_err, "sparse clone failed, falling back to shallow");
                let _ = std::fs::remove_dir_all(&repo_dir);
                git_ops::clone_repo_shallow_in_session(repo_url, &repo_dir, session)
                    .with_context(|| format!("Failed to shallow-clone {}", repo_url))?;
                Ok(repo_dir)
            }
        }
    }
}

fn ensure_installed_checkout_is_clean(repo_dir: &Path) -> Result<()> {
    let canonical_repo = std::fs::canonicalize(repo_dir)
        .with_context(|| format!("Failed to resolve repo cache '{}'", repo_dir.display()))?;
    let lockfile = crate::lockfile::Lockfile::load(&crate::lockfile::lockfile_path())
        .context("Failed to read installed Skill baselines before fetching repository")?;
    let hub = paths::hub_skills_dir();
    if !hub.exists() {
        return Ok(());
    }
    for hub_entry in std::fs::read_dir(&hub).with_context(|| {
        format!(
            "Failed to enumerate installed Skills in '{}'",
            hub.display()
        )
    })? {
        let skill_path = hub_entry
            .context("Failed to inspect an installed Skill entry")?
            .path();
        // Staging residue from an interrupted hub write is a symlink into this
        // same repo cache, so it would otherwise pass the ownership check below
        // and then fail the lock-entry check — permanently blocking every
        // install, scan and update for this repository over a hidden path.
        if !skill_path
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(crate::hub_entry::is_managed_hub_entry)
        {
            continue;
        }
        let belongs_to_checkout = crate::repo_link::repo_root_of(&skill_path)
            .and_then(|root| std::fs::canonicalize(root).ok())
            .is_some_and(|root| root == canonical_repo);
        if !belongs_to_checkout {
            continue;
        }
        let name = skill_path
            .file_name()
            .and_then(|name| name.to_str())
            .context("Installed Skill name is not valid UTF-8")?;
        let entry = lockfile
            .skills
            .iter()
            .find(|entry| {
                if cfg!(windows) {
                    entry.name.eq_ignore_ascii_case(name)
                } else {
                    entry.name == name
                }
            })
            .with_context(|| {
                format!(
                    "Skill '{name}' has no trusted lock entry; resolve it before refreshing this repository"
                )
            })?;
        let Some(baseline) = entry.content_hash.as_deref() else {
            anyhow::bail!(
                "Skill '{}' has no trusted content baseline; resolve it before refreshing this repository",
                entry.name
            );
        };
        if entry.content_hash_version != Some(crate::content::SNAPSHOT_HASH_VERSION) {
            anyhow::bail!(
                "Skill '{}' uses an unsupported content baseline; resolve it before refreshing this repository",
                entry.name
            );
        }
        let current = crate::content::snapshot(name)
            .with_context(|| format!("Failed to inspect local changes for '{name}'"))?;
        if current.content_hash != baseline {
            anyhow::bail!(
                "Skill '{}' has local changes; preserve them as a local copy or explicitly discard them before refreshing this repository",
                entry.name
            );
        }
    }
    Ok(())
}

fn validate_git_ref(git_ref: &str) -> Result<()> {
    let invalid = git_ref.is_empty()
        || git_ref.starts_with('-')
        || git_ref.ends_with('/')
        || git_ref.ends_with('.')
        || git_ref.ends_with(".lock")
        || git_ref.contains("..")
        || git_ref.contains("//")
        || git_ref.contains("@{")
        || git_ref
            .chars()
            .any(|ch| ch.is_control() || ch.is_whitespace() || "~^:?*[\\".contains(ch));
    if invalid {
        anyhow::bail!("Unsafe or invalid Git ref: {git_ref}");
    }
    Ok(())
}

fn fetch_and_reset_ref(
    repo_dir: &Path,
    git_ref: &str,
    session: &crate::git::transport::GitOperationSession,
) -> Result<()> {
    validate_git_ref(git_ref)?;

    let fetch = git_ops::run_git_shallow_fetch_in_session(
        repo_dir,
        &["fetch", "--depth", "1", "--quiet", "origin", git_ref],
        session,
    );
    match fetch {
        Ok(_) => {}
        Err(error)
            if git_ops::is_missing_remote_ref(error.as_ref())
                && checkout_has_skill_payload(repo_dir) =>
        {
            warn!(
                target: "repo_scanner",
                path = %repo_dir.display(),
                git_ref,
                "recorded git ref is gone — using existing cache and retargeting lock"
            );
            crate::update_checker::retarget_deleted_git_ref(repo_dir).with_context(|| {
                format!("failed to retarget lock after missing remote ref '{git_ref}'")
            })?;
            return Ok(());
        }
        Err(error) => {
            return Err(error).with_context(|| format!("git fetch for ref '{git_ref}' failed"));
        }
    }

    // Fetch is a network operation (up to seconds) — a user edit landing in
    // that window must not be destroyed by the reset below. Fail closed and
    // let the update caller preserve the worktree.
    if !git_ops::worktree_is_clean(repo_dir)? {
        return Err(git_ops::WorktreeDirty.into());
    }

    git_ops::checkout_in_session(repo_dir, &["reset", "--hard", "FETCH_HEAD"], session)
        .with_context(|| format!("git checkout for ref '{git_ref}' failed"))?;

    // Same reason as the `origin/HEAD` reset: Skills sharing this checkout just
    // had their files moved and must not be left looking locally diverged. A
    // no-op for the fresh-clone caller, which has no installed Skills yet.
    crate::skill_update::refresh_baselines_after_checkout_reset(repo_dir).with_context(|| {
        format!("failed to refresh content baselines after resetting to ref '{git_ref}'")
    })?;

    let output = command_with_path("git")
        .current_dir(repo_dir)
        .args(["config", "skillstar.ref", git_ref])
        .output()
        .context("Failed to persist requested Git ref in repo cache")?;
    if !output.status.success() {
        let err = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("failed to persist Git ref '{git_ref}': {}", err.trim());
    }
    Ok(())
}

fn clone_sparse_with_skills(
    repo_url: &str,
    dest: &Path,
    session: &crate::git::transport::GitOperationSession,
) -> Result<()> {
    git_ops::clone_repo_sparse_in_session(repo_url, dest, session)?;

    let skill_dirs = discover_skill_dirs_from_tree(dest)?;

    if skill_dirs.is_empty() {
        let _ = git_ops::checkout_in_session(dest, &["sparse-checkout", "disable"], session);
        let _ = git_ops::checkout_in_session(dest, &["checkout"], session);
        return Ok(());
    }

    let dir_refs: Vec<&str> = skill_dirs.iter().map(|s| s.as_str()).collect();
    git_ops::apply_sparse_checkout_in_session(dest, &dir_refs, session)?;

    Ok(())
}

pub(super) fn discover_skill_dirs_from_tree(repo_dir: &Path) -> Result<Vec<String>> {
    let all_paths = git_ops::list_tree_paths(repo_dir)?;
    Ok(derive_sparse_skill_dirs(&all_paths))
}

fn derive_sparse_skill_dirs(all_paths: &[String]) -> Vec<String> {
    // Keep every nested SKILL.md parent. Deduping by basename dropped
    // `.agents/skills/impeccable` when `.agent/skills/impeccable` was also
    // present (equal source_priority, tree order kept `.agent`). A root
    // SKILL.md used to force a full checkout; if nested copies exist, the
    // root file is a shim — materialize the nested folders instead.
    let mut skill_dirs: Vec<String> = all_paths
        .iter()
        .filter(|p| p.ends_with("/SKILL.md") || *p == "SKILL.md")
        .filter_map(|p| {
            let parent = Path::new(p).parent()?;
            let parent_str = parent.to_string_lossy().to_string();
            if parent_str.is_empty() {
                None
            } else {
                Some(parent_str)
            }
        })
        .collect();

    if skill_dirs.is_empty() {
        return Vec::new();
    }

    skill_dirs.sort();
    skill_dirs.dedup();
    compact_to_common_parents(&skill_dirs)
}

fn compact_to_common_parents(dirs: &[String]) -> Vec<String> {
    if dirs.is_empty() {
        return Vec::new();
    }

    let mut parent_counts: std::collections::HashMap<String, usize> =
        std::collections::HashMap::new();
    let mut parent_to_dirs: std::collections::HashMap<String, Vec<String>> =
        std::collections::HashMap::new();

    for dir in dirs {
        if let Some(parent) = Path::new(dir).parent() {
            let parent_str = parent.to_string_lossy().to_string();
            *parent_counts.entry(parent_str.clone()).or_insert(0) += 1;
            parent_to_dirs
                .entry(parent_str)
                .or_default()
                .push(dir.clone());
        }
    }

    let mut result = Vec::new();
    let mut handled = std::collections::HashSet::new();

    for dir in dirs {
        if handled.contains(dir) {
            continue;
        }
        if let Some(parent) = Path::new(dir).parent() {
            let parent_str = parent.to_string_lossy().to_string();
            if parent_counts.get(&parent_str).copied().unwrap_or(0) >= 2 {
                if !handled.contains(&parent_str) {
                    result.push(parent_str.clone());
                    if let Some(children) = parent_to_dirs.get(&parent_str) {
                        for child in children {
                            handled.insert(child.clone());
                        }
                    }
                    handled.insert(parent_str);
                }
            } else {
                result.push(dir.clone());
                handled.insert(dir.clone());
            }
        } else {
            result.push(dir.clone());
            handled.insert(dir.clone());
        }
    }

    result.sort();
    result
}

pub(super) fn is_sparse_checkout(repo_dir: &Path) -> bool {
    let output = command_with_path("git")
        .current_dir(repo_dir)
        .args(["config", "--get", "core.sparseCheckout"])
        .output();
    match output {
        Ok(o) => String::from_utf8_lossy(&o.stdout).trim() == "true",
        Err(_) => false,
    }
}

#[cfg(test)]
mod tests {
    use super::{
        cache_dir_name, clone_or_fetch_repo_in_session, compact_to_common_parents,
        ensure_installed_checkout_is_clean,
    };
    use crate::git::transport::{GitAuthMaterial, GitOperationSession, NoopGitProgressSink};
    use std::sync::Arc;

    #[test]
    fn generic_scan_never_resets_a_linked_skill_without_a_lock_entry() {
        let _guard = crate::lock_test_env();
        let temp = tempfile::tempdir().unwrap();
        let previous_data = std::env::var_os("SKILLSTAR_DATA_DIR");
        let previous_hub = std::env::var_os("SKILLSTAR_HUB_DIR");
        unsafe {
            std::env::set_var("SKILLSTAR_DATA_DIR", temp.path().join("data"));
            std::env::set_var("SKILLSTAR_HUB_DIR", temp.path().join("hub"));
        }

        let result = (|| -> anyhow::Result<()> {
            let source_url = "https://github.com/acme/channel.git";
            let repo =
                skillstar_core::infra::paths::repos_cache_dir().join(cache_dir_name(source_url));
            let source = repo.join("skills/writer");
            std::fs::create_dir_all(&source)?;
            std::fs::write(source.join("SKILL.md"), "# locally edited\n")?;
            let status = skillstar_core::infra::path_env::command_with_path("git")
                .args(["init", "-q"])
                .current_dir(&repo)
                .status()?;
            assert!(status.success());
            let hub = skillstar_core::infra::paths::hub_skills_dir();
            std::fs::create_dir_all(&hub)?;
            skillstar_core::infra::fs_ops::create_symlink(&source, &hub.join("writer"))?;
            let session = GitOperationSession::new(
                "missing-lock-scan",
                GitAuthMaterial::missing(),
                Arc::new(NoopGitProgressSink),
            );

            let error = clone_or_fetch_repo_in_session(source_url, source_url, &session)
                .expect_err("a linked Skill without a lock entry must stop before fetch/reset");

            assert!(error.to_string().contains("no trusted lock entry"));
            assert_eq!(
                std::fs::read_to_string(source.join("SKILL.md"))?,
                "# locally edited\n"
            );
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

    #[test]
    fn installed_checkout_with_local_changes_is_never_reset_by_a_refresh() {
        let _guard = crate::lock_test_env();
        let temp = tempfile::tempdir().unwrap();
        let previous_data = std::env::var_os("SKILLSTAR_DATA_DIR");
        let previous_hub = std::env::var_os("SKILLSTAR_HUB_DIR");
        unsafe {
            std::env::set_var("SKILLSTAR_DATA_DIR", temp.path().join("data"));
            std::env::set_var("SKILLSTAR_HUB_DIR", temp.path().join("hub"));
        }

        let result = (|| -> anyhow::Result<()> {
            let repo = skillstar_core::infra::paths::repos_cache_dir().join("acme--channel");
            let source = repo.join("skills/writer");
            std::fs::create_dir_all(&source)?;
            std::fs::write(source.join("SKILL.md"), "# clean\n")?;
            let status = skillstar_core::infra::path_env::command_with_path("git")
                .args(["init", "-q"])
                .current_dir(&repo)
                .status()?;
            assert!(status.success());
            let hub = skillstar_core::infra::paths::hub_skills_dir();
            std::fs::create_dir_all(&hub)?;
            skillstar_core::infra::fs_ops::create_symlink(&source, &hub.join("writer"))?;
            let baseline = crate::content::snapshot("writer")?.content_hash;
            let mut lockfile = crate::lockfile::Lockfile::default();
            lockfile.upsert(crate::lockfile::LockEntry {
                name: "writer".into(),
                git_url: "https://github.com/acme/channel.git".into(),
                git_ref: None,
                tree_hash: "tree".into(),
                content_hash: Some(baseline),
                content_hash_version: Some(crate::content::SNAPSHOT_HASH_VERSION),
                installed_at: chrono::Utc::now().to_rfc3339(),
                source_folder: Some("skills/writer".into()),
            });
            lockfile.save(&crate::lockfile::lockfile_path())?;

            std::fs::write(source.join("SKILL.md"), "# locally edited\n")?;
            let error = ensure_installed_checkout_is_clean(&repo).unwrap_err();
            assert!(error.to_string().contains("local changes"));
            assert_eq!(
                std::fs::read_to_string(source.join("SKILL.md"))?,
                "# locally edited\n"
            );
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

    #[test]
    fn compact_parents_groups_siblings() {
        let dirs = vec![
            "source/skills/adapt".to_string(),
            "source/skills/animate".to_string(),
            "source/skills/bolder".to_string(),
        ];
        let compacted = compact_to_common_parents(&dirs);
        assert_eq!(compacted, vec!["source/skills"]);
    }

    #[test]
    fn compact_parents_preserves_singles() {
        let dirs = vec!["custom/my-skill".to_string()];
        let compacted = compact_to_common_parents(&dirs);
        assert_eq!(compacted, vec!["custom/my-skill"]);
    }

    #[test]
    fn compact_parents_mixed() {
        let dirs = vec![
            "custom/lone-skill".to_string(),
            "source/skills/adapt".to_string(),
            "source/skills/animate".to_string(),
        ];
        let compacted = compact_to_common_parents(&dirs);
        assert_eq!(compacted, vec!["custom/lone-skill", "source/skills"]);
    }

    #[test]
    fn compact_parents_empty() {
        let dirs: Vec<String> = Vec::new();
        let compacted = compact_to_common_parents(&dirs);
        assert!(compacted.is_empty());
    }

    #[test]
    fn sparse_keeps_agent_and_agents_copies() {
        let dirs = super::derive_sparse_skill_dirs(&[
            ".agent/skills/impeccable/SKILL.md".to_string(),
            ".agents/skills/impeccable/SKILL.md".to_string(),
            ".cursor/skills/impeccable/SKILL.md".to_string(),
        ]);
        assert!(dirs.iter().any(|d| d.contains(".agent")), "{dirs:?}");
        assert!(dirs.iter().any(|d| d.contains(".agents")), "{dirs:?}");
        assert!(dirs.iter().any(|d| d.contains(".cursor")), "{dirs:?}");
    }

    #[test]
    fn sparse_ignores_root_shim_and_keeps_nested_harness_folders() {
        let dirs = super::derive_sparse_skill_dirs(&[
            "SKILL.md".to_string(),
            ".cursor/skills/rust/SKILL.md".to_string(),
            ".dsh/skills/rust/SKILL.md".to_string(),
            "skills/rust/SKILL.md".to_string(),
        ]);
        assert!(
            !dirs.is_empty(),
            "root SKILL.md must not force a full checkout when nested copies exist"
        );
        assert!(
            dirs.iter()
                .any(|d| d.contains("skills") || d.contains(".cursor") || d.contains(".dsh")),
            "{dirs:?}"
        );
    }

    #[test]
    fn sparse_root_only_skill_still_full_checkouts() {
        let dirs =
            super::derive_sparse_skill_dirs(&["SKILL.md".to_string(), "README.md".to_string()]);
        assert!(dirs.is_empty());
    }
}
