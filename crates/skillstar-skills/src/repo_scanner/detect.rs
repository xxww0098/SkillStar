use std::collections::{HashMap, HashSet};
use std::path::Path;

use tracing::warn;

use crate::discovery as skill_discover;
use crate::git::ops as git_ops;
use crate::git::transport::GitOperationSession;
use crate::lockfile;
use crate::source_resolver;
use crate::validation;
use crate::{update_checker, update_state};
use skillstar_core::infra::parallel;
use skillstar_core::infra::paths;

use super::cache::cache_dir_name;
use super::scan::annotate_discovered_skills;
use super::{DiscoveredSkill, RepoNewSkill};

/// Skills that every cached repository offers but the user has not installed.
///
/// Two sources feed this. The local checkout covers whatever the last
/// install or update materialized. The tracked remote revision — kept fetched
/// by the update checks — covers what upstream added since: the checkout
/// alone is blind to that, because it only moves when a Skill from the same
/// repository is installed or updated, so a Skill added upstream would stay
/// invisible until the user happened to pull something else from there.
///
/// A new Skill the last update check identified as the successor of an
/// installed one carries that name in `renamed_from`, so the UI can offer a
/// migration instead of presenting it as unrelated.
pub fn detect_new_skills_in_cached_repos(session: &GitOperationSession) -> Vec<RepoNewSkill> {
    let lock_path = paths::lockfile_path();
    let lf = match lockfile::Lockfile::load(&lock_path) {
        Ok(lf) => lf,
        Err(_) => return Vec::new(),
    };

    if lf.skills.is_empty() {
        return Vec::new();
    }

    let mut repo_groups: HashMap<String, (String, String)> = HashMap::new();
    let mut installed_repo: HashMap<String, String> = HashMap::new();

    for entry in &lf.skills {
        if entry.git_url.is_empty() {
            continue;
        }
        let norm_url = source_resolver::normalize_remote_url(&entry.git_url);
        installed_repo.insert(entry.name.clone(), norm_url.clone());
        repo_groups.entry(norm_url).or_insert_with(|| {
            // Must match the install-time derivation (`Source::parse(...).short`),
            // or non-GitHub repos never map to their cache directory.
            let source = source_resolver::Source::parse(&entry.git_url)
                .map(|parsed| parsed.short)
                .unwrap_or_else(|_| entry.git_url.clone());
            (source, entry.git_url.clone())
        });
    }

    let cache_dir = paths::repos_cache_dir();
    let mut jobs = Vec::new();
    for (source, repo_url) in repo_groups.values() {
        if !matches!(
            crate::skill_mutation::policy().managed_repository_for_url(repo_url),
            Ok(None)
        ) {
            continue;
        }
        let repo_dir = cache_dir.join(cache_dir_name(source));
        if !repo_dir.join(".git").exists() {
            continue;
        }
        jobs.push((source.clone(), repo_url.clone(), repo_dir));
    }

    let successors = update_state::successors();
    let mut found: Vec<RepoNewSkill> = parallel::map_bounded(
        jobs,
        parallel::blocking_concurrency_limit(),
        |(source, repo_url, repo_dir)| {
            let mut discovered = skill_discover::discover_skills(&repo_dir, false);
            // A repository that is itself one Skill lists only its root in
            // root-first mode; keep that shape instead of surfacing nested
            // folders upstream may have added.
            if !discovered.iter().any(|skill| skill.folder_path.is_empty()) {
                discovered.extend(upstream_added_skills(&repo_dir, session));
            }
            annotate_discovered_skills(discovered, &repo_url)
                .into_iter()
                .filter(|skill| !skill.already_installed)
                .map(|skill| RepoNewSkill {
                    repo_source: source.clone(),
                    repo_url: repo_url.clone(),
                    skill_id: skill.id,
                    folder_path: skill.folder_path,
                    description: skill.description,
                    renamed_from: None,
                })
                .collect::<Vec<_>>()
        },
    )
    .into_iter()
    .flatten()
    .collect();

    for ghost in &mut found {
        let ghost_repo = source_resolver::normalize_remote_url(&ghost.repo_url);
        ghost.renamed_from = successors
            .iter()
            .find(|(old_name, successor)| {
                successor.folder_path == ghost.folder_path
                    && installed_repo.get(old_name) == Some(&ghost_repo)
            })
            .map(|(old_name, _)| old_name.clone());
    }
    found
}

