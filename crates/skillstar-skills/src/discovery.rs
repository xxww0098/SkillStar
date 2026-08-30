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
//! This matches `npx skills add` behavior, with one pack-layout exception:
//! a repo-root `SKILL.md` that merely mirrors `skills/<name>/` (same
//! identity) is a one-level-scanner shim, not the install unit. See
//! `pack_layout`.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

pub use crate::pack_layout::source_priority;

// ── Data Types ──────────────────────────────────────────────────────

/// A skill discovered inside a cloned repository.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiscoveredSkill {
    pub id: String,
    pub folder_path: String,
    pub description: String,
    pub already_installed: bool,
    /// Frontmatter quality issues (stable snake_case codes), empty when the
    /// SKILL.md is a valid skill. Advisory issues (e.g. missing `name`) are
    /// listed here too; blocking ones make the skill un-installable.
    #[serde(default)]
    pub frontmatter_issues: Vec<String>,
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

    #[cfg(test)]
    pub fn root_first() -> Self {
        Self {
            mode: DiscoveryMode::RootFirst,
        }
    }

    #[cfg(test)]
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
struct SkillCandidate {
    folder_path: String,
    default_name: String,
    frontmatter: SkillFrontmatter,
}

impl SkillCandidate {
    fn discovered_skill(self) -> DiscoveredSkill {
        DiscoveredSkill {
            id: self.identity(),
            folder_path: self.folder_path,
            description: self.frontmatter.description,
            already_installed: false,
            frontmatter_issues: self.frontmatter.issues,
        }
    }

    fn identity(&self) -> String {
        self.frontmatter
            .name
            .as_deref()
            .map(str::trim)
            .filter(|name| !name.is_empty())
            .map(str::to_string)
            .unwrap_or_else(|| self.default_name.clone())
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
            folder_path,
            default_name,
        })
    }

    fn normalize_candidates(&self, candidates: Vec<SkillCandidate>) -> Vec<DiscoveredSkill> {
        let candidates = Self::strip_root_pack_shim(candidates);
        let candidates = match self.config.mode() {
            DiscoveryMode::RootFirst => self.limit_to_root_candidate(candidates),
            DiscoveryMode::FullDepth => candidates,
        };

        candidates
            .into_iter()
            .map(SkillCandidate::discovered_skill)
            .collect()
    }

    /// Drop a repo-root SKILL.md that only exists so one-level scanners can
    /// load the pack. The canonical nested skill is the install unit.
    fn strip_root_pack_shim(candidates: Vec<SkillCandidate>) -> Vec<SkillCandidate> {
        let Some(root_id) = candidates
            .iter()
            .find(|candidate| candidate.is_repo_root())
            .map(SkillCandidate::identity)
        else {
            return candidates;
        };
        let has_canonical_twin = candidates.iter().any(|candidate| {
            !candidate.is_repo_root()
                && candidate.identity().eq_ignore_ascii_case(&root_id)
                && crate::pack_layout::is_canonical_skill_folder(&candidate.folder_path)
        });
        if has_canonical_twin {
            candidates
                .into_iter()
                .filter(|candidate| !candidate.is_repo_root())
                .collect()
        } else {
            candidates
        }
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
        deduped.sort_by_key(|a| a.id.to_lowercase());
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
    ".agent/skills",
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
    ".omp/skills",
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

/// How deep known skill container directories are walked. Matches `npx skills`
/// (`DEFAULT_SKILL_CONTAINER_DEPTH`): container dirs cover flat layouts
/// (`skills/<name>/SKILL.md`) and catalog layouts one or two category levels
/// deep (`skills/<category>/<name>/SKILL.md`,
/// `skills/<category>/<category>/<name>/SKILL.md`).
const SKILL_CONTAINER_MAX_DEPTH: usize = 3;

/// Scan priority skill directories for SKILL.md files.
///
/// Container dirs are walked up to [`SKILL_CONTAINER_MAX_DEPTH`] levels; a
/// directory that is itself a skill shadows anything nested below it. The
/// repo root (`.` entry in [`PRIORITY_SKILL_DIRS`]) keeps its depth-1
/// behavior so unrelated `SKILL.md` files (e.g. under `examples/`) are not
/// surfaced in root-first mode. Plugin-manifest-declared skill dirs are
/// scanned at their declared depth.
fn scan_priority_skill_dirs(base_dir: &Path) -> Vec<PathBuf> {
    let mut results = Vec::new();

    let root_skill_md = base_dir.join("SKILL.md");
    if is_safe_skill_manifest(&root_skill_md) {
        results.push(root_skill_md);
    }

    for &dir in PRIORITY_SKILL_DIRS {
        if dir == "." {
            continue;
        }
        let skill_dir = base_dir.join(dir);
        if !is_safe_repository_directory(base_dir, &skill_dir) {
            continue;
        }
        walk_skill_container(&skill_dir, &mut results, 1, SKILL_CONTAINER_MAX_DEPTH);
    }

    // Claude Code plugin manifests may declare skills outside the standard
    // container dirs; honor them at their declared depth.
    for declared in crate::plugin_manifest::declared_skill_dirs(base_dir) {
        if !is_safe_repository_directory(base_dir, &declared) {
            continue;
        }
        walk_skill_container(&declared, &mut results, 1, 1);
    }

    results
}

/// Walk a skill container directory, collecting `SKILL.md` files.
///
/// A child directory that itself contains a `SKILL.md` is a skill; descent
/// stops below it (shadow semantics) and at `max_depth`. Non-directories and
/// unreadable entries are skipped silently.
fn walk_skill_container(dir: &Path, results: &mut Vec<PathBuf>, depth: usize, max_depth: usize) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let Ok(file_type) = entry.file_type() else {
            continue;
        };
        if !file_type.is_dir() {
            continue;
        }
        let child = entry.path();
        let skill_md = child.join("SKILL.md");
        let is_skill = is_safe_skill_manifest(&skill_md);
        if is_skill {
            results.push(skill_md);
        }
        if is_skill || depth >= max_depth {
            continue;
        }
        walk_skill_container(&child, results, depth + 1, max_depth);
    }
}

