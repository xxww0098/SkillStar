//! Which folder of a fetched checkout becomes the install unit, and what to
//! do when the Skill is already installed from the same repository.
//!
//! Harness folders are identity aliases of `skills/<name>/`; the choice among
//! them is `pack_layout::choose_copy`, shared with the inventory.

use super::find_target_skill;
use crate::pack_layout::CopyRequest;
use crate::{lockfile, repo_scanner};
use std::path::Path;

pub(super) enum SameRepoAction {
    Reuse,
    Retarget,
    Reject,
}

fn lock_entry_for(name: &str) -> Option<lockfile::LockEntry> {
    lockfile::Lockfile::load(&lockfile::lockfile_path())
        .ok()?
        .skills
        .into_iter()
        .find(|entry| entry.name == name)
}

fn source_folder_eq(entry: Option<&lockfile::LockEntry>, folder: &str) -> bool {
    entry
        .and_then(|entry| entry.source_folder.as_deref())
        .unwrap_or("")
        == folder
}

/// Reuse only when the hub already points at the requested harness folder.
/// A different folder from the same clone must retarget; another git URL
/// is still a hard collision. A pinned entry never retargets: harness clicks
/// and reinstalls are ignored, and only uninstalling clears the pin.
pub(super) fn existing_same_repo_action(
    skill_id: &str,
    repo_url: &str,
    requested_folder: &str,
    harness_prefix: Option<&str>,
) -> SameRepoAction {
    let entry = lock_entry_for(skill_id);
    let same_repo = entry
        .as_ref()
        .is_some_and(|entry| crate::source_resolver::same_remote_url(&entry.git_url, repo_url));
    if !same_repo {
        return SameRepoAction::Reject;
    }
    if entry.as_ref().is_some_and(|entry| entry.pinned) {
        if !source_folder_eq(entry.as_ref(), requested_folder) {
            tracing::warn!(
                target: "install_skill",
                skill_id = %skill_id,
                pinned_folder = ?entry.as_ref().and_then(|entry| entry.source_folder.as_deref()),
                requested_folder = %requested_folder,
                "skill is pinned to a subpath; ignoring harness/reinstall request"
            );
        }
        return SameRepoAction::Reuse;
    }
    if source_folder_eq(entry.as_ref(), requested_folder) {
        return SameRepoAction::Reuse;
    }
    if harness_prefix.is_some() {
        return SameRepoAction::Retarget;
    }
    SameRepoAction::Reuse
}

pub(super) fn requested_skill_not_found_error(names: &[String]) -> String {
    format!(
        "Requested Skill{} '{}' not found in the scanned repository; the source may no longer provide {} or {} may have been deleted or renamed",
        if names.len() == 1 { "" } else { "s" },
        names.join(", "),
        if names.len() == 1 {
            "this Skill"
        } else {
            "these Skills"
        },
        if names.len() == 1 { "it" } else { "they" },
    )
}

