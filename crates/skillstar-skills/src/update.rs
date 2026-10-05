//! vercel-labs/skills update semantics (D-081).
//!
//! Check = compare each lock entry's `skill_folder_hash` (a git tree SHA)
//! against the upstream tree, grouped by `(source_url, git_ref)` so skills on
//! different refs never compare against the wrong tree. Apply = fetch the
//! source into a temp checkout and overwrite-reinstall — local edits are not
//! detected or preserved (`npx skills update` parity).

use std::collections::BTreeMap;
use std::path::Path;



use crate::fetch;
use crate::git::transport::GitOperationSession;
use crate::installer::{self, InstallUnit};
use crate::skill_lock::{self, SkillLockEntry};
use crate::source_resolver::Source;

/// Upstream verdict for one locked skill.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Upstream {
    /// Upstream folder tree SHA (compare against the lock to decide).
    Hash(String),
    /// The locked `skill_path` no longer exists upstream.
    Removed,
    /// Could not determine (network/API failure) — keep the previous badge.
    Unknown,
}

/// Outcome of applying one update.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppliedUpdate {
    pub name: String,
    pub result: UpdateResult,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UpdateResult {
    /// Reinstalled from upstream; carries the new folder hash.
    Updated { folder_hash: Option<String> },
    /// Upstream no longer contains the skill.
    Removed,
    /// Entry missing from the lock or source unparsable.
    Failed(String),
}

/// Check upstream state for the given lock entries.
///
/// `token` is the SkillStar GitHub App token when signed in (`None` uses the
/// anonymous budget). GitHub sources try the Trees API first and fall back to
/// a temp shallow clone on any failure; non-GitHub sources always clone.
pub async fn check_upstream(
    entries: &[(String, SkillLockEntry)],
    token: Option<&str>,
) -> BTreeMap<String, Upstream> {
    let mut verdicts = BTreeMap::new();
    let groups = skill_lock::SkillLock::by_source_group(entries);

    for ((source_url, git_ref), group) in groups {
        // Local and bundle sources never participate in update checks.
        if group.iter().all(|(_, entry)| {
            matches!(entry.source_type, skill_lock::SourceType::Local | skill_lock::SourceType::Bundle)
        }) {
            for (name, _) in group {
                verdicts.insert(name, Upstream::Unknown);
            }
            continue;
        }

        let mut from_api: Option<BTreeMap<String, Option<String>>> = None;
        if let Some((owner, repo)) = crate::update_api::owner_repo_from_git_url(&source_url) {
            // vercel parity: no recorded ref means try `main`, then `master`.
            let api_refs: Vec<String> = match git_ref.clone() {
                Some(r) => vec![r],
                None => vec!["main".to_string(), "master".to_string()],
            };
            for api_ref in api_refs {
                match crate::update_api::fetch_remote_subtree_hashes(
                    &owner,
                    &repo,
                    &api_ref,
                    token,
                )
                .await
                {
                    Ok(tree) => {
                        let mut map = BTreeMap::new();
                        for (name, entry) in &group {
                            let path = entry.skill_path.clone().unwrap_or_default();
                            map.insert(name.clone(), subtree_from_api(&tree, &path));
                        }
                        from_api = Some(map);
                        break;
                    }
                    Err(_) => continue,
                }
            }
        }

        match from_api {
            Some(map) => {
                for (name, hash) in map {
                    verdicts.insert(
                        name,
                        match hash {
                            Some(hash) => Upstream::Hash(hash),
                            None => Upstream::Removed,
                        },
                    );
                }
            }
            None => {
                // Clone fallback (also the only path for non-GitHub sources).
                let Ok(mut spec) = Source::parse(&source_url) else {
                    for (name, _) in group {
                        verdicts.insert(name, Upstream::Unknown);
                    }
                    continue;
                };
                spec.git_ref = git_ref.clone();
                let session = GitOperationSession::public();
                match fetch::fetch_source(&spec, &session) {
                    Ok(checkout) => {
                        for (name, entry) in &group {
                            let path = entry.skill_path.clone().unwrap_or_default();
                            let hash = fetch::folder_tree_hash(
                                checkout.dir(),
                                if path.is_empty() { None } else { Some(&path) },
                            );
                            verdicts.insert(
                                name.clone(),
                                match hash {
                                    Some(hash) => Upstream::Hash(hash),
                                    None => Upstream::Removed,
                                },
                            );
                        }
                    }
                    Err(_) => {
                        for (name, _) in group {
                            verdicts.insert(name, Upstream::Unknown);
                        }
                    }
                }
            }
        }
    }
    verdicts
}

