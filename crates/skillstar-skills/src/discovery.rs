//! Pure filesystem SKILL.md discovery.
//!
//! Scans a directory tree for `SKILL.md` files, extracts YAML frontmatter
//! metadata, and deduplicates skills that appear in multiple agent-specific
//! directories.
//!
//! # Scan modes
//!
//! | Mode | `full_depth=false` (normal) | `full_depth=true` (full depth) |
//! |---|---|---|
//! | Root skill | Returns root skill only | Returns root + all nested |
//! | Priority dirs | Checked first; falls back to full scan if empty | Skipped |
//! | Recursive scan | Only if priority dirs are empty | Always performed |
//!
//! This matches `npx skills add` behavior.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

// ── Data Types ──────────────────────────────────────────────────────

/// A skill discovered inside a cloned repository.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiscoveredSkill {
    pub id: String,
    pub folder_path: String,
    pub description: String,
    pub already_installed: bool,
}

/// Configures how a repository should be scanned for skills.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SkillDiscoveryConfig {
    mode: DiscoveryMode,
}

impl SkillDiscoveryConfig {
    pub fn new(full_depth: bool) -> Self {
        Self {
            mode: DiscoveryMode::from_full_depth(full_depth),
        }
    }

    pub fn root_first() -> Self {
        Self {
            mode: DiscoveryMode::RootFirst,
        }
    }

    pub fn full_depth_mode() -> Self {
        Self {
            mode: DiscoveryMode::FullDepth,
        }
    }

    pub fn mode(self) -> DiscoveryMode {
        self.mode
    }

    pub fn is_full_depth(self) -> bool {
        self.mode.is_full_depth()
    }
}

/// High-level discovery behavior for repository scans.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiscoveryMode {
    RootFirst,
    FullDepth,
}

impl DiscoveryMode {
    fn from_full_depth(full_depth: bool) -> Self {
        if full_depth {
            Self::FullDepth
        } else {
            Self::RootFirst
        }
    }

    fn is_full_depth(self) -> bool {
        matches!(self, Self::FullDepth)
    }
}

/// Internal raw discovery item before it is normalized into a public skill.
#[derive(Debug, Clone)]
#[allow(dead_code)]
struct SkillCandidate {
    skill_md_path: PathBuf,
    folder_path: String,
    default_name: String,
    frontmatter: SkillFrontmatter,
}

impl SkillCandidate {
    fn discovered_skill(self) -> DiscoveredSkill {
        let id = self
            .frontmatter
            .name
            .filter(|name| !name.trim().is_empty())
            .unwrap_or(self.default_name);

        DiscoveredSkill {
            id,
            folder_path: self.folder_path,
            description: self.frontmatter.description,
            already_installed: false,
        }
    }

    fn is_repo_root(&self) -> bool {
        self.folder_path.is_empty()
    }
}

/// Type-driven discovery pipeline that keeps collection, normalization, and
/// post-processing separate while preserving the legacy public API.
#[derive(Debug, Clone, Copy)]
pub struct SkillDiscovery<'a> {
    repo_dir: &'a Path,
    config: SkillDiscoveryConfig,
}

impl<'a> SkillDiscovery<'a> {
    pub fn new(repo_dir: &'a Path, config: SkillDiscoveryConfig) -> Self {
        Self { repo_dir, config }
    }

    pub fn discover(&self) -> Vec<DiscoveredSkill> {
        let candidates = self.collect_candidates();
        let discovered = self.normalize_candidates(candidates);
        self.finalize(discovered)
    }

    fn collect_candidates(&self) -> Vec<SkillCandidate> {
        self.selected_skill_md_paths()
            .into_iter()
            .filter_map(|skill_md_path| self.skill_candidate(skill_md_path))
            .collect()
    }

    fn selected_skill_md_paths(&self) -> Vec<PathBuf> {
        if self.config.is_full_depth() {
            return find_all_skill_md_files(self.repo_dir);
        }

        let priority_results = scan_priority_skill_dirs(self.repo_dir);
        if priority_results.is_empty() {
            find_all_skill_md_files(self.repo_dir)
        } else {
            priority_results
        }
    }

