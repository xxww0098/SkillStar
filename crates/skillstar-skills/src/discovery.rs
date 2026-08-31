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
    full_depth: bool,
}

impl<'a> SkillDiscovery<'a> {
    pub fn new(repo_dir: &'a Path, full_depth: bool) -> Self {
        Self {
            repo_dir,
            full_depth,
        }
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
        if self.full_depth {
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
        let candidates = if self.full_depth {
            candidates
        } else {
            self.limit_to_root_candidate(candidates)
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
    ".maka/skills",
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

/// Would root-first discovery accept a `SKILL.md` at `folder_path`, judged
/// from a tree listing instead of the filesystem?
///
/// Mirrors [`scan_priority_skill_dirs`] for paths that are not checked out:
/// the folder must sit inside a priority container at most
/// [`SKILL_CONTAINER_MAX_DEPTH`] levels down, and no ancestor inside that
/// container may itself be a skill (shadowing). `tree_contains` answers
/// whether a repository-relative file path exists at the revision.
/// Plugin-manifest-declared directories are not consulted here.
pub(crate) fn is_container_skill_dir(
    folder_path: &str,
    tree_contains: impl Fn(&str) -> bool,
) -> bool {
    PRIORITY_SKILL_DIRS
        .iter()
        .filter(|container| **container != ".")
        .any(|container| {
            let Some(rest) = folder_path
                .strip_prefix(container)
                .and_then(|rest| rest.strip_prefix('/'))
            else {
                return false;
            };
            let segments: Vec<&str> = rest.split('/').collect();
            if segments.len() > SKILL_CONTAINER_MAX_DEPTH
                || segments.iter().any(|segment| segment.is_empty())
            {
                return false;
            }
            let mut ancestor = (*container).to_string();
            for segment in &segments[..segments.len() - 1] {
                ancestor.push('/');
                ancestor.push_str(segment);
                if tree_contains(&format!("{ancestor}/SKILL.md")) {
                    return false;
                }
            }
            true
        })
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
    SkillDiscovery::new(repo_dir, full_depth).discover()
}

/// Full discovery without identity deduplication, for integrity-sensitive
/// callers that must reject collisions instead of selecting one candidate.
pub fn discover_skills_without_dedup(
    repo_dir: &Path,
    full_depth: bool,
    root_default_name: Option<&str>,
) -> Vec<DiscoveredSkill> {
    let discovery = SkillDiscovery::new(repo_dir, full_depth);
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

#[cfg(test)]
mod depth_and_plugin_tests;
#[cfg(test)]
mod frontmatter_issue_tests;
#[cfg(test)]
mod tests;
