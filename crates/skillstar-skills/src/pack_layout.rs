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
//! same skill identity appears more than once **and** the caller did not
//! ask for a specific harness. A carousel / `--agent` click selects the
//! matching `.<harness>/` tree when it exists; otherwise it falls back to
//! catalog, the existing hub folder, or another nested copy — never the
//! repo root.

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

/// Known pack-tree prefixes for builtin agents. Codex skills live under
/// `.agents` (not `.codex/skills`). Antigravity uses `.agent`. DSH uses
/// `.dsh`. Cursor uses `.cursor`.
const KNOWN_PACK_HARNESS: &[(&str, &str)] = &[
    ("antigravity", ".agent"),
    ("augment", ".augment"),
    ("claude-code", ".claude"),
    ("codebuddy", ".codebuddy"),
    ("codex", ".agents"),
    ("copilot", ".github"),
    ("crush", ".crush"),
    ("cursor", ".cursor"),
    ("deepseek", ".dsh"),
    ("factory-droid", ".factory"),
    ("gemini-cli", ".gemini"),
    ("goose", ".goose"),
    ("iflow", ".iflow"),
    ("kilocode", ".kilocode"),
    ("kiro", ".kiro"),
    ("mux", ".mux"),
    ("neovate", ".neovate"),
    ("opencode", ".opencode"),
    ("pochi", ".pochi"),
    ("qoder", ".qoder"),
    ("qwen-code", ".qwen"),
    ("roo", ".roo"),
    ("trae", ".trae"),
    ("windsurf", ".windsurf"),
];

/// Pack-relative prefix for a target agent (`".cursor"`, `".dsh"`, …).
///
/// Prefers the hardcoded table (Codex → `.agents`, not the global
/// `~/.codex/skills` parent). Then the parent of `global_skills_dir`
/// when it is a hidden directory named `skills`. Project-relative
/// paths are last — Cursor's project dir is `.agents/skills` and
/// must not win over `.cursor`.
pub fn pack_harness_prefix(
    agent_id: &str,
    global_skills_dir: Option<&str>,
    project_skills_rel: Option<&str>,
) -> Option<String> {
    if let Some((_, prefix)) = KNOWN_PACK_HARNESS.iter().find(|(id, _)| *id == agent_id) {
        return Some((*prefix).to_string());
    }
    if let Some(dir) = global_skills_dir {
        if let Some(prefix) = hidden_skills_parent(dir) {
            return Some(prefix);
        }
    }
    if let Some(rel) = project_skills_rel {
        if let Some(prefix) = hidden_skills_parent(rel) {
            return Some(prefix);
        }
    }
    None
}

fn hidden_skills_parent(path: &str) -> Option<String> {
    let normalized = path.replace('\\', "/");
    let mut parts: Vec<&str> = normalized.split('/').filter(|s| !s.is_empty()).collect();
    if parts.last().copied() != Some("skills") {
        return None;
    }
    parts.pop();
    let parent = parts.last()?;
    if parent.starts_with('.') && *parent != "." {
        return Some((*parent).to_string());
    }
    None
}

/// `folder_path` is the harness root or a path under it.
/// `.agent` must not match `.agents` / `.agents/skills/…`.
pub fn folder_matches_harness(folder_path: &str, prefix: &str) -> bool {
    folder_path == prefix || folder_path.starts_with(&format!("{prefix}/"))
}

/// Prefer `.<harness>/skills/<id>` over a `SKILL.md` sitting on the harness root.
pub fn harness_folder_rank(prefix: &str, folder: &str) -> u8 {
    let body = format!("{prefix}/skills/");
    if folder.starts_with(&body) {
        0
    } else if folder == prefix {
        1
    } else {
        2
    }
}