    fn skill_candidate(&self, skill_md_path: PathBuf) -> Option<SkillCandidate> {
        let skill_dir = skill_md_path.parent()?;
        let raw_folder_path = skill_dir.strip_prefix(self.repo_dir).ok()?;
        let folder_path = normalize_folder_path(raw_folder_path);
        let default_name = default_skill_name(self.repo_dir, skill_dir, &folder_path)?;

        Some(SkillCandidate {
            frontmatter: extract_frontmatter(&skill_md_path),
            skill_md_path,
            folder_path,
            default_name,
        })
    }

    fn normalize_candidates(&self, candidates: Vec<SkillCandidate>) -> Vec<DiscoveredSkill> {
        let candidates = match self.config.mode() {
            DiscoveryMode::RootFirst => self.limit_to_root_candidate(candidates),
            DiscoveryMode::FullDepth => candidates,
        };

        candidates
            .into_iter()
            .map(SkillCandidate::discovered_skill)
            .collect()
    }

    fn limit_to_root_candidate(&self, candidates: Vec<SkillCandidate>) -> Vec<SkillCandidate> {
        if let Some(root_skill) = candidates
            .iter()
            .find(|candidate| candidate.is_repo_root())
            .cloned()
        {
            vec![root_skill]
        } else {
            candidates
        }
    }

    fn finalize(&self, discovered: Vec<DiscoveredSkill>) -> Vec<DiscoveredSkill> {
        let mut deduped = dedupe_discovered_skills(discovered);
        deduped.sort_by(|a, b| a.id.to_lowercase().cmp(&b.id.to_lowercase()));
        deduped
    }
}

// ── Priority Directories ─────────────────────────────────────────────

/// Priority skill search directories, aligned with `npx skills add`.
pub const PRIORITY_SKILL_DIRS: &[&str] = &[
    ".",
    "skills",
    "skills/.curated",
    "skills/.experimental",
    "skills/.system",
    ".agents/skills",
    ".augment/skills",
    ".bob/skills",
    ".claude/skills",
    ".cline/skills",
    ".codebuddy/skills",
    ".codex/skills",
    ".commandcode/skills",
    ".continue/skills",
    ".cortex/skills",
    ".crush/skills",
    ".factory/skills",
    ".github/skills",
    ".goose/skills",
    ".iflow/skills",
    ".junie/skills",
    ".kilocode/skills",
    ".kiro/skills",
    ".kode/skills",
    ".mcpjam/skills",
    ".mux/skills",
    ".neovate/skills",
    ".opencode/skills",
    ".openhands/skills",
    ".pi/skills",
    ".pochi/skills",
    ".qoder/skills",
    ".qwen/skills",
    ".roo/skills",
    ".trae/skills",
    ".vibe/skills",
    ".windsurf/skills",
    ".zencoder/skills",
    ".adal/skills",
];

/// Scan priority skill directories for SKILL.md files.
fn scan_priority_skill_dirs(base_dir: &Path) -> Vec<PathBuf> {
    let mut results = Vec::new();

    let root_skill_md = base_dir.join("SKILL.md");
    if root_skill_md.is_file() {
        results.push(root_skill_md);
    }

    for &dir in PRIORITY_SKILL_DIRS {
        if dir == "." {
            continue;
        }
        let skill_dir = base_dir.join(dir);
        let Ok(entries) = std::fs::read_dir(&skill_dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }
            let skill_md = path.join("SKILL.md");
            if skill_md.is_file() {
                results.push(skill_md);
            }
        }
    }

    results
}

// ── Discovery ───────────────────────────────────────────────────────

/// Scan a directory tree for SKILL.md files and return discovered skills.
///
/// This is a **pure filesystem scan** — it does not consult the lockfile.
pub fn discover_skills(repo_dir: &Path, full_depth: bool) -> Vec<DiscoveredSkill> {
    SkillDiscovery::new(repo_dir, SkillDiscoveryConfig::new(full_depth)).discover()
}

// ── Deduplication ───────────────────────────────────────────────────

