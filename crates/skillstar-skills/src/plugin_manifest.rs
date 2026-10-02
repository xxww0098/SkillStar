//! Claude Code plugin manifest discovery (`.claude-plugin/`).
//!
//! Claude Code plugin marketplaces declare skills in
//! `.claude-plugin/marketplace.json` (multi-plugin catalog) or
//! `.claude-plugin/plugin.json` (single plugin). Repos in that ecosystem
//! frequently place skills at paths the standard container scan never covers
//! (per-plugin `skills` arrays), so repo scans also honor declared skill
//! paths. Mirrors `npx skills` plugin-manifest handling (see
//! `skills/src/plugin-manifest.ts`).
//!
//! Only local, `./`-prefixed paths are honored; remote plugin sources are
//! skipped, and every resolved path must stay inside the repository root
//! (path-traversal guard). SkillStar never executes plugin install logic — it
//! only reads skill locations from the manifest.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// Conventional `./`-prefix requirement for manifest paths.
fn is_valid_relative_path(path: &str) -> bool {
    path.starts_with("./")
}

/// Join a `./`-prefixed `target` onto a repo-relative `base`, rejecting
/// `..` escapes so the result always stays inside the repository.
fn safe_relative_child(base: &str, target: &str) -> Option<String> {
    if !is_valid_relative_path(target) {
        return None;
    }
    let mut parts: Vec<&str> = Vec::new();
    if !base.is_empty() {
        parts.push(base);
    }
    for segment in target.trim_start_matches("./").split('/') {
        match segment {
            "" | "." => continue,
            ".." => return None,
            other => parts.push(other),
        }
    }
    Some(parts.join("/"))
}

#[derive(Debug, Deserialize, Default)]
struct MarketplaceManifest {
    #[serde(default)]
    metadata: MarketplaceMetadata,
    #[serde(default)]
    plugins: Vec<PluginEntry>,
}

#[derive(Debug, Deserialize, Default)]
struct MarketplaceMetadata {
    #[serde(default, rename = "pluginRoot")]
    plugin_root: Option<String>,
}

#[derive(Debug, Deserialize, Default)]
struct PluginEntry {
    #[serde(default)]
    source: Option<serde_json::Value>,
    #[serde(default)]
    skills: Vec<String>,
}

#[derive(Debug, Deserialize, Default)]
struct PluginManifest {
    #[serde(default)]
    skills: PathList,
}

/// Claude plugin manifests use either `"./skills/"` or `["./skills/rust"]`.
/// The string form names a **container** of skills; array entries name the
/// skills themselves.
#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum PathList {
    Container(String),
    Skills(Vec<String>),
}

impl Default for PathList {
    fn default() -> Self {
        Self::Skills(Vec::new())
    }
}

/// Collect the skill container directories declared by plugin manifests.
///
/// Each returned path is the *parent* of a declared skill path (or the
/// conventional `<plugin>/skills` directory), so the caller's depth-1 scan
/// finds the skill's own `SKILL.md` as a direct child — the same semantics
/// `npx skills` applies to manifest-declared paths.
pub fn declared_skill_dirs(repo_dir: &Path) -> Vec<PathBuf> {
    declared_skill_dir_names(repo_dir)
        .into_iter()
        .map(|dir| repo_dir.join(dir))
        .collect()
}

/// Repo-relative form of [`declared_skill_dirs`], read from disk.
pub fn declared_skill_dir_names(repo_dir: &Path) -> Vec<String> {
    let marketplace =
        std::fs::read_to_string(repo_dir.join(".claude-plugin/marketplace.json")).ok();
    let plugin = std::fs::read_to_string(repo_dir.join(".claude-plugin/plugin.json")).ok();
    declared_skill_dir_strings(marketplace.as_deref(), plugin.as_deref())
}

