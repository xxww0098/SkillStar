//! Skills present upstream that this source's lock does not already record.
//!
//! The list is for update --check and --dry-run. It never selects a skill
//! for installation or update.

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LockedSkillRef {
    pub name: String,
    pub folder_path: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiscoveredSourceSkill {
    pub name: String,
    pub folder_path: String,
    pub installable: bool,
    pub internal: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewlyAddedSkill {
    pub name: String,
    pub source: String,
    pub folder_path: String,
    pub internal: bool,
}

pub fn newly_added_skills(
    source: &str,
    locked: &[LockedSkillRef],
    discovered: &[DiscoveredSourceSkill],
) -> Vec<NewlyAddedSkill> {
    let names = locked
        .iter()
        .map(|skill| skill.name.to_ascii_lowercase())
        .collect::<std::collections::BTreeSet<_>>();
    let paths = locked
        .iter()
        .map(|skill| normalize_path(&skill.folder_path))
        .collect::<std::collections::BTreeSet<_>>();
    let mut added = Vec::new();
    for skill in discovered {
        if !skill.installable {
            continue;
        }
        if names.contains(&skill.name.to_ascii_lowercase()) {
            continue;
        }
        if paths.contains(&normalize_path(&skill.folder_path)) {
            continue;
        }
        added.push(NewlyAddedSkill {
            name: skill.name.clone(),
            source: source.to_string(),
            folder_path: skill.folder_path.clone(),
            internal: skill.internal,
        });
    }
    added.sort_by(|left, right| {
        left.name
            .to_ascii_lowercase()
            .cmp(&right.name.to_ascii_lowercase())
    });
    added
}

fn normalize_path(path: &str) -> String {
    path.replace('\\', "/").trim_matches('/').to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn locked(name: &str, path: &str) -> LockedSkillRef {
        LockedSkillRef {
            name: name.to_string(),
            folder_path: path.to_string(),
        }
    }

    fn found(name: &str, path: &str, internal: bool) -> DiscoveredSourceSkill {
        DiscoveredSourceSkill {
            name: name.to_string(),
            folder_path: path.to_string(),
            installable: true,
            internal,
        }
    }

    #[test]
    fn lists_a_new_skill_without_treating_it_as_installed() {
        let added = newly_added_skills(
            "owner/repo",
            &[locked("old", "skills/old")],
            &[
                found("old", "skills/old", false),
                found("fresh", "skills/fresh", false),
            ],
        );
        assert_eq!(added.len(), 1);
        assert_eq!(added[0].name, "fresh");
        assert!(!added[0].internal);
    }

    #[test]
    fn skips_locked_paths_broken_frontmatter_and_marks_internal() {
        let mut broken = found("broken", "skills/broken", false);
        broken.installable = false;
        let added = newly_added_skills(
            "owner/repo",
            &[locked("old", "skills/moved")],
            &[
                found("renamed", "skills/moved", false),
                broken,
                found("secret", "skills/secret", true),
            ],
        );
        assert_eq!(added.len(), 1);
        assert!(added[0].internal);
        assert_eq!(added[0].name, "secret");
    }
}