pub fn dedupe_discovered_skills(skills: Vec<DiscoveredSkill>) -> Vec<DiscoveredSkill> {
    let mut seen: HashMap<String, usize> = HashMap::new();
    let mut deduped: Vec<DiscoveredSkill> = Vec::with_capacity(skills.len());

    for skill in skills {
        let key = skill.id.to_lowercase();
        if let Some(&existing_idx) = seen.get(&key) {
            if discovered_skill_priority(&skill) > discovered_skill_priority(&deduped[existing_idx])
            {
                deduped[existing_idx] = skill;
            }
        } else {
            seen.insert(key, deduped.len());
            deduped.push(skill);
        }
    }

    deduped
}

fn normalize_folder_path(relative_dir: &Path) -> String {
    relative_dir
        .to_string_lossy()
        .replace('\\', "/")
        .trim_matches('/')
        .to_string()
}

fn default_skill_name(repo_dir: &Path, skill_dir: &Path, folder_path: &str) -> Option<String> {
    if folder_path.is_empty() {
        Some(default_root_skill_name(repo_dir))
    } else {
        skill_dir
            .file_name()
            .map(|name| name.to_string_lossy().to_string())
    }
}

fn default_root_skill_name(repo_dir: &Path) -> String {
    let repo_name = repo_dir
        .file_name()
        .map(|name| name.to_string_lossy().to_string())
        .unwrap_or_else(|| "skill".to_string());

    repo_name
        .split_once("--")
        .map(|(_, tail)| tail.to_string())
        .unwrap_or(repo_name)
}

fn discovered_skill_priority(skill: &DiscoveredSkill) -> u8 {
    if skill.folder_path.is_empty() {
        4
    } else {
        source_priority(&skill.folder_path)
    }
}

pub fn source_priority(folder_path: &str) -> u8 {
    if folder_path.starts_with("source/skills") || folder_path.starts_with("source\\skills") {
        3
    } else if folder_path.starts_with(".agents/skills")
        || folder_path.starts_with(".agents\\skills")
    {
        2
    } else {
        1
    }
}

// ── Filesystem Scanning ───────────────────────────────────────────────

/// Find all SKILL.md files using a full recursive scan.
pub fn find_all_skill_md_files(dir: &Path) -> Vec<PathBuf> {
    const SKIP_DIRS: &[&str] = &[
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
    ];

    let mut results = Vec::new();
    let mut stack = vec![dir.to_path_buf()];

    while let Some(current) = stack.pop() {
        let entries = match std::fs::read_dir(&current) {
            Ok(e) => e,
            Err(_) => continue,
        };

        for entry in entries.flatten() {
            let path = entry.path();
            let name = entry.file_name();
            let name_str = name.to_string_lossy();

            if path.is_dir() {
                if !SKIP_DIRS.iter().any(|skip| *skip == &*name_str) {
                    stack.push(path);
                }
            } else if name_str == "SKILL.md" {
                results.push(path);
            }
        }
    }

    results
}

// ── Frontmatter Extraction ──────────────────────────────────────────

#[derive(Debug, Clone)]
struct SkillFrontmatter {
    name: Option<String>,
    description: String,
}

fn extract_frontmatter(skill_md_path: &Path) -> SkillFrontmatter {
    let content = match std::fs::read_to_string(skill_md_path) {
        Ok(c) => c,
        Err(_) => {
            return SkillFrontmatter {
                name: None,
                description: String::new(),
            };
        }
    };

    if !content.starts_with("---") {
        return SkillFrontmatter {
            name: None,
            description: String::new(),
        };
    }

    let parts: Vec<&str> = content.splitn(3, "---").collect();
    if parts.len() < 3 {
        return SkillFrontmatter {
            name: None,
            description: String::new(),
        };
    }

    let yaml_str = parts[1];

    #[derive(Deserialize)]
    struct Frontmatter {
        name: Option<String>,
        description: Option<String>,
    }

    match serde_yaml::from_str::<Frontmatter>(yaml_str) {
        Ok(fm) => SkillFrontmatter {
            name: fm.name.map(|name| name.trim().to_string()),
            description: fm.description.unwrap_or_default().trim().to_string(),
        },
        Err(_) => SkillFrontmatter {
            name: None,
            description: String::new(),
        },
    }
}