fn is_safe_repository_directory(base_dir: &Path, directory: &Path) -> bool {
    let Ok(relative) = directory.strip_prefix(base_dir) else {
        return false;
    };
    let mut current = base_dir.to_path_buf();
    for component in relative.components() {
        let std::path::Component::Normal(component) = component else {
            return false;
        };
        current.push(component);
        let Ok(metadata) = std::fs::symlink_metadata(&current) else {
            return false;
        };
        if !metadata.file_type().is_dir() {
            return false;
        }
    }
    true
}

// ── Discovery ───────────────────────────────────────────────────────

/// Scan a directory tree for SKILL.md files and return discovered skills.
///
/// This is a **pure filesystem scan** — it does not consult the lockfile.
pub fn discover_skills(repo_dir: &Path, full_depth: bool) -> Vec<DiscoveredSkill> {
    SkillDiscovery::new(repo_dir, SkillDiscoveryConfig::new(full_depth)).discover()
}

/// Full discovery without identity deduplication, for integrity-sensitive
/// callers that must reject collisions instead of selecting one candidate.
pub fn discover_skills_without_dedup(
    repo_dir: &Path,
    full_depth: bool,
    root_default_name: Option<&str>,
) -> Vec<DiscoveredSkill> {
    let discovery = SkillDiscovery::new(repo_dir, SkillDiscoveryConfig::new(full_depth));
    let mut candidates = discovery.collect_candidates();
    if let Some(root_default_name) = root_default_name {
        for candidate in &mut candidates {
            if candidate.is_repo_root()
                && candidate
                    .frontmatter
                    .name
                    .as_deref()
                    .is_none_or(|name| name.trim().is_empty())
            {
                candidate.default_name = root_default_name.to_string();
            }
        }
    }
    let mut discovered = discovery.normalize_candidates(candidates);
    discovered.sort_by(|left, right| left.folder_path.cmp(&right.folder_path));
    discovered
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
    crate::pack_layout::discovered_folder_priority(&skill.folder_path)
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
            let Ok(file_type) = entry.file_type() else {
                continue;
            };

            if file_type.is_dir() {
                if !SKIP_DIRS.iter().any(|skip| *skip == &*name_str) {
                    stack.push(path);
                }
            } else if file_type.is_file() && name_str == "SKILL.md" && is_safe_skill_manifest(&path)
            {
                results.push(path);
            }
        }
    }

    results
}

fn is_safe_skill_manifest(path: &Path) -> bool {
    std::fs::symlink_metadata(path).is_ok_and(|metadata| {
        metadata.file_type().is_file() && metadata.len() <= crate::validation::MAX_MANIFEST_BYTES
    })
}