/// Container-style Skill folders present at `tracked_ref` but absent from
/// the local `HEAD` tree, sorted. `None` when either tree cannot be listed
/// (typically a checkout that has never fetched).
///
/// Only container additions count (see
/// [`skill_discover::is_container_skill_dir`]); a repository root turning
/// into a Skill is not surfaced here.
pub(crate) fn upstream_added_dirs(repo_dir: &Path, tracked_ref: &str) -> Option<Vec<String>> {
    let local = git_ops::list_tree_paths(repo_dir).ok()?;
    let remote = git_ops::list_tree_paths_at(repo_dir, tracked_ref).ok()?;
    let local: HashSet<&str> = local.iter().map(String::as_str).collect();
    let remote: HashSet<&str> = remote.iter().map(String::as_str).collect();

    let mut added: Vec<String> = remote
        .iter()
        .filter(|path| !local.contains(*path))
        .filter_map(|path| path.strip_suffix("/SKILL.md"))
        .filter(|folder| {
            skill_discover::is_container_skill_dir(folder, |candidate| remote.contains(candidate))
        })
        .map(str::to_string)
        .collect();
    added.sort_unstable();
    Some(added)
}

/// The Skill at `folder` as `revision` ships it, read straight from the Git
/// object so nothing gets checked out. `None` when the manifest cannot be
/// read this time (offline partial clone, oversized file); callers retry on
/// their next cycle.
pub(crate) fn skill_at_revision(
    repo_dir: &Path,
    revision: &str,
    folder: &str,
    session: &GitOperationSession,
) -> Option<DiscoveredSkill> {
    let manifest = match git_ops::read_blob_in_session(
        repo_dir,
        revision,
        &format!("{folder}/SKILL.md"),
        validation::MAX_MANIFEST_BYTES,
        session,
    ) {
        Ok(content) => content,
        Err(error) => {
            warn!(
                target: "repo_scanner",
                path = %repo_dir.display(),
                folder,
                error = %error,
                "could not read an upstream SKILL.md; skipping it this cycle"
            );
            return None;
        }
    };
    let report = validation::inspect_skill_frontmatter_content(&manifest);
    let default_name = folder.rsplit('/').next()?.to_string();
    Some(DiscoveredSkill {
        id: report
            .name
            .filter(|name| !name.trim().is_empty())
            .unwrap_or(default_name),
        folder_path: folder.to_string(),
        description: report.description.unwrap_or_default(),
        already_installed: false,
        frontmatter_issues: report
            .issues
            .iter()
            .map(|issue| issue.as_code().to_string())
            .collect(),
    })
}

