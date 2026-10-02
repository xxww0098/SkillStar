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

/// True when `folder_path` is a generated per-harness copy
/// (`.cursor/skills/<id>`, `.dsh`, …), not a catalog or repo-root skill.
pub fn is_harness_skill_folder(folder_path: &str) -> bool {
    let path = folder_path.replace('\\', "/");
    let path = path.trim_matches('/');
    if path.is_empty() || is_canonical_skill_folder(path) {
        return false;
    }
    let first = path.split('/').next().unwrap_or("");
    first.starts_with('.') && first != "."
}

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

/// Directory names that never hold installable Skills: build output, vendored
/// dependencies, and test fixtures (packs ship fixture `SKILL.md` files).
pub(crate) const IGNORED_DIR_NAMES: &[&str] = &[
    ".git",
    "node_modules",
    ".venv",
    "venv",
    "__pycache__",
    "target",
    "dist",
    "build",
    ".next",
    ".nuxt",
    "tests",
    "test",
    "__tests__",
    "fixtures",
];

/// True when an **ancestor** segment of `folder_path` is ignored: a Skill
/// named `test` inside `skills/` is still a Skill; `tests/**/impeccable` is not.
pub(crate) fn is_under_ignored_dir(folder_path: &str) -> bool {
    let path = folder_path.replace('\\', "/");
    let mut segments: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();
    segments.pop();
    segments
        .iter()
        .any(|segment| IGNORED_DIR_NAMES.contains(segment))
}

/// Case-insensitive Skill identity: frontmatter `name`, else the folder
/// basename (same rule as filesystem discovery).
pub(crate) fn identity_key(frontmatter_name: Option<&str>, folder_path: &str) -> String {
    frontmatter_name
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| folder_path.rsplit('/').next().unwrap_or(folder_path))
        .to_lowercase()
}

/// What the caller wants from a set of same-identity copies.
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct CopyRequest<'a> {
    /// Harness prefix of the clicked Agent (`".cursor"`).
    pub harness: Option<&'a str>,
    /// The identity's current lock `source_folder`.
    pub installed: Option<&'a str>,
    /// Hard-pinned folder from a subpath URL; nothing else qualifies.
    pub pinned: Option<&'a str>,
}

/// The single ranking table for choosing among same-identity copies. Lower
/// wins; ties break on folder path so the choice never depends on
/// filesystem iteration order. `None` excludes the folder.
///
/// | request | order |
/// |---|---|
/// | default | root → `skills/`·`source/skills/` → `.agents/skills/` → manifest container → other |
/// | harness `h` | `h/skills/` → `h` → other under `h` → catalog → installed → `.agents/skills/` → manifest → other (root excluded) |
/// | pinned `p` | only `p` |
fn copy_rank(folder: &str, req: CopyRequest<'_>, manifest_dirs: &[String]) -> Option<u8> {
    if let Some(pinned) = req.pinned {
        return (folder == pinned).then_some(0);
    }
    let in_manifest = || {
        folder
            .rsplit_once('/')
            .is_some_and(|(parent, _)| manifest_dirs.iter().any(|dir| dir == parent))
    };
    let shared = folder.starts_with(".agents/skills/");
    let Some(prefix) = req.harness else {
        return Some(if folder.is_empty() {
            0
        } else if is_canonical_skill_folder(folder) {
            1
        } else if shared {
            2
        } else if in_manifest() {
            3
        } else {
            4
        });
    };
    if folder.is_empty() {
        return None;
    }
    Some(if folder.starts_with(&format!("{prefix}/skills/")) {
        0
    } else if folder == prefix {
        1
    } else if folder_matches_harness(folder, prefix) {
        2
    } else if is_canonical_skill_folder(folder) {
        3
    } else if req.installed == Some(folder) {
        4
    } else if shared {
        5
    } else if in_manifest() {
        6
    } else {
        7
    })
}

