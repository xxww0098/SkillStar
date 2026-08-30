//! Pack-layout rules for repository skill discovery.
//!
//! Some skill packs (rust-skills, impeccable-style sources) keep the
//! canonical skill under `skills/<name>/` and also publish a repo-root
//! `SKILL.md` so one-level harness scanners can treat the whole clone as a
//! skill directory. That root file is a **shim**, not an install unit:
//! installing it would link the entire repository (tests, scripts, generated
//! harness copies) as the skill.
//!
//! Canonical catalog folders outrank generated per-harness copies when the
//! same skill identity appears more than once.

/// True when `folder_path` is a public or unpublished skill catalog, not a
/// generated harness copy (`.claude/skills`, `.grok/skills`, …).
pub fn is_canonical_skill_folder(folder_path: &str) -> bool {
    let path = folder_path.replace('\\', "/");
    let path = path.trim_matches('/');
    path == "skills"
        || path.starts_with("skills/")
        || path == "source/skills"
        || path.starts_with("source/skills/")
}

/// Higher values win when the same skill identity is found in more than one
/// folder. Root (empty path) is scored separately by the caller.
pub fn source_priority(folder_path: &str) -> u8 {
    let path = folder_path.replace('\\', "/");
    if is_canonical_skill_folder(&path) {
        3
    } else if path.starts_with(".agent/skills") || path.starts_with(".agents/skills") {
        2
    } else {
        1
    }
}

/// Priority used when deduplicating discovered skills by identity.
pub fn discovered_folder_priority(folder_path: &str) -> u8 {
    if folder_path.is_empty() {
        4
    } else {
        source_priority(folder_path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canonical_folders_are_skills_and_source_skills() {
        assert!(is_canonical_skill_folder("skills/rust"));
        assert!(is_canonical_skill_folder("skills\\rust"));
        assert!(is_canonical_skill_folder("source/skills/my-skill"));
        assert!(!is_canonical_skill_folder(""));
        assert!(!is_canonical_skill_folder(".claude/skills/rust"));
        assert!(!is_canonical_skill_folder(".grok/skills/rust"));
        assert!(!is_canonical_skill_folder(".agents/skills/rust"));
    }

    #[test]
    fn catalog_outranks_harness_copies() {
        assert!(source_priority("skills/rust") > source_priority(".agents/skills/rust"));
        assert!(source_priority("skills/rust") > source_priority(".claude/skills/rust"));
        assert_eq!(
            source_priority("skills/foo"),
            source_priority("source/skills/foo")
        );
        assert!(source_priority("source/skills/foo") > source_priority(".agents/skills/foo"));
        assert!(source_priority(".agents/skills/foo") > source_priority(".claude/skills/foo"));
        assert_eq!(
            source_priority(".agent/skills/foo"),
            source_priority(".agents/skills/foo")
        );
        assert_eq!(discovered_folder_priority(""), 4);
        assert!(discovered_folder_priority("") > source_priority("skills/rust"));
    }
}

#[cfg(test)]
mod discovery_integration {
    use crate::discovery::discover_skills;

    fn write_skill_md(path: &std::path::Path, name: &str, description: &str) {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        std::fs::write(
            path,
            format!("---\nname: {name}\ndescription: {description}\n---\n\n# {name}\n"),
        )
        .unwrap();
    }

    #[test]
    fn pack_root_shim_installs_canonical_skills_folder() {
        let dir = tempfile::tempdir().unwrap();
        let repo = dir.path().join("rust-skills");
        write_skill_md(&repo.join("SKILL.md"), "rust", "shim at pack root");
        write_skill_md(
            &repo.join("skills/rust/SKILL.md"),
            "rust",
            "canonical rust skill",
        );
        write_skill_md(
            &repo.join(".claude/skills/rust/SKILL.md"),
            "rust",
            "harness copy",
        );
        std::fs::create_dir_all(repo.join("tests")).unwrap();
        std::fs::write(repo.join("tests/not-a-skill.txt"), "noise").unwrap();

        for full_depth in [false, true] {
            let skills = discover_skills(&repo, full_depth);
            assert_eq!(skills.len(), 1, "full_depth={full_depth}: {skills:?}");
            assert_eq!(skills[0].id, "rust");
            assert_eq!(
                skills[0].folder_path, "skills/rust",
                "must not install the whole repo (full_depth={full_depth})"
            );
        }
    }

    #[test]
    fn genuine_root_skill_still_wins_over_differently_named_nested() {
        let dir = tempfile::tempdir().unwrap();
        let repo = dir.path().join("owner--repo");
        write_skill_md(&repo.join("SKILL.md"), "root-skill", "root");
        write_skill_md(
            &repo.join("skills/nested-skill/SKILL.md"),
            "nested-skill",
            "nested",
        );

        let skills = discover_skills(&repo, false);
        assert_eq!(skills.len(), 1);
        assert_eq!(skills[0].id, "root-skill");
        assert!(skills[0].folder_path.is_empty());
    }

    #[test]
    fn case_insensitive_shim_identity() {
        let dir = tempfile::tempdir().unwrap();
        let repo = dir.path();
        write_skill_md(&repo.join("SKILL.md"), "Rust", "shim");
        write_skill_md(&repo.join("skills/rust/SKILL.md"), "rust", "canonical");

        let skills = discover_skills(repo, false);
        assert_eq!(skills.len(), 1);
        assert_eq!(skills[0].folder_path, "skills/rust");
    }
}