/// Repo-relative declared skill container dirs, from manifest file contents.
///
/// Same semantics as [`declared_skill_dirs`], but content-driven so callers
/// that hold a treeless partial clone can read the manifests as blobs before
/// any directory has been materialized.
pub fn declared_skill_dir_strings(
    marketplace_json: Option<&str>,
    plugin_json: Option<&str>,
) -> Vec<String> {
    let mut dirs = Vec::new();

    let add_plugin_skills = |dirs: &mut Vec<String>, base: &str, skills: &[String]| {
        for skill_path in skills {
            if let Some(child) = safe_relative_child(base, skill_path)
                && let Some((parent, _)) = child.rsplit_once('/')
            {
                // Parent of the declared skill path so a depth-1 scan of that
                // parent finds the skill's SKILL.md as a direct child.
                dirs.push(parent.to_string());
            }
        }
        // Conventional per-plugin skills/ directory is always discoverable.
        if base.is_empty() {
            dirs.push("skills".to_string());
        } else {
            dirs.push(format!("{base}/skills"));
        }
    };

    // marketplace.json — multi-plugin catalog.
    if let Some(content) = marketplace_json
        && let Ok(manifest) = serde_json::from_str::<MarketplaceManifest>(content)
    {
        let plugin_root = manifest
            .metadata
            .plugin_root
            .as_deref()
            .filter(|root| is_valid_relative_path(root))
            .and_then(|root| safe_relative_child("", root));
        for plugin in manifest.plugins {
            // Remote sources (object with `source`/`repo`) are skipped;
            // only local string paths are honored.
            let Some(source) = plugin.source.as_ref().and_then(|value| value.as_str()) else {
                continue;
            };
            if !is_valid_relative_path(source) {
                continue;
            }
            let base = match plugin_root.as_deref() {
                Some(root) => safe_relative_child(root, source)
                    .or_else(|| Some(format!("{root}/{}", source.trim_start_matches("./")))),
                None => safe_relative_child("", source),
            };
            if let Some(base) = base {
                add_plugin_skills(&mut dirs, &base, &plugin.skills);
            }
        }
    }

    // plugin.json — single plugin at the repo root.
    if let Some(content) = plugin_json
        && let Ok(manifest) = serde_json::from_str::<PluginManifest>(content)
    {
        match manifest.skills {
            PathList::Container(container) => {
                dirs.extend(safe_relative_child("", &container));
                add_plugin_skills(&mut dirs, "", &[]);
            }
            PathList::Skills(skills) => add_plugin_skills(&mut dirs, "", &skills),
        }
    }

    dirs
}

/// A declared Claude Code plugin whose `hooks`/`agents` SkillStar will not
/// install — skills-only install; see the README non-goals for why (own
/// implementation would duplicate `impeccable`'s own installer).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct PluginHint {
    pub hooks: bool,
    pub agents: bool,
}

/// `paths` is every tracked file path in the checkout at the target
/// revision (`git_ops::list_tree_paths`, a `git ls-tree` over the commit —
/// exact even under a sparse checkout, since tree objects are known
/// independent of what is materialized on disk). A tarball-synthesized
/// commit only lists the fetched plan directory, so the hint can
/// under-report there; that only ever loses the hint, never blocks install.
///
/// `None` when the repo does not declare a `.claude-plugin/*.json` manifest,
/// or declares one with no `hooks`/`agents` for SkillStar to skip.
pub fn plugin_hint<'a>(paths: impl Iterator<Item = &'a str>) -> Option<PluginHint> {
    let mut is_plugin = false;
    let mut hooks = false;
    let mut agents = false;
    for path in paths {
        if path == ".claude-plugin/marketplace.json" || path == ".claude-plugin/plugin.json" {
            is_plugin = true;
        }
        hooks |= has_dir_segment(path, "hooks");
        agents |= has_dir_segment(path, "agents");
    }
    (is_plugin && (hooks || agents)).then_some(PluginHint { hooks, agents })
}

fn has_dir_segment(path: &str, name: &str) -> bool {
    path.rsplit_once('/')
        .is_some_and(|(dir, _file)| dir.split('/').any(|segment| segment == name))
}