/// Skills present at the tracked remote revision but absent from the local
/// `HEAD` tree.
fn upstream_added_skills(repo_dir: &Path, session: &GitOperationSession) -> Vec<DiscoveredSkill> {
    let tracked_ref = update_checker::tracked_update_ref(repo_dir);
    upstream_added_dirs(repo_dir, tracked_ref)
        .unwrap_or_default()
        .iter()
        .filter_map(|folder| skill_at_revision(repo_dir, tracked_ref, folder, session))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::process::Command;

    fn git(dir: &Path, args: &[&str]) {
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
    }

    fn write_skill(repo: &Path, folder: &str, name: &str, description: &str) {
        let dir = repo.join(folder);
        fs::create_dir_all(&dir).unwrap();
        fs::write(
            dir.join("SKILL.md"),
            format!("---\nname: {name}\ndescription: {description}\n---\n# {name}\n"),
        )
        .unwrap();
    }

    #[test]
    fn upstream_additions_surface_after_a_fetch_without_touching_the_checkout() {
        let _guard = crate::lock_test_env();
        let temp = tempfile::tempdir().unwrap();
        let previous_data = std::env::var_os("SKILLSTAR_DATA_DIR");
        let previous_hub = std::env::var_os("SKILLSTAR_HUB_DIR");
        unsafe {
            std::env::set_var("SKILLSTAR_DATA_DIR", temp.path().join("data"));
            std::env::set_var("SKILLSTAR_HUB_DIR", temp.path().join("hub"));
        }
        update_state::reset_for_test();

        let result = (|| -> anyhow::Result<()> {
            let remote = temp.path().join("remote");
            fs::create_dir_all(&remote)?;
            git(&remote, &["init", "-q", "--initial-branch=main"]);
            git(&remote, &["config", "user.email", "test@example.com"]);
            git(&remote, &["config", "user.name", "SkillStar Tests"]);
            write_skill(&remote, "skills/one", "one", "First skill");
            git(&remote, &["add", "."]);
            git(&remote, &["commit", "-q", "-m", "one"]);

            let cache = paths::repos_cache_dir();
            fs::create_dir_all(&cache)?;
            let repo_dir = cache.join(cache_dir_name("acme/demo"));
            git(
                &cache,
                &[
                    "clone",
                    "-q",
                    remote.to_str().unwrap(),
                    repo_dir.to_str().unwrap(),
                ],
            );

            let hub = paths::hub_skills_dir();
            fs::create_dir_all(&hub)?;
            skillstar_core::infra::fs_ops::create_symlink(
                &repo_dir.join("skills/one"),
                &hub.join("one"),
            )?;
            let mut lock = lockfile::Lockfile::default();
            lock.upsert(lockfile::LockEntry {
                name: "one".into(),
                git_url: "https://github.com/acme/demo.git".into(),
                git_ref: None,
                tree_hash: "tree".into(),
                content_hash: None,
                content_hash_version: None,
                installed_at: "2026-08-21T00:00:00Z".into(),
                source_folder: Some("skills/one".into()),
            });
            lock.save(&lockfile::lockfile_path())?;

            let session = GitOperationSession::public();
            assert!(
                detect_new_skills_in_cached_repos(&session).is_empty(),
                "nothing is new before upstream moves"
            );

            write_skill(&remote, "skills/in-progress/two", "two", "Second skill");
            git(&remote, &["add", "."]);
            git(&remote, &["commit", "-q", "-m", "two"]);
            assert!(
                detect_new_skills_in_cached_repos(&session).is_empty(),
                "an addition nobody fetched yet is not visible"
            );

            git(&repo_dir, &["fetch", "-q"]);
            let found = detect_new_skills_in_cached_repos(&session);
            assert_eq!(found.len(), 1, "{found:?}");
            assert_eq!(found[0].skill_id, "two");
            assert_eq!(found[0].folder_path, "skills/in-progress/two");
            assert_eq!(found[0].description, "Second skill");
            assert_eq!(found[0].repo_source, "acme/demo");
            assert_eq!(found[0].renamed_from, None);
            assert!(
                !repo_dir.join("skills/in-progress").exists(),
                "detection must never materialize the checkout"
            );

            // Once an update check has recorded that "one" moved into "two",
            // the ghost says so instead of posing as an unrelated Skill.
            update_state::commit_scan(
                update_state::stamp(),
                &[update_state::SkillUpdateState {
                    name: "one".into(),
                    update_available: false,
                    upstream_change: Some(skillstar_core::types::UpstreamChange::Removed {
                        suggested_local_name: "one.local".into(),
                        successor: Some(skillstar_core::types::UpstreamSuccessor {
                            skill_id: "two".into(),
                            folder_path: "skills/in-progress/two".into(),
                            description: "Second skill".into(),
                            similarity: Some(100),
                        }),
                    }),
                }],
            );
            let found = detect_new_skills_in_cached_repos(&session);
            assert_eq!(found[0].renamed_from.as_deref(), Some("one"));
            Ok(())
        })();

        update_state::reset_for_test();
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
    fn container_rules_match_root_first_discovery() {
        let none = |_: &str| false;
        assert!(skill_discover::is_container_skill_dir(
            "skills/in-progress/implement-spec",
            none
        ));
        assert!(skill_discover::is_container_skill_dir("skills/demo", none));
        assert!(skill_discover::is_container_skill_dir(
            ".claude/skills/demo",
            none
        ));
        assert!(!skill_discover::is_container_skill_dir(
            "examples/demo",
            none
        ));
        assert!(!skill_discover::is_container_skill_dir(
            "skills/a/b/c/d",
            none
        ));
        assert!(
            !skill_discover::is_container_skill_dir("skills/a/b", |path| path
                == "skills/a/SKILL.md"),
            "a skill shadows everything nested below it"
        );
    }
}