pub fn missing_skill_payload_error(prefix: &str, requested_name: Option<&str>) -> String {
    match requested_name {
        Some(name) => format!(
            "This pack has no installable SKILL.md for '{name}'. \
             Looked for '{prefix}/skills/{name}', catalog skills/, source/skills/, \
             and other harness copies. The repository root is not an install unit."
        ),
        None => format!(
            "This pack has no installable SKILL.md. \
             Looked for '{prefix}/skills/<name>', catalog skills/, source/skills/, \
             and other harness copies. The repository root is not an install unit."
        ),
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

    #[test]
    fn agent_prefix_does_not_match_a_longer_sibling() {
        assert!(folder_matches_harness(".agent/skills/impeccable", ".agent"));
        assert!(!folder_matches_harness(
            ".agents/skills/impeccable",
            ".agent"
        ));
        assert!(folder_matches_harness(
            ".agents/skills/impeccable",
            ".agents"
        ));
        assert!(!folder_matches_harness(
            ".agent/skills/impeccable",
            ".agents"
        ));
        assert!(folder_matches_harness(".cursor", ".cursor"));
        assert!(folder_matches_harness(".cursor/skills/rust", ".cursor"));
        assert!(!folder_matches_harness(
            ".cursor-extra/skills/rust",
            ".cursor"
        ));
    }

    #[test]
    fn known_agents_map_to_pack_harness_prefixes() {
        assert_eq!(
            pack_harness_prefix("cursor", Some("~/.cursor/skills"), Some(".agents/skills"))
                .as_deref(),
            Some(".cursor")
        );
        assert_eq!(
            pack_harness_prefix("deepseek", Some("~/.dsh/skills"), None).as_deref(),
            Some(".dsh")
        );
        assert_eq!(
            pack_harness_prefix("codex", Some("~/.codex/skills"), Some(".agents/skills"))
                .as_deref(),
            Some(".agents")
        );
        assert_eq!(
            pack_harness_prefix("antigravity", None, None).as_deref(),
            Some(".agent")
        );
    }

    #[test]
    fn unknown_agent_uses_hidden_global_skills_parent() {
        assert_eq!(
            pack_harness_prefix(
                "custom-bot",
                Some("~/.mybot/skills"),
                Some(".agents/skills")
            )
            .as_deref(),
            Some(".mybot")
        );
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

    #[test]
    fn root_shim_plus_harness_copies_does_not_install_the_repo_root() {
        let dir = tempfile::tempdir().unwrap();
        let repo = dir.path().join("rust-skills");
        write_skill_md(&repo.join("SKILL.md"), "rust", "shim at pack root");
        write_skill_md(
            &repo.join(".cursor/skills/rust/SKILL.md"),
            "rust",
            "cursor copy",
        );
        write_skill_md(&repo.join(".dsh/skills/rust/SKILL.md"), "rust", "dsh copy");
        std::fs::create_dir_all(repo.join("tests")).unwrap();
        std::fs::write(repo.join("tests/not-a-skill.txt"), "noise").unwrap();

        let skills = discover_skills(&repo, false);
        assert_eq!(skills.len(), 1, "{skills:?}");
        assert_eq!(skills[0].id, "rust");
        assert!(
            !skills[0].folder_path.is_empty(),
            "must not install the whole repo: {skills:?}"
        );
        assert!(
            skills[0].folder_path.starts_with(".cursor/")
                || skills[0].folder_path.starts_with(".dsh/"),
            "expected a harness folder, got {}",
            skills[0].folder_path
        );
    }

    #[test]
    fn catalog_wins_over_cursor_and_dsh_when_no_harness_is_requested() {
        let dir = tempfile::tempdir().unwrap();
        let repo = dir.path().join("rust-skills");
        write_skill_md(
            &repo.join("skills/rust/SKILL.md"),
            "rust",
            "canonical rust skill",
        );
        write_skill_md(
            &repo.join(".cursor/skills/rust/SKILL.md"),
            "rust",
            "cursor copy",
        );
        write_skill_md(&repo.join(".dsh/skills/rust/SKILL.md"), "rust", "dsh copy");

        let skills = discover_skills(&repo, false);
        assert_eq!(skills.len(), 1, "{skills:?}");
        assert_eq!(skills[0].folder_path, "skills/rust");
    }
}
