//! Deciding what an update changes, separated from carrying it out.
//!
//! Skills installed from the same repository share a checkout, so pulling one
//! of them moves all of them. Working out *which* lock entries that touches,
//! and what hash each should end up at, is the part of an update most likely
//! to be wrong — and the part that a caller can check without a real `$HOME`,
//! a real lockfile, or a real git remote.
//!
//! [`plan_update`] is that decision as a pure function. The executor in the
//! parent module supplies the one thing it cannot know on its own: what each
//! sibling looks like on disk.

use crate::lockfile::LockEntry;

/// What the executor found when it looked for a sibling of the updated skill.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum SiblingState {
    /// Not present in the hub. Its lock entry stands untouched and it takes no
    /// part in the update — an entry can outlive the directory it describes.
    Absent,
    /// Present in the hub. `Some(hash)` when a fresh tree hash was computed;
    /// `None` when hashing failed, in which case the old hash must stand but
    /// the skill still counts as touched.
    Present(Option<String>),
}

/// Every lock-entry write and follow-up an update implies.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct UpdatePlan {
    /// `(lock entry name, new tree hash)` writes to apply, target first.
    pub hash_writes: Vec<(String, String)>,
    /// Siblings refreshed alongside the target, sorted.
    pub siblings_cleared: Vec<String>,
    /// Target followed by its siblings — everything needing relink + cascade.
    pub affected: Vec<String>,
    /// Remote of the updated skill, empty when it has no lock entry.
    pub git_url: String,
}

/// Work out what updating `name` to `pulled_hash` changes.
///
/// `is_repo_skill` distinguishes a skill symlinked into the shared repo cache
/// (siblings move with it) from a standalone clone (nothing else is touched).
pub(crate) fn plan_update<F>(
    entries: &[LockEntry],
    name: &str,
    pulled_hash: &str,
    is_repo_skill: bool,
    sibling_state: F,
) -> UpdatePlan
where
    F: Fn(&LockEntry) -> SiblingState,
{
    let target = entries.iter().find(|entry| entry.name == name);

    let mut plan = UpdatePlan {
        git_url: target
            .map(|entry| entry.git_url.clone())
            .unwrap_or_default(),
        ..Default::default()
    };

    // Only a skill with a lock entry has somewhere to record its new hash.
    if target.is_some() {
        plan.hash_writes
            .push((name.to_string(), pulled_hash.to_string()));
    }

    if is_repo_skill && let Some(target) = target {
        let mut siblings: Vec<String> = Vec::new();
        for entry in entries
            .iter()
            .filter(|entry| entry.git_url == target.git_url && entry.name != name)
        {
            let SiblingState::Present(hash) = sibling_state(entry) else {
                continue;
            };
            if let Some(hash) = hash {
                plan.hash_writes.push((entry.name.clone(), hash));
            }
            siblings.push(entry.name.clone());
        }
        siblings.sort();
        siblings.dedup();
        plan.siblings_cleared = siblings;
    }

    plan.affected = std::iter::once(name.to_string())
        .chain(plan.siblings_cleared.iter().cloned())
        .collect();
    plan
}

/// One update call and the other requested names it covers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RepoGroup {
    pub representative: String,
    pub covered: Vec<String>,
}