/// The Trees API lists only top-level directories non-recursively; deeper
/// `skill_path`s cannot be resolved from it, so callers fall back to the
/// clone path — modeled here as `None`.
fn subtree_from_api(
    tree: &crate::update_api::ApiRemoteTree,
    skill_path: &str,
) -> Option<String> {
    if skill_path.is_empty() {
        return tree.folders.get("").cloned();
    }
    if skill_path.contains('/') {
        return None;
    }
    tree.folders.get(skill_path).cloned()
}

/// Overwrite-reinstall the named skills from their locked sources.
///
/// One skill per unit of work: a failure is reported for that name only and
/// does not block the rest. Skills whose upstream folder vanished are
/// reported as [`UpdateResult::Removed`] for the UI's remove/convert exits.
pub fn apply_updates(names: &[String], session: &GitOperationSession) -> Vec<AppliedUpdate> {
    let lock = skill_lock::load();
    let mut results = Vec::new();
    for name in names {
        let result = apply_one(name, &lock, session);
        results.push(AppliedUpdate {
            name: name.clone(),
            result,
        });
    }
    results
}

fn apply_one(name: &str, lock: &skill_lock::SkillLock, session: &GitOperationSession) -> UpdateResult {
    let Some(entry) = lock.skills.get(name) else {
        return UpdateResult::Failed(format!("'{name}' is not recorded in the lock"));
    };
    if matches!(
        entry.source_type,
        skill_lock::SourceType::Local | skill_lock::SourceType::Bundle
    ) {
        return UpdateResult::Failed(format!(
            "'{name}' was installed from a local source and cannot be updated from upstream"
        ));
    }
    let spec = match Source::parse(&entry.source_url) {
        Ok(mut spec) => {
            spec.git_ref = entry.git_ref.clone().or(spec.git_ref);
            spec.subpath = None;
            spec.skill_filter = None;
            spec
        }
        Err(error) => return UpdateResult::Failed(error.to_string()),
    };
    let checkout = match fetch::fetch_source(&spec, session) {
        Ok(checkout) => checkout,
        Err(error) => return UpdateResult::Failed(format!("{error:#}")),
    };
    let outcome = reinstall_from_checkout(checkout.dir(), &spec, name, entry);
    if let UpdateResult::Updated { folder_hash } = &outcome {
        let folder_hash = folder_hash.clone();
        let _ = skill_lock::mutate(|lock| {
            if let Some(stored) = lock.skills.get_mut(name) {
                stored.skill_folder_hash = folder_hash.clone();
                stored.updated_at = chrono::Utc::now().to_rfc3339();
            }
        });
    }
    outcome
}

