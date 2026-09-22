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
    let marketplace = std::fs::read_to_string(repo_dir.join(".claude-plugin/marketplace.json")).ok();
    let plugin = std::fs::read_to_string(repo_dir.join(".claude-plugin/plugin.json")).ok();
    declared_skill_dir_strings(marketplace.as_deref(), plugin.as_deref())
        .into_iter()
        .map(|dir| repo_dir.join(dir))
        .collect()
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
        && let Ok(manifest) = serde_json::from_str::<PluginManifest>(&content)
    {
        add_plugin_skills(&mut dirs, "", &manifest.skills);
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
        assert_eq!(
            dirs,
            vec!["skills".to_string(), "skills".to_string()]
        );

        assert!(declared_skill_dir_strings(None, None).is_empty());
    }
}