/// Collapse a batch request down to one update per repository.
///
/// Skills sharing a repository share a checkout, so updating one of them
/// already moves the rest — issuing a second pull would fetch the same repo
/// again for no gain. Skills with no lock entry, or no recorded remote, stand
/// alone: nothing is known to travel with them.
///
/// Request order is preserved, and the first requested member of a repository
/// becomes its representative.
pub(crate) fn group_by_repo(entries: &[LockEntry], names: &[String]) -> Vec<RepoGroup> {
    let mut groups: Vec<RepoGroup> = Vec::new();
    let mut keys: Vec<String> = Vec::new();
    let mut seen: Vec<&str> = Vec::new();

    for name in names {
        if seen.contains(&name.as_str()) {
            continue;
        }
        seen.push(name);

        let key = entries
            .iter()
            .find(|entry| entry.name == *name)
            .map(|entry| entry.git_url.trim())
            .filter(|git_url| !git_url.is_empty())
            .map(|git_url| format!("repo:{git_url}"))
            .unwrap_or_else(|| format!("standalone:{name}"));

        match keys.iter().position(|existing| *existing == key) {
            Some(index) => groups[index].covered.push(name.clone()),
            None => {
                keys.push(key);
                groups.push(RepoGroup {
                    representative: name.clone(),
                    covered: Vec::new(),
                });
            }
        }
    }

    groups
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(values: &[&str]) -> Vec<String> {
        values.iter().map(|value| value.to_string()).collect()
    }

    fn entry(name: &str, git_url: &str, folder: Option<&str>) -> LockEntry {
        LockEntry {
            name: name.to_string(),
            git_url: git_url.to_string(),
            git_ref: None,
            tree_hash: format!("old-{name}"),
            installed_at: "2026-01-01T00:00:00Z".to_string(),
            source_folder: folder.map(str::to_string),
        }
    }

    /// Every sibling present, each hashing to `new-<name>`.
    fn all_present(entry: &LockEntry) -> SiblingState {
        SiblingState::Present(Some(format!("new-{}", entry.name)))
    }

    #[test]
    fn repo_siblings_each_take_their_own_subtree_hash() {
        let entries = vec![
            entry(
                "alpha",
                "https://github.com/acme/pack",
                Some("skills/alpha"),
            ),
            entry("beta", "https://github.com/acme/pack", Some("skills/beta")),
            entry(
                "gamma",
                "https://github.com/acme/pack",
                Some("skills/gamma"),
            ),
        ];

        let plan = plan_update(&entries, "alpha", "pulled-hash", true, all_present);

        assert_eq!(
            plan.hash_writes,
            vec![
                ("alpha".to_string(), "pulled-hash".to_string()),
                ("beta".to_string(), "new-beta".to_string()),
                ("gamma".to_string(), "new-gamma".to_string()),
            ]
        );
        assert_eq!(plan.siblings_cleared, vec!["beta", "gamma"]);
        assert_eq!(plan.affected, vec!["alpha", "beta", "gamma"]);
        assert_eq!(plan.git_url, "https://github.com/acme/pack");
    }

    #[test]
    fn skills_from_other_repos_are_left_alone() {
        let entries = vec![
            entry("alpha", "https://github.com/acme/pack", None),
            entry("stranger", "https://github.com/other/pack", None),
        ];

        let plan = plan_update(&entries, "alpha", "pulled-hash", true, all_present);

        assert_eq!(plan.siblings_cleared, Vec::<String>::new());
        assert_eq!(plan.affected, vec!["alpha"]);
        assert!(
            plan.hash_writes.iter().all(|(name, _)| name != "stranger"),
            "a different remote must never be rehashed"
        );
    }

    #[test]
    fn sibling_missing_from_hub_is_not_touched() {
        let entries = vec![
            entry("alpha", "https://github.com/acme/pack", None),
            entry("ghost", "https://github.com/acme/pack", None),
        ];

        let plan = plan_update(&entries, "alpha", "pulled-hash", true, |entry| {
            if entry.name == "ghost" {
                SiblingState::Absent
            } else {
                all_present(entry)
            }
        });

        assert_eq!(plan.siblings_cleared, Vec::<String>::new());
        assert_eq!(plan.affected, vec!["alpha"]);
        assert_eq!(
            plan.hash_writes,
            vec![("alpha".to_string(), "pulled-hash".to_string())],
            "an entry whose directory is gone keeps its recorded hash"
        );
    }

    #[test]
    fn sibling_that_fails_to_hash_keeps_its_hash_but_still_counts_as_touched() {
        let entries = vec![
            entry("alpha", "https://github.com/acme/pack", None),
            entry("broken", "https://github.com/acme/pack", None),
        ];

        let plan = plan_update(&entries, "alpha", "pulled-hash", true, |entry| {
            if entry.name == "broken" {
                SiblingState::Present(None)
            } else {
                all_present(entry)
            }
        });

        assert_eq!(
            plan.hash_writes,
            vec![("alpha".to_string(), "pulled-hash".to_string())],
            "a failed hash must not overwrite the recorded one"
        );
        assert_eq!(
            plan.siblings_cleared,
            vec!["broken"],
            "it still needs a relink — the checkout moved under it"
        );
        assert_eq!(plan.affected, vec!["alpha", "broken"]);
    }

    #[test]
    fn standalone_clone_updates_only_itself() {
        let entries = vec![
            entry("alpha", "https://github.com/acme/pack", None),
            entry("beta", "https://github.com/acme/pack", None),
        ];

        let plan = plan_update(&entries, "alpha", "pulled-hash", false, all_present);

        assert_eq!(
            plan.hash_writes,
            vec![("alpha".to_string(), "pulled-hash".to_string())]
        );
        assert_eq!(plan.siblings_cleared, Vec::<String>::new());
        assert_eq!(plan.affected, vec!["alpha"]);
    }

    #[test]
    fn one_update_per_repository_covers_the_rest() {
        let entries = vec![
            entry("alpha", "https://github.com/acme/pack", None),
            entry("beta", "https://github.com/acme/pack", None),
            entry("solo", "https://github.com/other/solo", None),
        ];

        let groups = group_by_repo(&entries, &names(&["alpha", "beta", "solo"]));

        assert_eq!(
            groups,
            vec![
                RepoGroup {
                    representative: "alpha".to_string(),
                    covered: names(&["beta"]),
                },
                RepoGroup {
                    representative: "solo".to_string(),
                    covered: Vec::new(),
                },
            ]
        );
    }

    #[test]
    fn the_first_requested_member_represents_its_repository() {
        let entries = vec![
            entry("alpha", "https://github.com/acme/pack", None),
            entry("beta", "https://github.com/acme/pack", None),
        ];

        let groups = group_by_repo(&entries, &names(&["beta", "alpha"]));

        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].representative, "beta");
        assert_eq!(groups[0].covered, names(&["alpha"]));
    }

    #[test]
    fn skills_without_a_recorded_remote_never_share_a_group() {
        let entries = vec![entry("tracked", "https://github.com/acme/pack", None)];

        let groups = group_by_repo(&entries, &names(&["loose-a", "loose-b", "tracked"]));

        assert_eq!(
            groups.iter().map(|g| &g.representative).collect::<Vec<_>>(),
            vec!["loose-a", "loose-b", "tracked"],
            "an unknown skill must not be assumed to share anything"
        );
        assert!(groups.iter().all(|group| group.covered.is_empty()));
    }

    #[test]
    fn a_name_requested_twice_is_updated_once() {
        let entries = vec![entry("alpha", "https://github.com/acme/pack", None)];

        let groups = group_by_repo(&entries, &names(&["alpha", "alpha"]));

        assert_eq!(groups.len(), 1);
        assert!(groups[0].covered.is_empty());
    }

    #[test]
    fn skill_without_a_lock_entry_writes_nothing_and_has_no_remote() {
        let entries = vec![entry("beta", "https://github.com/acme/pack", None)];

        let plan = plan_update(&entries, "orphan", "pulled-hash", true, all_present);

        assert_eq!(plan.hash_writes, Vec::new());
        assert_eq!(plan.siblings_cleared, Vec::<String>::new());
        assert_eq!(plan.affected, vec!["orphan"]);
        assert_eq!(plan.git_url, "", "no lock entry means no known remote");
    }
}