// ── Frontmatter Extraction ──────────────────────────────────────────

#[derive(Debug, Clone)]
struct SkillFrontmatter {
    name: Option<String>,
    description: String,
    /// Frontmatter quality issue codes (see `validation`).
    issues: Vec<String>,
}

fn extract_frontmatter(skill_md_path: &Path) -> SkillFrontmatter {
    // Delegate to the shared validation parser so discovery and the install
    // gate always agree on what a valid skill is.
    let report = crate::validation::inspect_skill_frontmatter(
        skill_md_path.parent().unwrap_or_else(|| Path::new(".")),
    );
    SkillFrontmatter {
        name: report.name,
        description: report.description.unwrap_or_default(),
        issues: report
            .issues
            .iter()
            .map(|issue| issue.as_code().to_string())
            .collect(),
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
        assert!(source_priority("skills/foo") > source_priority(".agents/skills/foo"));
        assert_eq!(
            source_priority("skills/foo"),
            source_priority("source/skills/foo")
        );
        assert!(source_priority(".agents/skills/foo") > source_priority(".claude/skills/foo"));
        // Singular `.agent/skills` (Antigravity CLI official path) ranks the
        // same as the legacy plural form.
        assert_eq!(
            source_priority(".agent/skills/foo"),
            source_priority(".agents/skills/foo")
        );
    }

    #[test]
    fn dedupe_keeps_higher_priority() {
        let skills = vec![
            DiscoveredSkill {
                id: "my-skill".to_string(),
                folder_path: ".claude/skills/my-skill".to_string(),
                description: "low priority".to_string(),
                already_installed: false,
                frontmatter_issues: Vec::new(),
            },
            DiscoveredSkill {
                id: "my-skill".to_string(),
                folder_path: "source/skills/my-skill".to_string(),
                description: "high priority".to_string(),
                already_installed: false,
                frontmatter_issues: Vec::new(),
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

    #[cfg(unix)]
    #[test]
    fn discovery_never_follows_repository_symlinks() {
        use std::os::unix::fs::symlink;

        let dir = tempfile::tempdir().unwrap();
        let repo = dir.path().join("repo");
        let outside = dir.path().join("outside");
        write_skill_md(&outside.join("nested/SKILL.md"), "secret", "outside").unwrap();
        std::fs::create_dir_all(repo.join("skills")).unwrap();
        symlink(outside.join("nested"), repo.join("skills/leak")).unwrap();
        symlink(outside.join("nested/SKILL.md"), repo.join("SKILL.md")).unwrap();

        assert!(discover_skills(&repo, true).is_empty());
        assert!(discover_skills(&repo, false).is_empty());
    }

    #[cfg(unix)]
    #[test]
    fn priority_discovery_rejects_symlinked_parent_directories() {
        use std::os::unix::fs::symlink;

        let dir = tempfile::tempdir().unwrap();
        let repo = dir.path().join("repo");
        let outside = dir.path().join("outside");
        write_skill_md(&outside.join("skills/secret/SKILL.md"), "secret", "outside").unwrap();
        std::fs::create_dir_all(repo.join(".agents")).unwrap();
        symlink(outside.join("skills"), repo.join(".agents/skills")).unwrap();

        assert!(discover_skills(&repo, false).is_empty());

        let repo_with_symlinked_root = dir.path().join("repo-parent");
        std::fs::create_dir_all(&repo_with_symlinked_root).unwrap();
        symlink(&outside, repo_with_symlinked_root.join(".agents")).unwrap();
        assert!(discover_skills(&repo_with_symlinked_root, false).is_empty());
    }

    #[test]
    fn discovery_rejects_oversized_skill_manifests() {
        let dir = tempfile::tempdir().unwrap();
        let skill_md = dir.path().join("SKILL.md");
        std::fs::write(&skill_md, vec![b'x'; 1_048_577]).unwrap();

        assert!(discover_skills(dir.path(), true).is_empty());
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
        assert_eq!(candidates[0].default_name, "repo");
        assert_eq!(candidates[0].frontmatter.name.as_deref(), Some("root-name"));
        assert_eq!(candidates[0].frontmatter.description, "root-desc");
    }
}

#[cfg(test)]
mod frontmatter_issue_tests {
    use super::*;

    #[test]
    fn discovery_reports_frontmatter_issues_on_invalid_skills() {
        let dir = tempfile::tempdir().unwrap();
        let repo = dir.path().join("owner--repo");
        std::fs::create_dir_all(&repo).unwrap();
        // Valid skill — no issues.
        std::fs::create_dir_all(repo.join("skills/valid")).unwrap();
        std::fs::write(
            repo.join("skills/valid/SKILL.md"),
            "---\nname: valid\ndescription: A valid skill\n---\n",
        )
        .unwrap();
        // Bare SKILL.md — missing frontmatter/name/description.
        std::fs::create_dir_all(repo.join("skills/bare")).unwrap();
        std::fs::write(repo.join("skills/bare/SKILL.md"), "# No frontmatter\n").unwrap();

        let skills = discover_skills(&repo, true);
        let valid = skills.iter().find(|s| s.id == "valid").unwrap();
        assert!(
            valid.frontmatter_issues.is_empty(),
            "{:?}",
            valid.frontmatter_issues
        );

        let bare = skills.iter().find(|s| s.id == "bare").unwrap();
        assert!(
            bare.frontmatter_issues
                .contains(&"missing_description".to_string())
        );
        assert!(
            bare.frontmatter_issues
                .contains(&"missing_frontmatter".to_string())
        );
    }
}

#[cfg(test)]
mod depth_and_plugin_tests {
    use super::*;

    fn write_skill_md(path: &std::path::Path, name: &str) {
        let dir = path.parent().unwrap();
        std::fs::create_dir_all(dir).unwrap();
        std::fs::write(
            path,
            format!("---\nname: {name}\ndescription: {name} description\n---\n\n# {name}\n"),
        )
        .unwrap();
    }

    #[test]
    fn root_first_discovers_catalog_layouts_inside_containers() {
        let dir = tempfile::tempdir().unwrap();
        let repo = dir.path().join("owner--repo");

        // Flat layout and a two-level catalog layout under the same container.
        write_skill_md(&repo.join("skills/flat/SKILL.md"), "flat");
        write_skill_md(
            &repo.join("skills/security/web/reviewer/SKILL.md"),
            "web-reviewer",
        );

        let skills = discover_skills(&repo, false);
        let ids: Vec<&str> = skills.iter().map(|s| s.id.as_str()).collect();
        assert!(ids.contains(&"flat"), "{ids:?}");
        assert!(ids.contains(&"web-reviewer"), "{ids:?}");
    }

    #[test]
    fn a_skill_shadows_anything_nested_below_it() {
        let dir = tempfile::tempdir().unwrap();
        let repo = dir.path().join("owner--repo");

        write_skill_md(&repo.join("skills/foo/SKILL.md"), "foo");
        write_skill_md(&repo.join("skills/foo/bar/SKILL.md"), "bar");

        let skills = discover_skills(&repo, false);
        let ids: Vec<&str> = skills.iter().map(|s| s.id.as_str()).collect();
        assert!(ids.contains(&"foo"), "{ids:?}");
        assert!(!ids.contains(&"bar"), "{ids:?}");
    }

    #[test]
    fn full_depth_still_descends_past_shadowing_skills() {
        let dir = tempfile::tempdir().unwrap();
        let repo = dir.path().join("owner--repo");

        write_skill_md(&repo.join("skills/foo/SKILL.md"), "foo");
        write_skill_md(&repo.join("skills/foo/bar/SKILL.md"), "bar");

        let skills = discover_skills(&repo, true);
        let ids: Vec<&str> = skills.iter().map(|s| s.id.as_str()).collect();
        assert!(ids.contains(&"bar"), "{ids:?}");
    }

    #[test]
    fn plugin_manifest_declared_skills_are_discovered() {
        let dir = tempfile::tempdir().unwrap();
        let repo = dir.path().join("owner--repo");
        std::fs::create_dir_all(&repo).unwrap();
        std::fs::create_dir_all(repo.join(".claude-plugin")).unwrap();

        std::fs::write(
            repo.join(".claude-plugin/marketplace.json"),
            r#"{
              "metadata": { "pluginRoot": "./plugins" },
              "plugins": [
                { "name": "review", "source": "./review", "skills": ["./skills/review"] }
              ]
            }"#,
        )
        .unwrap();
        write_skill_md(
            &repo.join("plugins/review/skills/review/SKILL.md"),
            "review",
        );

        let skills = discover_skills(&repo, false);
        let ids: Vec<&str> = skills.iter().map(|s| s.id.as_str()).collect();
        assert!(ids.contains(&"review"), "{ids:?}");
    }
}
