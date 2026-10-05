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

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// Directory names whose subtrees are never scanned for skills (build
/// outputs, dependency trees, test fixtures). A directory with one of these
/// names may itself be a skill; nothing below it is scanned.
pub const IGNORED_DIR_NAMES: &[&str] = &[
    "node_modules", ".git", "dist", "build", "out", "target", "vendor", "__pycache__",
    "__pypackages__", ".venv", "venv", "tests", "test", "__tests__", "fixtures", "e2e",
    "examples", "example",
];

// ── Data Types ──────────────────────────────────────────────────────

/// A skill discovered inside a cloned repository.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiscoveredSkill {
    pub id: String,
    pub folder_path: String,
    pub description: String,
    pub already_installed: bool,
    /// Whether the shared frontmatter validator permits installation.
    pub installable: bool,
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
            installable: self.frontmatter.installable,
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
    /// Restrict scanning to this repo-relative subtree; `folder_path` stays
    /// relative to `repo_dir` (the scope prefix falls out of `strip_prefix`
    /// in [`Self::skill_candidate`] for free, so callers never re-concatenate it).
    scope: Option<&'a str>,
}

impl<'a> SkillDiscovery<'a> {
    pub fn new(repo_dir: &'a Path, full_depth: bool) -> Self {
        Self {
            repo_dir,
            full_depth,
            scope: None,
        }
    }

    /// Scan only `scope` (a repo-relative subpath). `discover()` still
    /// reports `folder_path` relative to the repository root.
    pub fn within(mut self, scope: &'a str) -> Self {
        self.scope = Some(scope);
        self
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

    /// The directory scanning actually starts from: `repo_dir` joined with
    /// `scope` when set. `None` when `scope` escapes `repo_dir` (traversal or
    /// a broken symlink) — the safety check is canonical, but the returned
    /// path is not, so `skill_candidate`'s `strip_prefix(self.repo_dir)` still
    /// works when `repo_dir` itself is not canonical (e.g. under a symlinked
    /// temp dir).
    fn scan_root(&self) -> Option<PathBuf> {
        let Some(scope) = self.scope.filter(|scope| !scope.is_empty()) else {
            return Some(self.repo_dir.to_path_buf());
        };
        let root = self.repo_dir.canonicalize().ok()?;
        let candidate = self.repo_dir.join(scope);
        let canonical = candidate.canonicalize().ok()?;
        canonical.starts_with(&root).then_some(candidate)
    }

    fn selected_skill_md_paths(&self) -> Vec<PathBuf> {
        let Some(root) = self.scan_root() else {
            return Vec::new();
        };
        if self.full_depth {
            return find_all_skill_md_files(&root);
        }

        let priority_results = scan_priority_skill_dirs(&root);
        if priority_results.is_empty() {
            find_all_skill_md_files(&root)
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


    /// Normal (non-full-depth) mode is root-first, vercel parity: a repo-root
    /// `SKILL.md` is the skill and nested copies are not scanned further.
    /// Full depth sees everything.
    fn limit_to_root_candidate(&self, candidates: Vec<SkillCandidate>) -> Vec<SkillCandidate> {
        let Some(root_skill) = candidates
            .iter()
            .find(|candidate| candidate.is_repo_root())
            .cloned()
        else {
            return candidates;
        };
        vec![root_skill]
    }

    fn finalize(&self, discovered: Vec<DiscoveredSkill>) -> Vec<DiscoveredSkill> {
        let manifest_dirs = crate::plugin_manifest::declared_skill_dir_names(self.repo_dir);
        let mut deduped = dedupe_discovered_skills(discovered, &manifest_dirs);
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
    ".cursor/skills",
    ".devin/skills",
    ".dsh/skills",
    ".factory/skills",
    ".gemini/skills",
    ".github/skills",
    ".goose/skills",
    ".grok/skills",
    ".iflow/skills",
    ".junie/skills",
    ".kilo/skills",
    ".kilocode/skills",
    ".kiro/skills",
    ".kimchi/skills",
    ".kode/skills",
    ".mcpjam/skills",
    ".minimax/skills",
    ".mux/skills",
    ".neovate/skills",
    ".omp/skills",
    ".opencode/skills",
    ".openhands/skills",
    ".pi/skills",
    ".pochi/skills",
    ".posit/assistant/skills",
    ".qoder/skills",
    ".qwen/skills",
    ".roo/skills",
    ".trae/skills",
    ".vibe/skills",
    ".windsurf/skills",
    ".workbuddy/skills",
    ".zcode/skills",
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

/// One skill per identity, first-seen in priority order (vercel parity).
///
/// Rank = (priority-container index, folder depth, path): `skills/foo`
/// shadows `.claude/skills/foo`; inside one container the shallower folder
/// wins; ties break lexicographically for determinism.
pub fn dedupe_discovered_skills(
    skills: Vec<DiscoveredSkill>,
    _manifest_dirs: &[String],
) -> Vec<DiscoveredSkill> {
    let mut order: Vec<String> = Vec::new();
    let mut groups: HashMap<String, DiscoveredSkill> = HashMap::new();
    for skill in skills {
        let key = skill.id.to_lowercase();
        match groups.get_mut(&key) {
            None => {
                order.push(key.clone());
                groups.insert(key, skill);
            }
            Some(current) => {
                if priority_rank(&skill.folder_path) < priority_rank(&current.folder_path) {
                    *current = skill;
                }
            }
        }
    }
    order
        .into_iter()
        .filter_map(|key| groups.remove(&key))
        .collect()
}

/// Sort key implementing "first seen in priority order wins".
fn priority_rank(folder_path: &str) -> (usize, usize, String) {
    let container = PRIORITY_SKILL_DIRS
        .iter()
        .position(|dir| {
            *dir == "."
                || folder_path == dir.trim_end_matches('/')
                || folder_path
                    .strip_prefix(&format!("{dir}/"))
                    .is_some_and(|rest| !rest.is_empty())
        })
        .unwrap_or(PRIORITY_SKILL_DIRS.len());
    let depth = folder_path.split('/').filter(|s| !s.is_empty()).count();
    (container, depth, folder_path.to_string())
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

// ── Filesystem Scanning ───────────────────────────────────────────────

/// Find all SKILL.md files using a full recursive scan.
///
/// An ignored directory ([`IGNORED_DIR_NAMES`]) may itself be a
/// Skill (`skills/test`), but nothing below it is scanned.
pub fn find_all_skill_md_files(dir: &Path) -> Vec<PathBuf> {
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
                if !IGNORED_DIR_NAMES.contains(&&*name_str) {
                    stack.push(path);
                } else if is_safe_skill_manifest(&path.join("SKILL.md")) {
                    results.push(path.join("SKILL.md"));
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
    installable: bool,
    /// Frontmatter quality issue codes (see `validation`).
    issues: Vec<String>,
}

fn extract_frontmatter(skill_md_path: &Path) -> SkillFrontmatter {
    // Delegate to the shared validation parser so discovery and the install
    // gate always agree on what a valid skill is.
    let report = crate::validation::inspect_skill_frontmatter(
        skill_md_path.parent().unwrap_or_else(|| Path::new(".")),
    );
    let installable = report.is_installable();
    SkillFrontmatter {
        name: report.name,
        description: report.description.unwrap_or_default(),
        installable,
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