// ── Tests ────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn write_skill_md(path: &Path, name: &str, description: &str) -> std::io::Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let content = format!("---\nname: {name}\ndescription: {description}\n---\n\n# {name}\n",);
        std::fs::write(path, content)
    }

    #[test]
    fn discover_root_first_returns_only_root_skill() {
        let dir = tempfile::tempdir().unwrap();
        let repo = dir.path().join("owner--repo");

        write_skill_md(&repo.join("SKILL.md"), "root-skill", "root").unwrap();
        write_skill_md(
            &repo.join("skills/nested-skill/SKILL.md"),
            "nested-skill",
            "nested",
        )
        .unwrap();

        let skills = discover_skills(&repo, false);
        assert_eq!(skills.len(), 1);
        assert_eq!(skills[0].id, "root-skill");
        assert!(skills[0].folder_path.is_empty());
    }

    #[test]
    fn discover_full_depth_includes_all_skills() {
        let dir = tempfile::tempdir().unwrap();
        let repo = dir.path().join("owner--repo");

        write_skill_md(&repo.join("SKILL.md"), "root-skill", "root").unwrap();
        write_skill_md(
            &repo.join("skills/nested-skill/SKILL.md"),
            "nested-skill",
            "nested",
        )
        .unwrap();

        let skills = discover_skills(&repo, true);
        assert_eq!(skills.len(), 2);
        assert!(
            skills
                .iter()
                .any(|s| s.id == "root-skill" && s.folder_path.is_empty())
        );
        assert!(
            skills
                .iter()
                .any(|s| s.id == "nested-skill" && s.folder_path == "skills/nested-skill")
        );
    }

    #[test]
    fn discover_uses_frontmatter_name() {
        let dir = tempfile::tempdir().unwrap();
        let repo = dir.path().join("owner--repo");
        write_skill_md(&repo.join("SKILL.md"), "custom-name", "desc").unwrap();

        let skills = discover_skills(&repo, false);
        assert_eq!(skills.len(), 1);
        assert_eq!(skills[0].id, "custom-name");
    }

    #[test]
    fn discover_root_default_name_keeps_repo_double_dash_segments() {
        let dir = tempfile::tempdir().unwrap();
        let repo = dir.path().join("owner--my--tool");
        std::fs::create_dir_all(&repo).unwrap();
        std::fs::write(repo.join("SKILL.md"), "# demo\n").unwrap();

        let skills = discover_skills(&repo, false);
        assert_eq!(skills.len(), 1);
        assert_eq!(skills[0].id, "my--tool");
    }

    #[test]
    fn discover_deduplicates_agent_copies() {
        let dir = tempfile::tempdir().unwrap();
        let repo = dir.path();

        write_skill_md(
            &repo.join("source/skills/my-skill/SKILL.md"),
            "my-skill",
            "canonical",
        )
        .unwrap();
        write_skill_md(
            &repo.join(".claude/skills/my-skill/SKILL.md"),
            "my-skill",
            "claude copy",
        )
        .unwrap();

        let skills = discover_skills(repo, true);
        assert_eq!(skills.len(), 1);
        assert_eq!(skills[0].id, "my-skill");
        assert!(skills[0].folder_path.starts_with("source/skills"));
    }

    #[test]
    fn discover_empty_dir_returns_empty() {
        let dir = tempfile::tempdir().unwrap();
        let skills = discover_skills(dir.path(), false);
        assert!(skills.is_empty());
    }

    #[test]
    fn source_priority_ordering() {
        assert!(source_priority("source/skills/foo") > source_priority(".agents/skills/foo"));
        assert!(source_priority(".agents/skills/foo") > source_priority(".claude/skills/foo"));
    }

    #[test]
    fn dedupe_keeps_higher_priority() {
        let skills = vec![
            DiscoveredSkill {
                id: "my-skill".to_string(),
                folder_path: ".claude/skills/my-skill".to_string(),
                description: "low priority".to_string(),
                already_installed: false,
            },
            DiscoveredSkill {
                id: "my-skill".to_string(),
                folder_path: "source/skills/my-skill".to_string(),
                description: "high priority".to_string(),
                already_installed: false,
            },
        ];
        let deduped = dedupe_discovered_skills(skills);
        assert_eq!(deduped.len(), 1);
        assert!(deduped[0].folder_path.starts_with("source/skills"));
    }

    #[test]
    fn discover_priority_dir_skips_non_standard() {
        let dir = tempfile::tempdir().unwrap();
        let repo = dir.path();

        write_skill_md(
            &repo.join("skills/opencli-browser/SKILL.md"),
            "opencli-browser",
            "browser",
        )
        .unwrap();
        write_skill_md(
            &repo.join("clis/antigravity/SKILL.md"),
            "antigravity",
            "desktop automation",
        )
        .unwrap();

        let skills = discover_skills(repo, false);
        assert_eq!(skills.len(), 1);
        assert_eq!(skills[0].id, "opencli-browser");
    }

    #[test]
    fn discover_falls_back_to_non_standard_when_no_priority_skills() {
        let dir = tempfile::tempdir().unwrap();
        let repo = dir.path();

        write_skill_md(&repo.join("custom/demo/SKILL.md"), "demo", "non-standard").unwrap();

        let skills = discover_skills(repo, false);
        assert_eq!(skills.len(), 1);
        assert_eq!(skills[0].id, "demo");
    }

    #[test]
    fn discover_full_depth_includes_non_standard() {
        let dir = tempfile::tempdir().unwrap();
        let repo = dir.path();

        write_skill_md(
            &repo.join("skills/opencli-browser/SKILL.md"),
            "opencli-browser",
            "browser",
        )
        .unwrap();
        write_skill_md(
            &repo.join("clis/antigravity/SKILL.md"),
            "antigravity",
            "desktop automation",
        )
        .unwrap();

        let skills = discover_skills(repo, true);
        assert_eq!(skills.len(), 2);
        assert!(
            skills
                .iter()
                .any(|s| s.id == "opencli-browser" && s.folder_path == "skills/opencli-browser")
        );
        assert!(
            skills
                .iter()
                .any(|s| s.id == "antigravity" && s.folder_path == "clis/antigravity")
        );
    }

    #[test]
    fn skill_discovery_config_preserves_legacy_full_depth_mapping() {
        assert_eq!(
            SkillDiscoveryConfig::new(false).mode(),
            DiscoveryMode::RootFirst
        );
        assert_eq!(
            SkillDiscoveryConfig::new(true).mode(),
            DiscoveryMode::FullDepth
        );
        assert!(!SkillDiscoveryConfig::root_first().is_full_depth());
        assert!(SkillDiscoveryConfig::full_depth_mode().is_full_depth());
    }

    #[test]
    fn skill_discovery_pipeline_matches_compatibility_api() {
        let dir = tempfile::tempdir().unwrap();
        let repo = dir.path();

        write_skill_md(&repo.join("skills/demo/SKILL.md"), "demo", "demo").unwrap();

        let from_pipeline =
            SkillDiscovery::new(repo, SkillDiscoveryConfig::root_first()).discover();
        let from_compat = discover_skills(repo, false);

        assert_eq!(from_pipeline.len(), 1);
        assert_eq!(from_pipeline[0].id, from_compat[0].id);
        assert_eq!(from_pipeline[0].folder_path, from_compat[0].folder_path);
        assert_eq!(from_pipeline[0].description, from_compat[0].description);
    }

    #[test]
    fn skill_discovery_candidate_keeps_root_path_and_frontmatter() {
        let dir = tempfile::tempdir().unwrap();
        let repo = dir.path().join("owner--repo");
        write_skill_md(&repo.join("SKILL.md"), "root-name", "root-desc").unwrap();

        let discovery = SkillDiscovery::new(&repo, SkillDiscoveryConfig::root_first());
        let candidates = discovery.collect_candidates();

        assert_eq!(candidates.len(), 1);
        assert!(candidates[0].is_repo_root());
        assert!(candidates[0].skill_md_path.ends_with("SKILL.md"));
        assert_eq!(candidates[0].default_name, "repo");
        assert_eq!(candidates[0].frontmatter.name.as_deref(), Some("root-name"));
        assert_eq!(candidates[0].frontmatter.description, "root-desc");
    }
}