/// Choose install units. Harness folders are identity aliases of `skills/<name>/`.
///
/// A tree URL's `source.subpath` is a hard pin: it always wins over any
/// already-recorded pin, which in turn always wins over `harness_prefix` —
/// a pinned identity is never resolved through the harness ranking, so a
/// harness click never even materializes the harness's copy for it.
pub(super) fn choose_install_skills(
    repo_dir: &Path,
    source: &crate::source_resolver::Source,
    requests: &[(Option<&str>, &str)],
    harness_prefix: Option<&str>,
    session: &crate::git::transport::GitOperationSession,
) -> Result<Vec<repo_scanner::DiscoveredSkill>, String> {
    let lock = lockfile::Lockfile::load(&lockfile::lockfile_path()).ok();
    let preferred_for = |search: &str| {
        lock.as_ref().and_then(|lockfile| {
            lockfile
                .skills
                .iter()
                .find(|entry| entry.name.eq_ignore_ascii_case(search))
                .and_then(|entry| entry.source_folder.as_deref())
        })
    };
    let pinned_for = |search: &str| {
        lock.as_ref().and_then(|lockfile| {
            lockfile
                .skills
                .iter()
                .find(|entry| entry.name.eq_ignore_ascii_case(search) && entry.pinned)
                .and_then(|entry| entry.source_folder.as_deref())
        })
    };

    if let Some(subpath) = source.subpath.as_deref() {
        // The pin target may be a folder discovery would otherwise never look
        // at (a test fixture, a deferred duplicate); materialize it directly
        // by path instead of going through the by-identity inventory lookup.
        repo_scanner::inventory::materialize_dirs(repo_dir, session, &[subpath.to_string()]);
    } else if harness_prefix.is_some() {
        // Filesystem discovery cannot see a deferred copy. A harness request
        // surfaces exactly the one copy the shared table picks for it — the
        // same table `resolve_install_skills` applies below, so both agree. A
        // pinned identity keeps its pinned folder instead of the harness's.
        let wants: Vec<(&str, CopyRequest)> = requests
            .iter()
            .map(|(requested, hint)| {
                let search = requested.unwrap_or(*hint);
                let request = match pinned_for(search) {
                    Some(folder) => CopyRequest {
                        harness: None,
                        installed: None,
                        pinned: Some(folder),
                    },
                    None => CopyRequest {
                        harness: harness_prefix,
                        installed: preferred_for(search),
                        pinned: None,
                    },
                };
                (search, request)
            })
            .collect();
        repo_scanner::inventory::materialize_for(repo_dir, session, &wants);
    }

    let mut chosen: Vec<repo_scanner::DiscoveredSkill> = Vec::new();
    let mut missing = Vec::new();
    for (requested_name, name_hint) in requests {
        let search = requested_name.unwrap_or(*name_hint);
        let pinned = source.subpath.as_deref().or_else(|| pinned_for(search));
        let copy = match pinned {
            Some(folder) => CopyRequest {
                harness: None,
                installed: None,
                pinned: Some(folder),
            },
            None => CopyRequest {
                harness: harness_prefix,
                installed: preferred_for(search),
                pinned: None,
            },
        };
        let resolved = crate::discovery::resolve_install_skills(
            repo_dir,
            &crate::discovery::InstallQuery {
                scope: pinned,
                name: *requested_name,
                copy,
            },
        )?;
        match find_target_skill(&resolved, *requested_name, name_hint) {
            Some(skill) => push_unique(&mut chosen, skill.clone()),
            None => match requested_name
                .and_then(|name| nameless_root_skill(&resolved).map(|skill| (name, skill.clone())))
            {
                Some((name, mut skill)) => {
                    skill.id = name.to_string();
                    push_unique(&mut chosen, skill);
                }
                None if requested_name.is_some() => missing.push(search.to_string()),
                None => {
                    return Err("No valid SKILL.md found in the selected source".to_string());
                }
            },
        }
    }
    if !missing.is_empty() {
        return Err(requested_skill_not_found_error(&missing));
    }
    Ok(chosen)
}

fn push_unique(
    chosen: &mut Vec<repo_scanner::DiscoveredSkill>,
    skill: repo_scanner::DiscoveredSkill,
) {
    if !chosen.iter().any(|seen| seen.id == skill.id) {
        chosen.push(skill);
    }
}

/// A genuine root SKILL.md with no frontmatter `name` keeps the requested
/// identity. Missing name is advisory; the old whole-repo clone used the
/// caller's name hint, and the pipeline must do the same.
pub(super) fn nameless_root_skill(
    skills: &[repo_scanner::DiscoveredSkill],
) -> Option<&repo_scanner::DiscoveredSkill> {
    let roots: Vec<_> = skills
        .iter()
        .filter(|skill| skill.folder_path.is_empty())
        .collect();
    let [root] = roots.as_slice() else {
        return None;
    };
    root.frontmatter_issues
        .iter()
        .any(|code| code == "missing_name")
        .then_some(*root)
}