/// Pick the copy `req` asks for from same-identity candidates.
pub(crate) fn choose_copy<'a, T>(
    copies: &'a [T],
    folder: impl Fn(&T) -> &str,
    req: CopyRequest<'_>,
    manifest_dirs: &[String],
) -> Option<&'a T> {
    copies
        .iter()
        .filter_map(|copy| {
            let path = folder(copy);
            copy_rank(path, req, manifest_dirs).map(|rank| (rank, path.to_string(), copy))
        })
        .min_by(|left, right| (left.0, &left.1).cmp(&(right.0, &right.1)))
        .map(|(_, _, copy)| copy)
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
    ("devin", ".devin"),
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
    ("workbuddy", ".workbuddy"),
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
    if let Some(prefix) = global_skills_dir.and_then(hidden_skills_parent) {
        return Some(prefix);
    }
    if let Some(prefix) = project_skills_rel.and_then(hidden_skills_parent) {
        return Some(prefix);
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
    fn harness_copies_are_hidden_agent_trees() {
        assert!(is_harness_skill_folder(".cursor/skills/rust"));
        assert!(is_harness_skill_folder(".dsh/skills/impeccable"));
        assert!(is_harness_skill_folder(".dsh"));
        assert!(!is_harness_skill_folder("skills/rust"));
        assert!(!is_harness_skill_folder("source/skills/rust"));
        assert!(!is_harness_skill_folder(""));
        assert!(!is_harness_skill_folder("examples/writer"));
    }

    fn pick<'a>(
        copies: &'a [String],
        req: CopyRequest<'_>,
        manifest: &[String],
    ) -> Option<&'a str> {
        choose_copy(copies, |copy| copy.as_str(), req, manifest).map(String::as_str)
    }

    /// The whole table, driven by the impeccable-shaped fixture registry.
    #[test]
    fn copy_selection_table() {
        let copies = crate::pack_fixture::published_copies();
        let manifest = vec!["plugin/skills".to_string(), ".claude/skills".to_string()];
        let harness = |prefix| CopyRequest {
            harness: Some(prefix),
            ..CopyRequest::default()
        };
        let rows: &[(CopyRequest<'_>, Option<&str>)] = &[
            (CopyRequest::default(), Some(".agents/skills/impeccable")),
            (harness(".cursor"), Some(".cursor/skills/impeccable")),
            (harness(".agent"), Some(".agent/skills/impeccable")),
            (harness(".agents"), Some(".agents/skills/impeccable")),
            (harness(".windsurf"), Some(".agents/skills/impeccable")),
            (
                CopyRequest {
                    harness: Some(".windsurf"),
                    installed: Some(".dsh/skills/impeccable"),
                    pinned: None,
                },
                Some(".dsh/skills/impeccable"),
            ),
            (
                CopyRequest {
                    pinned: Some("plugin/skills/impeccable"),
                    ..harness(".cursor")
                },
                Some("plugin/skills/impeccable"),
            ),
            (
                CopyRequest {
                    pinned: Some("nowhere/impeccable"),
                    ..CopyRequest::default()
                },
                None,
            ),
        ];
        for (req, expected) in rows {
            assert_eq!(pick(&copies, *req, &manifest), *expected, "{req:?}");
        }
    }

    #[test]
    fn default_order_is_root_catalog_agents_manifest_other() {
        let manifest = vec!["plugin/skills".to_string()];
        let mut copies: Vec<String> = [
            "",
            "skills/x",
            ".agents/skills/x",
            "plugin/skills/x",
            ".claude/skills/x",
        ]
        .map(String::from)
        .to_vec();
        for expected in [
            "",
            "skills/x",
            ".agents/skills/x",
            "plugin/skills/x",
            ".claude/skills/x",
        ] {
            assert_eq!(
                pick(&copies, CopyRequest::default(), &manifest),
                Some(expected)
            );
            copies.retain(|copy| copy != expected);
        }
    }

    #[test]
    fn harness_request_never_selects_repo_root() {
        let copies = vec![String::new()];
        let req = CopyRequest {
            harness: Some(".dsh"),
            ..CopyRequest::default()
        };
        assert_eq!(pick(&copies, req, &[]), None);
    }

    #[test]
    fn ties_break_lexicographically() {
        let forward = vec![".kiro/skills/x".to_string(), ".claude/skills/x".to_string()];
        let backward: Vec<String> = forward.iter().rev().cloned().collect();
        for copies in [forward, backward] {
            assert_eq!(
                pick(&copies, CopyRequest::default(), &[]),
                Some(".claude/skills/x")
            );
        }
    }

    #[test]
    fn ignored_dirs_match_ancestors_only() {
        assert!(is_under_ignored_dir(
            "tests/oracle/workspaces/ctx-pin/.claude/skills/audit"
        ));
        assert!(is_under_ignored_dir("node_modules/pkg/skills/x"));
        assert!(!is_under_ignored_dir("skills/test"));
        assert!(!is_under_ignored_dir("fixtures"));
        assert!(!is_under_ignored_dir(".claude/skills/impeccable"));
    }

    #[test]
    fn identity_prefers_frontmatter_name() {
        assert_eq!(
            identity_key(Some(" Impeccable "), ".cursor/skills/x"),
            "impeccable"
        );
        assert_eq!(identity_key(Some(""), ".cursor/skills/Rust"), "rust");
        assert_eq!(identity_key(None, "skills/rust"), "rust");
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
        // Devin's global skills dir sits under the shared `~/.config`, so the
        // hidden-parent fallback alone cannot name its pack folder.
        assert_eq!(
            pack_harness_prefix("devin", Some("~/.config/devin/skills"), None).as_deref(),
            Some(".devin")
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