/// [`plugin_hint`] over a checkout's tracked paths, read via `git ls-tree`
/// (`crate::git::ops::list_tree_paths`). A read failure yields `None` — the
/// hint is advisory, never worth failing a scan or install over.
pub fn plugin_hint_for_repo(repo_dir: &Path) -> Option<PluginHint> {
    let paths = crate::git::ops::list_tree_paths(repo_dir).ok()?;
    plugin_hint(paths.iter().map(String::as_str))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(path: &Path, content: &str) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, content).unwrap();
    }

    #[test]
    fn marketplace_json_declared_skills_are_collected() {
        let repo = tempfile::tempdir().unwrap();
        write(
            &repo.path().join(".claude-plugin/marketplace.json"),
            r#"{
              "metadata": { "pluginRoot": "./plugins" },
              "plugins": [
                { "name": "review", "source": "./review", "skills": ["./skills/review", "./skills/test"] },
                { "name": "remote", "source": { "source": "github.com/org/repo", "repo": "x" } }
              ]
            }"#,
        );
        write(
            &repo.path().join("plugins/review/skills/review/SKILL.md"),
            "# R\n",
        );
        write(
            &repo.path().join("plugins/review/skills/test/SKILL.md"),
            "# T\n",
        );

        let dirs = declared_skill_dirs(repo.path());
        let skills_dir = dirs
            .iter()
            .find(|d| d.ends_with("plugins/review/skills"))
            .expect("declared skills parent collected");
        assert!(skills_dir.join("review/SKILL.md").exists());
        assert!(skills_dir.join("test/SKILL.md").exists());
        // Remote source must not appear.
        assert!(!dirs.iter().any(|d| d.to_string_lossy().contains("remote")));
    }

    #[test]
    fn plugin_json_at_root_is_honored() {
        let repo = tempfile::tempdir().unwrap();
        write(
            &repo.path().join(".claude-plugin/plugin.json"),
            r#"{ "skills": ["./skills/alpha"] }"#,
        );
        write(&repo.path().join("skills/alpha/SKILL.md"), "# A\n");

        let dirs = declared_skill_dirs(repo.path());
        assert!(dirs.iter().any(|d| d.ends_with("skills")));
        let skills_dir = dirs.iter().find(|d| d.ends_with("skills")).unwrap();
        assert!(skills_dir.join("alpha/SKILL.md").exists());
    }

    #[test]
    fn traversal_and_bad_paths_are_rejected() {
        let repo = tempfile::tempdir().unwrap();
        write(
            &repo.path().join(".claude-plugin/plugin.json"),
            r#"{ "skills": ["../outside", "absolute", "/etc/passwd"] }"#,
        );

        let dirs = declared_skill_dirs(repo.path());
        // Only the conventional skills/ dir survives (no SKILL.md inside).
        assert_eq!(dirs.len(), 1);
        assert!(dirs[0].ends_with("skills"));
    }

    /// The string form is a container path. It used to be treated as one
    /// skill path (parent taken), which turned `./.claude/skills/` into
    /// `.claude`; `./skills/` only passed via the conventional `skills` dir.
    #[test]
    fn plugin_json_string_is_a_container_path() {
        let dirs = declared_skill_dir_strings(None, Some(r#"{ "skills": "./.claude/skills/" }"#));
        assert!(dirs.contains(&".claude/skills".to_string()), "{dirs:?}");
        assert!(!dirs.contains(&".claude".to_string()), "{dirs:?}");

        let repo = tempfile::tempdir().unwrap();
        write(
            &repo.path().join(".claude-plugin/plugin.json"),
            r#"{ "skills": "./.claude/skills/" }"#,
        );
        write(
            &repo.path().join(".claude/skills/rust/SKILL.md"),
            "# rust\n",
        );
        let dirs = declared_skill_dirs(repo.path());
        let container = dirs
            .iter()
            .find(|dir| dir.ends_with(".claude/skills"))
            .expect("container declared");
        assert!(container.join("rust/SKILL.md").exists());
    }

    #[test]
    fn plugin_json_array_entries_are_skill_paths() {
        let dirs = declared_skill_dir_strings(
            None,
            Some(r#"{ "skills": ["./plugins/one/alpha", "./beta"] }"#),
        );
        assert!(dirs.contains(&"plugins/one".to_string()), "{dirs:?}");
        assert!(!dirs.contains(&"plugins/one/alpha".to_string()), "{dirs:?}");
    }

    #[test]
    fn missing_manifests_yield_only_conventional_dirs() {
        let repo = tempfile::tempdir().unwrap();
        let dirs = declared_skill_dirs(repo.path());
        assert!(dirs.is_empty());
    }

    #[test]
    fn plugin_hint_detects_hooks_and_agents() {
        let paths = [
            ".claude-plugin/marketplace.json",
            ".claude-plugin/plugin.json",
            "plugin/skills/impeccable/SKILL.md",
            "plugin/hooks/hooks.json",
            "plugin/agents/impeccable-reviewer.md",
        ];
        let hint = plugin_hint(paths.into_iter()).expect("plugin with hooks and agents");
        assert!(hint.hooks);
        assert!(hint.agents);
    }

    #[test]
    fn plugin_hint_is_none_without_manifest() {
        // Folders literally named hooks/agents, but no `.claude-plugin/*.json`
        // manifest declaring them: not a Claude plugin, no hint.
        let paths = ["hooks/pre-commit.sh", "agents/README.md"];
        assert!(plugin_hint(paths.into_iter()).is_none());

        // A manifest with neither hooks nor agents: nothing to skip.
        let paths = [".claude-plugin/plugin.json", "skills/demo/SKILL.md"];
        assert!(plugin_hint(paths.into_iter()).is_none());
    }

    #[test]
    fn string_version_matches_disk_version_shape() {
        let marketplace = r#"{
          "metadata": { "pluginRoot": "./plugins" },
          "plugins": [
            { "name": "review", "source": "./review", "skills": ["./skills/review"] },
            { "name": "remote", "source": { "source": "github.com/org/repo", "repo": "x" } }
          ]
        }"#;
        let dirs = declared_skill_dir_strings(Some(marketplace), None);
        assert!(dirs.contains(&"plugins/review/skills".to_string()));
        // Remote sources never contribute.
        assert!(!dirs.iter().any(|dir| dir.contains("github.com")));

        let plugin = r#"{ "skills": ["./skills/alpha", "../outside"] }"#;
        let dirs = declared_skill_dir_strings(None, Some(plugin));
        assert_eq!(dirs, vec!["skills".to_string(), "skills".to_string()]);

        assert!(declared_skill_dir_strings(None, None).is_empty());
    }
}
