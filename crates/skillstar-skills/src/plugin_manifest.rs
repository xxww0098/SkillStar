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

use serde::Deserialize;
use std::path::{Component, Path, PathBuf};

/// Conventional `./`-prefix requirement for manifest paths.
fn is_valid_relative_path(path: &str) -> bool {
    path.starts_with("./")
}

/// Resolve `target` against `base` and require the result to stay inside
/// `base`. Rejects `..` escapes and absolute paths.
fn contained_join(base: &Path, target: &str) -> Option<PathBuf> {
    if !is_valid_relative_path(target) {
        return None;
    }
    let joined = base.join(target.trim_start_matches("./"));
    let remainder = joined.strip_prefix(base).ok()?;
    if remainder
        .components()
        .any(|component| !matches!(component, Component::Normal(_)))
    {
        return None;
    }
    Some(joined)
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
    #[serde(default, deserialize_with = "deserialize_path_list")]
    skills: Vec<String>,
}

/// Claude plugin manifests use either `"./skills/"` or `["./skills/rust"]`.
fn deserialize_path_list<'de, D>(deserializer: D) -> Result<Vec<String>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum PathList {
        One(String),
        Many(Vec<String>),
    }
    Ok(match PathList::deserialize(deserializer)? {
        PathList::One(path) => vec![path],
        PathList::Many(paths) => paths,
    })
}

/// Collect the skill container directories declared by plugin manifests.
///
/// Each returned path is the *parent* of a declared skill path (or the
/// conventional `<plugin>/skills` directory), so the caller's depth-1 scan
/// finds the skill's own `SKILL.md` as a direct child — the same semantics
/// `npx skills` applies to manifest-declared paths.
pub fn declared_skill_dirs(repo_dir: &Path) -> Vec<PathBuf> {
    let mut dirs = Vec::new();

    let add_plugin_skills = |dirs: &mut Vec<PathBuf>, plugin_base: &Path, skills: &[String]| {
        if !plugin_base.starts_with(repo_dir) {
            return;
        }
        for skill_path in skills {
            let Some(skill_dir) = contained_join(plugin_base, skill_path) else {
                continue;
            };
            // Add the parent of the declared skill path so a depth-1 scan of
            // that parent finds the skill's SKILL.md as a direct child.
            if let Some(parent) = skill_dir.parent()
                && parent.starts_with(repo_dir)
            {
                dirs.push(parent.to_path_buf());
            }
        }
        // Conventional per-plugin skills/ directory is always discoverable.
        dirs.push(plugin_base.join("skills"));
    };

    // marketplace.json — multi-plugin catalog.
    if let Ok(content) = std::fs::read_to_string(repo_dir.join(".claude-plugin/marketplace.json"))
        && let Ok(manifest) = serde_json::from_str::<MarketplaceManifest>(&content)
    {
        let plugin_root = manifest
            .metadata
            .plugin_root
            .as_deref()
            .filter(|root| is_valid_relative_path(root));
        for plugin in manifest.plugins {
            // Remote sources (object with `source`/`repo`) are skipped;
            // only local string paths are honored.
            let Some(source) = plugin.source.as_ref().and_then(|value| value.as_str()) else {
                continue;
            };
            if !is_valid_relative_path(source) {
                continue;
            }
            let base = match plugin_root {
                Some(root) => contained_join(repo_dir, root).map(|root_dir| {
                    contained_join(&root_dir, source)
                        .unwrap_or_else(|| root_dir.join(source.trim_start_matches("./")))
                }),
                None => contained_join(repo_dir, source),
            };
            if let Some(base) = base {
                add_plugin_skills(&mut dirs, &base, &plugin.skills);
            }
        }
    }

    // plugin.json — single plugin at the repo root.
    if let Ok(content) = std::fs::read_to_string(repo_dir.join(".claude-plugin/plugin.json"))
        && let Ok(manifest) = serde_json::from_str::<PluginManifest>(&content)
    {
        add_plugin_skills(&mut dirs, repo_dir, &manifest.skills);
    }

    dirs
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

    #[test]
    fn plugin_json_skills_string_is_accepted() {
        let repo = tempfile::tempdir().unwrap();
        write(
            &repo.path().join(".claude-plugin/plugin.json"),
            r#"{ "skills": "./skills/" }"#,
        );
        write(&repo.path().join("skills/rust/SKILL.md"), "# rust\n");

        let dirs = declared_skill_dirs(repo.path());
        assert!(dirs.iter().any(|dir| dir.ends_with("skills")), "{dirs:?}");
        let skills_dir = dirs.iter().find(|dir| dir.ends_with("skills")).unwrap();
        assert!(skills_dir.join("rust/SKILL.md").exists());
    }

    #[test]
    fn missing_manifests_yield_only_conventional_dirs() {
        let repo = tempfile::tempdir().unwrap();
        let dirs = declared_skill_dirs(repo.path());
        assert!(dirs.is_empty());
    }
}