fn reinstall_from_checkout(
    checkout: &Path,
    spec: &Source,
    name: &str,
    entry: &SkillLockEntry,
) -> UpdateResult {
    let folder = entry.skill_path.clone().unwrap_or_default();
    let dir = if folder.is_empty() {
        checkout.to_path_buf()
    } else {
        checkout.join(&folder)
    };
    if !dir.join("SKILL.md").is_file() {
        return UpdateResult::Removed;
    }
    // The skill may have been renamed in frontmatter upstream; keep installing
    // under the locked identity so agent links stay valid (vercel reinstalls
    // by `--skill <name>` discovered identity — a changed identity is surfaced
    // by the next scan, not by update).
    let id = crate::validation::inspect_skill_frontmatter(&dir)
        .name
        .filter(|n| !n.trim().is_empty())
        .unwrap_or_else(|| name.to_string());
    let unit = InstallUnit {
        id,
        folder_path: folder,
    };
    match installer::install_units(checkout, spec, &[unit]) {
        Ok(_) => {
            let path = if entry.skill_path.as_deref().unwrap_or("").is_empty() {
                None
            } else {
                entry.skill_path.as_deref()
            };
            UpdateResult::Updated {
                folder_hash: fetch::folder_tree_hash(checkout, path),
            }
        }
        Err(error) => UpdateResult::Failed(format!("{error:#}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::git::transport::{GitAuthMaterial, NoopGitProgressSink};
    use crate::skill_lock::SourceType;
    use std::sync::Arc;

    fn session() -> GitOperationSession {
        GitOperationSession::new(
            "update-test",
            GitAuthMaterial::missing(),
            Arc::new(NoopGitProgressSink),
        )
    }

    /// A lock entry modeling a git-backed install. Tests drive the clone
    /// fallback with `file://` fixture repos while claiming GitHub provenance,
    /// exactly what a real lock would record.
    fn entry(url: &str, path: &str, hash: Option<String>) -> SkillLockEntry {
        SkillLockEntry {
            source: "o/r".into(),
            source_type: SourceType::Github,
            source_url: url.into(),
            git_ref: None,
            skill_path: if path.is_empty() { None } else { Some(path.into()) },
            skill_folder_hash: hash,
            installed_at: String::new(),
            updated_at: String::new(),
        }
    }

    /// A local git repo standing in for an upstream source, addressable via
    /// `file://` so `fetch_source` can clone it.
    struct UpstreamFixture {
        dir: tempfile::TempDir,
        url: String,
    }

    impl UpstreamFixture {
        fn new() -> Self {
            let dir = tempfile::tempdir().unwrap();
            Self::git(dir.path(), &["init", "--quiet"]);
            Self::commit(dir.path(), "skills/foo", "---\nname: foo\ndescription: v1\n---\n");
            Self { url: git_upstream_url(dir.path()), dir }
        }

        fn bump(&self) {
            Self::commit(self.dir.path(), "skills/foo", "---\nname: foo\ndescription: v2\n---\n");
        }

        fn drop_skill(&self) {
            let remove = skillstar_core::infra::path_env::command_with_path("git")
                .args(["rm", "-rq", "skills/foo"])
                .current_dir(&self.dir)
                .status()
                .unwrap();
            assert!(remove.success());
            Self::git(self.dir.path(), &["commit", "--quiet", "-m", "drop", "--no-gpg-sign"]);
        }

        fn commit(dir: &std::path::Path, rel: &str, skill_md: &str) {
            let target = dir.join(rel);
            std::fs::create_dir_all(&target).unwrap();
            std::fs::write(target.join("SKILL.md"), skill_md).unwrap();
            Self::git(dir, &["add", "."]);
            Self::git(dir, &["commit", "--quiet", "-m", "init", "--no-gpg-sign"]);
        }

        fn git(dir: &std::path::Path, args: &[&str]) {
            let status = skillstar_core::infra::path_env::command_with_path("git")
                .args(args)
                .env("GIT_AUTHOR_NAME", "Test")
                .env("GIT_COMMITTER_NAME", "Test")
                .env("GIT_AUTHOR_EMAIL", "t@t")
                .env("GIT_COMMITTER_EMAIL", "t@t")
                .env("GIT_COMMITTER_DATE", "1759500000 +0000")
                .env("GIT_AUTHOR_DATE", "1759500000 +0000")
                .current_dir(dir)
                .status()
                .unwrap();
            assert!(status.success(), "git {:?} failed", args);
        }
    }

    fn git_upstream_url(dir: &std::path::Path) -> String {
        crate::git::ops::local_file_url(dir)
    }

    #[tokio::test]
    async fn check_reports_changed_and_removed_via_clone_fallback() {
        let _guard = crate::lock_test_env_async();
        let sandbox = tempfile::tempdir().unwrap();
        let previous = std::env::var_os("SKILLSTAR_DATA_DIR");
        unsafe { std::env::set_var("SKILLSTAR_DATA_DIR", sandbox.path().join("data")) };

        let upstream = UpstreamFixture::new();
        // Clone once locally to compute the v1 hash the lock would store.
        let spec = Source::parse(&upstream.url).unwrap();
        let probe = fetch::fetch_source(&spec, &GitOperationSession::public()).unwrap();
        let v1 = fetch::folder_tree_hash(probe.dir(), Some("skills/foo")).unwrap();

        let entries = vec![(
            "foo".to_string(),
            entry(&upstream.url, "skills/foo", Some(v1.clone())),
        )];
        let verdicts = check_upstream(&entries, None).await;
        assert_eq!(verdicts["foo"], Upstream::Hash(v1.clone()));

        upstream.bump();
        let verdicts = check_upstream(&entries, None).await;
        assert_ne!(verdicts["foo"], Upstream::Hash(v1));

        upstream.drop_skill();
        let verdicts = check_upstream(&entries, None).await;
        assert_eq!(verdicts["foo"], Upstream::Removed);

        unsafe {
            match previous {
                Some(value) => std::env::set_var("SKILLSTAR_DATA_DIR", value),
                None => std::env::remove_var("SKILLSTAR_DATA_DIR"),
            }
        }
    }

    #[test]
    fn apply_reinstalls_and_updates_lock_hash() {
        let _guard = crate::lock_test_env();
        let sandbox = tempfile::tempdir().unwrap();
        let previous_data = std::env::var_os("SKILLSTAR_DATA_DIR");
        let previous_hub = std::env::var_os("SKILLSTAR_HUB_DIR");
        unsafe {
            std::env::set_var("SKILLSTAR_DATA_DIR", sandbox.path().join("data"));
            std::env::set_var("SKILLSTAR_HUB_DIR", sandbox.path().join("hub"));
        }

        let upstream = UpstreamFixture::new();
        // Initial install through the real pipeline (v1).
        let spec = Source::parse(&upstream.url).unwrap();
        {
            let checkout = fetch::fetch_source(&spec, &session()).unwrap();
            installer::install_units(
                checkout.dir(),
                &spec,
                &[InstallUnit {
                    id: "foo".into(),
                    folder_path: "skills/foo".into(),
                }],
            )
            .unwrap();
        }
        // The file:// fixture drives the same clone path a GitHub source
        // takes; model the provenance a real GitHub install would record.
        skill_lock::mutate(|lock| {
            if let Some(entry) = lock.skills.get_mut("foo") {
                entry.source_type = SourceType::Github;
            }
        })
        .unwrap();
        let old_hash = skill_lock::load().skills["foo"].skill_folder_hash.clone();

        upstream.bump();
        let results = apply_updates(&["foo".to_string()], &session());
        assert!(
            matches!(&results[0].result, UpdateResult::Updated { .. }),
            "{:?}",
            results[0].result
        );
        let canonical = skillstar_core::infra::paths::agents_skill_dir("foo");
        let content = std::fs::read_to_string(canonical.join("SKILL.md")).unwrap();
        assert!(content.contains("v2"), "{content}");
        let new_hash = skill_lock::load().skills["foo"].skill_folder_hash.clone();
        assert_ne!(old_hash, new_hash);

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
    }

    #[test]
    fn apply_reports_removed_upstream() {
        let _guard = crate::lock_test_env();
        let sandbox = tempfile::tempdir().unwrap();
        let previous_data = std::env::var_os("SKILLSTAR_DATA_DIR");
        unsafe { std::env::set_var("SKILLSTAR_DATA_DIR", sandbox.path().join("data")) };

        let upstream = UpstreamFixture::new();
        let mut lock = skill_lock::SkillLock::default();
        lock.upsert(
            "foo",
            entry(&upstream.url, "skills/foo", Some("deadbeef".into())),
        );
        lock.save(&skill_lock::lock_path()).unwrap();

        upstream.drop_skill();
        let results = apply_updates(&["foo".to_string()], &session());
        assert_eq!(results[0].result, UpdateResult::Removed);

        unsafe {
            match previous_data {
                Some(value) => std::env::set_var("SKILLSTAR_DATA_DIR", value),
                None => std::env::remove_var("SKILLSTAR_DATA_DIR"),
            }
        }
    }

    #[test]
    fn subtree_from_api_handles_depth() {
        let mut tree = crate::update_api::ApiRemoteTree::default();
        tree.folders.insert(String::new(), "root".into());
        tree.folders.insert("skills".into(), "abc".into());
        assert_eq!(subtree_from_api(&tree, ""), Some("root".into()));
        assert_eq!(subtree_from_api(&tree, "skills"), Some("abc".into()));
        assert_eq!(subtree_from_api(&tree, "skills/foo"), None);
        assert_eq!(subtree_from_api(&tree, "missing"), None);
    }

    #[test]
    fn missing_lock_entry_fails_with_reason() {
        let results = apply_updates(&["nope".to_string()], &session());
        assert!(matches!(&results[0].result, UpdateResult::Failed(reason) if reason.contains("nope")));
    }
}
