use anyhow::{Context, Result, anyhow};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

use super::{
    git_ops, local_skill, lockfile, path_env::command_with_path, repo_history, security_scan, sync,
};

// ── Data Types ──────────────────────────────────────────────────────

/// A skill discovered inside a cloned repository.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiscoveredSkill {
    /// Skill directory name, e.g. "vercel-react"
    pub id: String,
    /// Relative path within the repo, e.g. "skills/vercel-react"
    pub folder_path: String,
    /// Description from SKILL.md YAML frontmatter
    pub description: String,
    /// Whether this skill is already installed in the hub
    pub already_installed: bool,
}

/// Result of scanning a GitHub repository.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScanResult {
    /// Short identifier, e.g. "vercel-labs/skills"
    pub source: String,
    /// Full URL, e.g. "https://github.com/vercel-labs/skills.git"
    pub source_url: String,
    /// All skills discovered in the repository
    pub skills: Vec<DiscoveredSkill>,
}

/// Target for batch install from scan results.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillInstallTarget {
    /// Skill directory name
    pub id: String,
    /// Relative path within the repo
    pub folder_path: String,
}

// ── URL Normalization ───────────────────────────────────────────────

/// Normalize user input into a full clone URL and a short source identifier.
///
/// Supported inputs:
/// - `owner/repo` → `https://github.com/owner/repo.git`
/// - `https://github.com/owner/repo` → appends `.git`
/// - `https://github.com/owner/repo.git` → as-is
pub fn normalize_repo_url(input: &str) -> Result<(String, String)> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Err(anyhow!("Repository URL cannot be empty"));
    }

    if trimmed.to_lowercase().starts_with("https://") {
        // Full HTTPS URL
        let mut source = trimmed.to_string();

        // Extract owner/repo from URL
        if source.to_lowercase().starts_with("https://github.com/") {
            source = trimmed
                .get("https://github.com/".len()..)
                .unwrap_or(trimmed)
                .to_string();
        }
        // else: Non-GitHub HTTPS URLs use the full URL as source

        // Clean up source
        if source.ends_with(".git") {
            source = source[..source.len() - 4].to_string();
        }
        if source.ends_with('/') {
            source.pop();
        }

        // Build clone URL
        let mut repo_url = trimmed.to_string();
        if repo_url.ends_with('/') {
            repo_url.pop();
        }
        if !repo_url.ends_with(".git") {
            repo_url.push_str(".git");
        }

        Ok((repo_url, source))
    } else {
        // owner/repo format
        let components: Vec<&str> = trimmed.split('/').collect();
        if components.len() != 2 || components[0].is_empty() || components[1].is_empty() {
            return Err(anyhow!(
                "Invalid repository format. Use 'owner/repo' or a full GitHub URL."
            ));
        }

        let mut repo_name = components[1].to_string();
        if repo_name.ends_with(".git") {
            repo_name = repo_name[..repo_name.len() - 4].to_string();
        }

        let source = format!("{}/{}", components[0], repo_name);
        let repo_url = format!("https://github.com/{}.git", source);

        Ok((repo_url, source))
    }
}

// ── Repo Cache ──────────────────────────────────────────────────────

/// Get the repo cache directory: `~/.skillstar/.agents/.repos/`
fn get_repos_cache_dir() -> PathBuf {
    super::paths::repos_cache_dir()
}

/// Derive a cache directory name from a source identifier.
///
/// `"vercel-labs/skills"` → `"vercel-labs--skills"`
fn cache_dir_name(source: &str) -> String {
    source.replace('/', "--")
}

/// Clone or fetch a repository into the cache.
///
/// Uses a **sparse treeless clone** to minimise network traffic and disk usage:
///
/// 1. First clone with `--filter=blob:none --depth 1 --no-checkout` — this
///    downloads only tree metadata (~200KB), not file blobs.
/// 2. Use `git ls-tree` on the tree objects to discover all `SKILL.md` locations.
/// 3. Configure sparse-checkout with only the directories containing SKILL.md.
/// 4. `git checkout` materializes just those directories.
///
/// For repos like `pbakaus/impeccable` (~20MB full), this reduces the clone to
/// ~500KB by skipping `public/`, duplicated agent dirs, tests, etc.
///
/// If the repo is already cached, runs `git fetch --depth 1` and updates the
/// working tree (sparse-checkout rules are preserved).
///
/// Falls back to a full shallow clone if sparse operations fail (e.g. old git).
pub fn clone_or_fetch_repo(repo_url: &str, source: &str) -> Result<PathBuf> {
    let cache_dir = get_repos_cache_dir();
    std::fs::create_dir_all(&cache_dir).context("Failed to create repo cache directory")?;

    let repo_dir = cache_dir.join(cache_dir_name(source));

    if repo_dir.join(".git").exists() {
        // Already cached — shallow fetch latest
        let output = command_with_path("git")
            .current_dir(&repo_dir)
            .args(["fetch", "--depth", "1", "--quiet"])
            .output()
            .context("Failed to execute git fetch")?;

        if !output.status.success() {
            let err = String::from_utf8_lossy(&output.stderr);
            // Fetch failure is non-fatal for scanning; the cached version is still usable
            eprintln!("[repo_scanner] git fetch warning: {}", err.trim());
        }

        // Reset working tree to match remote HEAD so scanned content is up-to-date
        let _ = command_with_path("git")
            .current_dir(&repo_dir)
            .args(["reset", "--hard", "origin/HEAD"])
            .output();

        // Re-discover SKILL.md dirs and update sparse-checkout (handles new skills added upstream)
        if is_sparse_checkout(&repo_dir) {
            if let Ok(dirs) = discover_skill_dirs_from_tree(&repo_dir) {
                let dir_refs: Vec<&str> = dirs.iter().map(|s| s.as_str()).collect();
                let _ = git_ops::apply_sparse_checkout(&repo_dir, &dir_refs);
            }
        }

        Ok(repo_dir)
    } else {
        // Try sparse treeless clone first, fall back to full shallow
        match clone_sparse_with_skills(repo_url, &repo_dir) {
            Ok(()) => Ok(repo_dir),
            Err(sparse_err) => {
                eprintln!(
                    "[repo_scanner] sparse clone failed, falling back to shallow: {}",
                    sparse_err
                );
                // Clean up partial sparse clone
                let _ = std::fs::remove_dir_all(&repo_dir);
                git_ops::clone_repo_shallow(repo_url, &repo_dir)
                    .with_context(|| format!("Failed to shallow-clone {}", repo_url))?;
                Ok(repo_dir)
            }
        }
    }
}

/// Clone a repo sparsely and configure checkout for only SKILL.md directories.
fn clone_sparse_with_skills(repo_url: &str, dest: &Path) -> Result<()> {
    // Phase 1: treeless clone — only tree objects, no file blobs
    git_ops::clone_repo_sparse(repo_url, dest)?;

    // Phase 2: discover SKILL.md locations from tree objects
    let skill_dirs = discover_skill_dirs_from_tree(dest)?;

    if skill_dirs.is_empty() {
        // No skills found — fall back to full checkout
        let _ = command_with_path("git")
            .current_dir(dest)
            .arg("checkout")
            .output();
        return Ok(());
    }

    // Phase 3: sparse-checkout only the needed directories
    let dir_refs: Vec<&str> = skill_dirs.iter().map(|s| s.as_str()).collect();
    git_ops::apply_sparse_checkout(dest, &dir_refs)?;

    Ok(())
}

/// Discover directories containing SKILL.md via `git ls-tree` (no checkout needed).
///
/// Returns deduplicated parent directory paths of all SKILL.md files found,
/// preferring the canonical source path (`source/skills/`) over duplicated
/// agent-specific copies.
fn discover_skill_dirs_from_tree(repo_dir: &Path) -> Result<Vec<String>> {
    let all_paths = git_ops::list_tree_paths(repo_dir)?;

    // Collect parent dirs of all SKILL.md files
    let skill_dirs: Vec<String> = all_paths
        .iter()
        .filter(|p| p.ends_with("/SKILL.md") || *p == "SKILL.md")
        .filter_map(|p| {
            // Get parent directory path
            let parent = Path::new(p).parent()?;
            let parent_str = parent.to_string_lossy().to_string();
            if parent_str.is_empty() {
                // Root-level SKILL.md — need the whole repo root
                None
            } else {
                Some(parent_str)
            }
        })
        .collect();

    if skill_dirs.is_empty() {
        // Might be a root-level SKILL.md repo
        if all_paths.iter().any(|p| p == "SKILL.md") {
            // Need the entire repo — return empty to trigger full checkout
            return Ok(Vec::new());
        }
    }

    // Deduplicate: for skills that appear in multiple agent dirs (e.g.
    // `.claude/skills/foo`, `.agents/skills/foo`, `source/skills/foo`),
    // keep only the canonical `source/skills/` or `.agents/skills/` version
    // to avoid materializing duplicates.
    let mut canonical_skills: std::collections::HashMap<String, String> =
        std::collections::HashMap::new();

    for dir in &skill_dirs {
        // Extract skill name (last path component)
        let skill_name = Path::new(dir)
            .file_name()
            .map(|n| n.to_string_lossy().to_lowercase())
            .unwrap_or_default();
        if skill_name.is_empty() {
            continue;
        }

        let priority = source_priority(dir);
        let should_replace = canonical_skills
            .get(&skill_name)
            .map(|existing| source_priority(existing) < priority)
            .unwrap_or(true);

        if should_replace {
            canonical_skills.insert(skill_name, dir.clone());
        }
    }

    // Collect unique dirs, also include any non-skill directories that might be
    // needed (but for repo scanning, skill dirs are sufficient)
    let mut result: Vec<String> = canonical_skills.into_values().collect();
    result.sort();
    result.dedup();

    // Also find common parent prefixes to reduce sparse-checkout entries.
    // e.g. if we have `source/skills/foo`, `source/skills/bar`, just use `source/skills`
    let compacted = compact_to_common_parents(&result);

    Ok(compacted)
}

/// Compact a list of directories to their common parent prefixes.
///
/// e.g. `["source/skills/foo", "source/skills/bar"]` → `["source/skills"]`
fn compact_to_common_parents(dirs: &[String]) -> Vec<String> {
    if dirs.is_empty() {
        return Vec::new();
    }

    // Group by grandparent (2 levels up from the skill dir)
    let mut parent_counts: std::collections::HashMap<String, usize> =
        std::collections::HashMap::new();
    let mut parent_to_dirs: std::collections::HashMap<String, Vec<String>> =
        std::collections::HashMap::new();

    for dir in dirs {
        if let Some(parent) = Path::new(dir).parent() {
            let parent_str = parent.to_string_lossy().to_string();
            *parent_counts.entry(parent_str.clone()).or_insert(0) += 1;
            parent_to_dirs
                .entry(parent_str)
                .or_default()
                .push(dir.clone());
        }
    }

    let mut result = Vec::new();
    let mut handled = std::collections::HashSet::new();

    for dir in dirs {
        if handled.contains(dir) {
            continue;
        }
        if let Some(parent) = Path::new(dir).parent() {
            let parent_str = parent.to_string_lossy().to_string();
            if parent_counts.get(&parent_str).copied().unwrap_or(0) >= 2 {
                // Multiple skills share this parent — use the parent
                if !handled.contains(&parent_str) {
                    result.push(parent_str.clone());
                    // Mark all children as handled
                    if let Some(children) = parent_to_dirs.get(&parent_str) {
                        for child in children {
                            handled.insert(child.clone());
                        }
                    }
                    handled.insert(parent_str);
                }
            } else {
                result.push(dir.clone());
                handled.insert(dir.clone());
            }
        } else {
            result.push(dir.clone());
            handled.insert(dir.clone());
        }
    }

    result.sort();
    result
}

/// Check if a repo directory is using sparse-checkout.
fn is_sparse_checkout(repo_dir: &Path) -> bool {
    let output = command_with_path("git")
        .current_dir(repo_dir)
        .args(["config", "--get", "core.sparseCheckout"])
        .output();
    match output {
        Ok(o) => String::from_utf8_lossy(&o.stdout).trim() == "true",
        Err(_) => false,
    }
}

// ── SKILL.md Scanning ───────────────────────────────────────────────

/// Scan a cloned repo directory for all SKILL.md files and extract metadata.
pub fn scan_skills_in_repo(repo_dir: &Path) -> Vec<DiscoveredSkill> {
    let hub_skills_dir = sync::get_hub_skills_dir();

    // Collect all SKILL.md paths
    let skill_md_paths = find_skill_md_files(repo_dir);
    let mut discovered = Vec::with_capacity(skill_md_paths.len());

    for skill_md_path in skill_md_paths {
        let skill_dir = match skill_md_path.parent() {
            Some(dir) => dir,
            None => continue,
        };

        // Derive skill name from directory name
        let skill_name = match skill_dir.file_name() {
            Some(name) => name.to_string_lossy().to_string(),
            None => continue,
        };

        // Skip if the skill directory IS the repo root (single-skill repo with SKILL.md at root)
        // In this case folder_path will be empty and we use the repo name as skill id
        let folder_path = match skill_dir.strip_prefix(repo_dir) {
            Ok(rel) => {
                let rel_str = rel.to_string_lossy().to_string();
                // Normalize: remove trailing separators
                let clean = rel_str.trim_matches('/').to_string();
                clean
            }
            Err(_) => continue,
        };

        // For root-level SKILL.md, use repo directory name as skill ID
        let (effective_name, effective_folder) = if folder_path.is_empty() {
            let repo_name = repo_dir
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| "skill".to_string());
            // Strip the cache dir name format (owner--repo → repo)
            let clean_name = if repo_name.contains("--") {
                repo_name
                    .rsplit("--")
                    .next()
                    .unwrap_or(&repo_name)
                    .to_string()
            } else {
                repo_name
            };
            (clean_name, String::new())
        } else {
            (skill_name, folder_path)
        };

        let description = extract_frontmatter_description(&skill_md_path);
        let already_installed = hub_skills_dir.join(&effective_name).exists();

        discovered.push(DiscoveredSkill {
            id: effective_name,
            folder_path: effective_folder,
            description,
            already_installed,
        });
    }

    // Deduplicate by skill id.
    // Repos like "impeccable" distribute the same skill into multiple agent directories
    // (.claude/skills/, .cursor/skills/, .gemini/skills/, .agents/skills/, source/skills/ etc).
    // We keep only one entry per id, preferring the canonical source path.
    let mut seen: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
    let mut deduped: Vec<DiscoveredSkill> = Vec::with_capacity(discovered.len());

    for skill in discovered {
        let key = skill.id.to_lowercase();
        if let Some(&existing_idx) = seen.get(&key) {
            // Replace if the new one has a higher-priority source path
            if source_priority(&skill.folder_path)
                > source_priority(&deduped[existing_idx].folder_path)
            {
                deduped[existing_idx] = skill;
            }
        } else {
            seen.insert(key, deduped.len());
            deduped.push(skill);
        }
    }

    // Sort by name
    deduped.sort_by(|a, b| a.id.to_lowercase().cmp(&b.id.to_lowercase()));
    deduped
}

/// Assign a priority to a folder path for deduplication.
/// Higher is preferred. `source/skills/` is canonical, `.agents/skills/` second best.
fn source_priority(folder_path: &str) -> u8 {
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

/// Recursively find all SKILL.md files in a directory, skipping .git.
fn find_skill_md_files(dir: &Path) -> Vec<PathBuf> {
    let mut results = Vec::new();
    find_skill_md_recursive(dir, &mut results);
    results
}

fn find_skill_md_recursive(dir: &Path, results: &mut Vec<PathBuf>) {
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return,
    };

    for entry in entries.flatten() {
        let path = entry.path();
        let name = entry.file_name();
        let name_str = name.to_string_lossy();

        // Skip .git directory
        if name_str == ".git" {
            continue;
        }

        if path.is_dir() {
            find_skill_md_recursive(&path, results);
        } else if name_str == "SKILL.md" {
            results.push(path);
        }
    }
}

/// Extract the `description` field from SKILL.md YAML frontmatter.
fn extract_frontmatter_description(skill_md_path: &Path) -> String {
    let content = match std::fs::read_to_string(skill_md_path) {
        Ok(c) => c,
        Err(_) => return String::new(),
    };

    if !content.starts_with("---") {
        return String::new();
    }

    let parts: Vec<&str> = content.splitn(3, "---").collect();
    if parts.len() < 3 {
        return String::new();
    }

    let yaml_str = parts[1];

    // Use serde_yaml to parse the frontmatter
    #[derive(Deserialize)]
    struct Frontmatter {
        description: Option<String>,
    }

    match serde_yaml::from_str::<Frontmatter>(yaml_str) {
        Ok(fm) => fm.description.unwrap_or_default().trim().to_string(),
        Err(_) => String::new(),
    }
}

// ── Batch Install ───────────────────────────────────────────────────

/// Install selected skills from a scanned repo.
///
/// For multi-skill repos, creates symlinks from the hub skills directory to the
/// cached repo's skill subfolder. For single-skill repos (SKILL.md at root),
/// the entire cached repo is the skill source.
///
/// Returns the list of successfully installed skill names.
pub fn install_from_repo(
    source: &str,
    repo_url: &str,
    targets: &[SkillInstallTarget],
) -> Result<Vec<String>> {
    let hub_skills_dir = sync::get_hub_skills_dir();
    std::fs::create_dir_all(&hub_skills_dir).context("Failed to create hub skills directory")?;

    let cache_dir = get_repos_cache_dir().join(cache_dir_name(source));
    if !cache_dir.exists() {
        return Err(anyhow!(
            "Repo cache not found. Please scan the repository first."
        ));
    }

    let _lock = lockfile::get_mutex()
        .lock()
        .map_err(|_| anyhow!("Lockfile mutex poisoned"))?;
    let lock_path = lockfile::lockfile_path();
    let mut lf = lockfile::Lockfile::load(&lock_path).unwrap_or_default();

    let mut installed_names = Vec::new();

    for target in targets {
        let dest = hub_skills_dir.join(&target.id);
        let existing_entry = lf.skills.iter().find(|entry| entry.name == target.id);

        if dest.symlink_metadata().is_ok()
            && !can_replace_existing_skill(&target.id, repo_url, existing_entry)
        {
            eprintln!(
                "[repo_scanner] refusing to replace existing skill '{}' from a different source",
                target.id
            );
            continue;
        }

        // If it already exists, remove it to allow reinstall
        if let Ok(meta) = dest.symlink_metadata() {
            if meta.is_symlink() {
                #[cfg(unix)]
                let _ = std::fs::remove_file(&dest);
                #[cfg(windows)]
                {
                    // Directory symlinks (created via symlink_dir) must use
                    // remove_dir; file symlinks must use remove_file.
                    if meta.is_dir() {
                        std::fs::remove_dir(&dest)
                    } else {
                        std::fs::remove_file(&dest)
                    }
                    .unwrap_or_else(|e| {
                        eprintln!(
                            "[repo_scanner] failed to remove symlink {}: {}",
                            dest.display(),
                            e
                        );
                    });
                }
            } else {
                let _ = std::fs::remove_dir_all(&dest);
            }
        }

        // Determine the source path within the cached repo
        let source_path = if target.folder_path.is_empty() {
            cache_dir.clone()
        } else {
            cache_dir.join(&target.folder_path)
        };

        if !source_path.exists() {
            eprintln!(
                "[repo_scanner] Skill folder not found: {}",
                source_path.display()
            );
            continue;
        }

        // Create symlink: hub/skills/<name> → .repos/<cache>/<folder>/
        #[cfg(unix)]
        std::os::unix::fs::symlink(&source_path, &dest)
            .with_context(|| format!("Failed to symlink {:?} → {:?}", source_path, dest))?;

        #[cfg(windows)]
        std::os::windows::fs::symlink_dir(&source_path, &dest)
            .with_context(|| format!("Failed to symlink {:?} → {:?}", source_path, dest))?;

        // Compute tree hash from the cached repo for this skill's subfolder
        let tree_hash = if target.folder_path.is_empty() {
            git_ops::compute_tree_hash(&cache_dir).unwrap_or_default()
        } else {
            compute_subtree_hash(&cache_dir, &target.folder_path).unwrap_or_default()
        };

        // Update lockfile
        let source_folder = if target.folder_path.is_empty() {
            None
        } else {
            Some(target.folder_path.clone())
        };

        lf.upsert(lockfile::LockEntry {
            name: target.id.clone(),
            git_url: repo_url.to_string(),
            tree_hash,
            installed_at: chrono::Utc::now().to_rfc3339(),
            source_folder,
        });

        installed_names.push(target.id.clone());
    }

    let _ = lf.save(&lock_path);

    // Save to repo history
    let _ = repo_history::upsert_entry(source, repo_url);

    // Invalidate security scan cache for installed/reinstalled skills
    for name in &installed_names {
        security_scan::invalidate_skill_cache(name);
    }

    Ok(installed_names)
}

fn can_replace_existing_skill(
    skill_name: &str,
    repo_url: &str,
    existing_entry: Option<&lockfile::LockEntry>,
) -> bool {
    if local_skill::is_local_skill(skill_name) {
        return false;
    }

    existing_entry
        .map(|entry| same_remote_url(&entry.git_url, repo_url))
        .unwrap_or(false)
}

fn same_remote_url(left: &str, right: &str) -> bool {
    normalize_remote_url(left) == normalize_remote_url(right)
}

fn normalize_remote_url(url: &str) -> String {
    url.trim()
        .trim_end_matches(".git")
        .trim_end_matches('/')
        .to_lowercase()
}

/// Compute the git tree hash for a specific subfolder within a repo.
///
/// Uses `git rev-parse HEAD:<folder_path>` to get the tree hash of a subdirectory.
fn compute_subtree_hash(repo_dir: &Path, folder_path: &str) -> Result<String> {
    let output = command_with_path("git")
        .current_dir(repo_dir)
        .args(["rev-parse", &format!("HEAD:{}", folder_path)])
        .output()
        .context("Failed to execute git rev-parse for subtree")?;

    if !output.status.success() {
        let err = String::from_utf8_lossy(&output.stderr);
        return Err(anyhow!("git rev-parse failed: {}", err.trim()));
    }

    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

/// Public wrapper for `compute_subtree_hash`, used by `commands::update_skill`
/// to recompute sibling lockfile hashes after a shared repo pull.
pub fn compute_subtree_hash_pub(repo_dir: &Path, folder_path: &str) -> Result<String> {
    compute_subtree_hash(repo_dir, folder_path)
}

/// Check if a repo-cached skill has updates available.
///
/// Resolves the symlink to find the source repo, fetches the latest remote
/// state (shallow), and compares local HEAD vs `origin/HEAD` by hash.
/// This works correctly with both shallow and full clones.
///
/// Note: for batch checks, prefer [`prefetch_unique_repos`] +
/// [`check_repo_skill_update_local`] to avoid redundant fetches.
#[allow(dead_code)]
pub fn check_repo_skill_update(skill_path: &Path) -> bool {
    // Resolve the symlink to find the actual repo cache directory
    let real_path = match std::fs::read_link(skill_path) {
        Ok(target) => {
            if target.is_absolute() {
                target
            } else {
                skill_path.parent().unwrap_or(Path::new(".")).join(target)
            }
        }
        Err(_) => return false,
    };

    // Walk up from the skill folder to find the repo root (contains .git)
    let repo_root = find_repo_root(&real_path);
    let repo_root = match repo_root {
        Some(root) => root,
        None => return false,
    };

    // Shallow-friendly update check: fetch --depth 1 then compare hashes
    let _ = command_with_path("git")
        .current_dir(&repo_root)
        .args(["fetch", "--depth", "1", "--quiet"])
        .output();

    let local_head = command_with_path("git")
        .current_dir(&repo_root)
        .args(["rev-parse", "HEAD"])
        .output()
        .ok()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string());

    let remote_head = command_with_path("git")
        .current_dir(&repo_root)
        .args(["rev-parse", "origin/HEAD"])
        .output()
        .ok()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string());

    match (local_head, remote_head) {
        (Some(local), Some(remote)) => !local.is_empty() && !remote.is_empty() && local != remote,
        _ => false,
    }
}

/// Resolve a repo-cached skill path to its repository root.
///
/// Returns `None` if the skill is not a repo-cached symlink or resolution fails.
pub fn resolve_skill_repo_root(skill_path: &Path) -> Option<PathBuf> {
    if !is_repo_cached_skill(skill_path) {
        return None;
    }
    let target = std::fs::read_link(skill_path).ok()?;
    let real_path = if target.is_absolute() {
        target
    } else {
        skill_path.parent().unwrap_or(Path::new(".")).join(target)
    };
    find_repo_root(&real_path)
}

/// Pre-fetch unique repo roots for a batch of skill paths.
///
/// Walks the paths, identifies repo-cached skills, resolves their repo roots,
/// and issues a single `git fetch --depth 1` per unique repository. This
/// avoids redundant fetches when multiple skills share the same repo.
pub fn prefetch_unique_repos(skill_paths: &[PathBuf]) {
    let mut fetched = std::collections::HashSet::new();
    for path in skill_paths {
        if let Some(root) = resolve_skill_repo_root(path) {
            if fetched.insert(root.clone()) {
                let _ = command_with_path("git")
                    .current_dir(&root)
                    .args(["fetch", "--depth", "1", "--quiet"])
                    .output();
            }
        }
    }
}

/// Check if a repo-cached skill has updates **without fetching**.
///
/// Compares local HEAD vs `origin/HEAD`. Call [`prefetch_unique_repos`] first
/// to ensure `origin/HEAD` is up-to-date.
pub fn check_repo_skill_update_local(skill_path: &Path) -> bool {
    let real_path = match std::fs::read_link(skill_path) {
        Ok(target) => {
            if target.is_absolute() {
                target
            } else {
                skill_path.parent().unwrap_or(Path::new(".")).join(target)
            }
        }
        Err(_) => return false,
    };

    let repo_root = match find_repo_root(&real_path) {
        Some(root) => root,
        None => return false,
    };

    let local_head = command_with_path("git")
        .current_dir(&repo_root)
        .args(["rev-parse", "HEAD"])
        .output()
        .ok()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string());

    let remote_head = command_with_path("git")
        .current_dir(&repo_root)
        .args(["rev-parse", "origin/HEAD"])
        .output()
        .ok()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string());

    match (local_head, remote_head) {
        (Some(local), Some(remote)) => !local.is_empty() && !remote.is_empty() && local != remote,
        _ => false,
    }
}

/// Walk up directories to find the git repo root (directory containing .git).
fn find_repo_root(path: &Path) -> Option<PathBuf> {
    let mut current = path.to_path_buf();
    loop {
        if current.join(".git").exists() {
            return Some(current);
        }
        if !current.pop() {
            return None;
        }
    }
}

/// Check whether a skill directory is a symlink into the repo cache.
pub fn is_repo_cached_skill(skill_path: &Path) -> bool {
    if !skill_path.is_symlink() {
        return false;
    }
    let target = match std::fs::read_link(skill_path) {
        Ok(t) => t,
        Err(_) => return false,
    };
    let target_str = target.to_string_lossy();
    target_str.contains(".repos/") || target_str.contains(".repos\\")
}

/// Pull updates for a repo-cached skill.
///
/// Finds the source repo from the symlink, updates local refs from origin,
/// hard-resets to `origin/HEAD`, and returns the new tree hash.
///
/// We intentionally use fetch + hard reset (instead of `git pull`) for both
/// shallow and full clones to guarantee deterministic "exactly at remote HEAD"
/// behavior for cached repos.
pub fn pull_repo_skill_update(skill_path: &Path, folder_path: Option<&str>) -> Result<String> {
    let real_path = std::fs::read_link(skill_path).context("Skill is not a symlink")?;
    let absolute_path = if real_path.is_absolute() {
        real_path
    } else {
        skill_path
            .parent()
            .unwrap_or(Path::new("."))
            .join(real_path)
    };

    let repo_root = find_repo_root(&absolute_path)
        .ok_or_else(|| anyhow!("Cannot find git repo root for symlinked skill"))?;

    let fetch_args: Vec<&str> = if is_shallow_repo(&repo_root) {
        vec!["fetch", "--depth", "1", "--quiet"]
    } else {
        vec!["fetch", "--quiet"]
    };

    let output = command_with_path("git")
        .current_dir(&repo_root)
        .args(fetch_args)
        .output()
        .context("Failed to execute git fetch for repo-cached update")?;

    if !output.status.success() {
        let err = String::from_utf8_lossy(&output.stderr);
        return Err(anyhow!("git fetch failed: {}", err.trim()));
    }

    let output = command_with_path("git")
        .current_dir(&repo_root)
        .args(["reset", "--hard", "origin/HEAD"])
        .output()
        .context("Failed to execute git reset for repo-cached update")?;

    if !output.status.success() {
        let err = String::from_utf8_lossy(&output.stderr);
        return Err(anyhow!("git reset failed: {}", err.trim()));
    }

    // Re-apply sparse checkout if enabled (handles newly added skills)
    if is_sparse_checkout(&repo_root) {
        if let Ok(dirs) = discover_skill_dirs_from_tree(&repo_root) {
            let dir_refs: Vec<&str> = dirs.iter().map(|s| s.as_str()).collect();
            let _ = git_ops::apply_sparse_checkout(&repo_root, &dir_refs);
        }
    }

    // Compute new tree hash
    match folder_path {
        Some(fp) if !fp.is_empty() => compute_subtree_hash(&repo_root, fp),
        _ => git_ops::compute_tree_hash(&repo_root),
    }
}

/// Check whether a git repo is a shallow clone.
fn is_shallow_repo(repo_dir: &Path) -> bool {
    repo_dir.join(".git/shallow").exists()
}

// ── Top-level Scan Command ──────────────────────────────────────────

/// Full scan flow: normalize URL → clone/fetch → scan → save history → return results.
pub fn scan_repo(input: &str) -> Result<ScanResult> {
    let (repo_url, source) = normalize_repo_url(input)?;

    let repo_dir = clone_or_fetch_repo(&repo_url, &source)?;

    let skills = scan_skills_in_repo(&repo_dir);

    // Save to history
    let _ = repo_history::upsert_entry(&source, &repo_url);

    Ok(ScanResult {
        source,
        source_url: repo_url,
        skills,
    })
}

// ── Cache Management ────────────────────────────────────────────────

/// Information about the repo cache directory.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RepoCacheInfo {
    /// Total size of all cached repos in bytes
    pub total_bytes: u64,
    /// Number of cached repos
    pub repo_count: usize,
    /// Number of repos that are unused (no installed skill symlinks point to them)
    pub unused_count: usize,
    /// Bytes used by unused repos
    pub unused_bytes: u64,
}

/// Collect information about the repo cache (`.repos/` directory).
pub fn get_cache_info() -> RepoCacheInfo {
    let cache_dir = get_repos_cache_dir();
    if !cache_dir.exists() {
        return RepoCacheInfo {
            total_bytes: 0,
            repo_count: 0,
            unused_count: 0,
            unused_bytes: 0,
        };
    }

    let hub_skills_dir = sync::get_hub_skills_dir();
    let referenced = collect_referenced_cache_dirs(&hub_skills_dir, &cache_dir);

    let mut total_bytes: u64 = 0;
    let mut repo_count: usize = 0;
    let mut unused_count: usize = 0;
    let mut unused_bytes: u64 = 0;

    if let Ok(entries) = std::fs::read_dir(&cache_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }
            let size = dir_size(&path);
            total_bytes += size;
            repo_count += 1;

            if !referenced.contains(&path) {
                unused_count += 1;
                unused_bytes += size;
            }
        }
    }

    RepoCacheInfo {
        total_bytes,
        repo_count,
        unused_count,
        unused_bytes,
    }
}

/// Remove cached repos that are NOT referenced by any installed skill symlink.
///
/// Returns the number of repos removed.
pub fn clean_unused_cache() -> Result<usize> {
    let cache_dir = get_repos_cache_dir();
    if !cache_dir.exists() {
        return Ok(0);
    }

    let hub_skills_dir = sync::get_hub_skills_dir();
    let referenced = collect_referenced_cache_dirs(&hub_skills_dir, &cache_dir);

    let mut removed: usize = 0;

    if let Ok(entries) = std::fs::read_dir(&cache_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }
            if !referenced.contains(&path) {
                if std::fs::remove_dir_all(&path).is_ok() {
                    removed += 1;
                }
            }
        }
    }

    Ok(removed)
}

/// Collect canonical cache-dir paths that are referenced by installed skills.
///
/// Walks `~/.agents/skills/`, resolves any symlinks, walks up to the repo root
/// (dir containing `.git`), and checks if that root lives under `cache_dir`.
fn collect_referenced_cache_dirs(
    hub_skills_dir: &Path,
    cache_dir: &Path,
) -> std::collections::HashSet<PathBuf> {
    let mut referenced = std::collections::HashSet::new();

    let entries = match std::fs::read_dir(hub_skills_dir) {
        Ok(e) => e,
        Err(_) => return referenced,
    };

    for entry in entries.flatten() {
        let skill_path = entry.path();
        if !skill_path.is_symlink() {
            continue;
        }

        // Resolve symlink target
        let target = match std::fs::read_link(&skill_path) {
            Ok(t) => {
                if t.is_absolute() {
                    t
                } else {
                    skill_path.parent().unwrap_or(Path::new(".")).join(t)
                }
            }
            Err(_) => continue,
        };

        // Walk up to find repo root
        if let Some(repo_root) = find_repo_root(&target) {
            // Only track if the repo root is directly inside our cache_dir
            if let Some(parent) = repo_root.parent() {
                if parent == cache_dir {
                    referenced.insert(repo_root);
                }
            }
        }
    }

    referenced
}

/// Recursively compute the total size of a directory in bytes.
///
/// Uses an explicit stack to avoid deep recursion on large repos.
fn dir_size(path: &Path) -> u64 {
    let mut total: u64 = 0;
    let mut stack = vec![path.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let entries = match std::fs::read_dir(&dir) {
            Ok(e) => e,
            Err(_) => continue,
        };
        for entry in entries.flatten() {
            let entry_path = entry.path();
            if entry_path.is_dir() {
                stack.push(entry_path);
            } else if let Ok(meta) = entry_path.metadata() {
                total += meta.len();
            }
        }
    }
    total
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::{local_skill, lockfile, sync};
    use anyhow::Result;
    use std::ffi::OsStr;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn env_lock() -> &'static std::sync::Mutex<()> {
        crate::core::test_env_lock()
    }

    fn set_env<K: AsRef<OsStr>, V: AsRef<OsStr>>(key: K, value: V) {
        unsafe { std::env::set_var(key, value) }
    }

    fn remove_env<K: AsRef<OsStr>>(key: K) {
        unsafe { std::env::remove_var(key) }
    }

    #[test]
    fn normalize_owner_repo() {
        let (url, source) = normalize_repo_url("vercel-labs/skills").unwrap();
        assert_eq!(url, "https://github.com/vercel-labs/skills.git");
        assert_eq!(source, "vercel-labs/skills");
    }

    #[test]
    fn normalize_full_url() {
        let (url, source) = normalize_repo_url("https://github.com/vercel-labs/skills").unwrap();
        assert_eq!(url, "https://github.com/vercel-labs/skills.git");
        assert_eq!(source, "vercel-labs/skills");
    }

    #[test]
    fn normalize_full_url_with_git_suffix() {
        let (url, source) =
            normalize_repo_url("https://github.com/vercel-labs/skills.git").unwrap();
        assert_eq!(url, "https://github.com/vercel-labs/skills.git");
        assert_eq!(source, "vercel-labs/skills");
    }

    #[test]
    fn normalize_empty_input_fails() {
        assert!(normalize_repo_url("").is_err());
        assert!(normalize_repo_url("  ").is_err());
    }

    #[test]
    fn normalize_invalid_format_fails() {
        assert!(normalize_repo_url("just-a-name").is_err());
        assert!(normalize_repo_url("a/b/c").is_err());
    }

    #[test]
    fn cache_dir_name_conversion() {
        assert_eq!(cache_dir_name("vercel-labs/skills"), "vercel-labs--skills");
        assert_eq!(cache_dir_name("anthropics/courses"), "anthropics--courses");
    }

    #[test]
    fn compact_parents_groups_siblings() {
        let dirs = vec![
            "source/skills/adapt".to_string(),
            "source/skills/animate".to_string(),
            "source/skills/bolder".to_string(),
        ];
        let compacted = compact_to_common_parents(&dirs);
        assert_eq!(compacted, vec!["source/skills"]);
    }

    #[test]
    fn compact_parents_preserves_singles() {
        let dirs = vec!["custom/my-skill".to_string()];
        let compacted = compact_to_common_parents(&dirs);
        assert_eq!(compacted, vec!["custom/my-skill"]);
    }

    #[test]
    fn compact_parents_mixed() {
        let dirs = vec![
            "custom/lone-skill".to_string(),
            "source/skills/adapt".to_string(),
            "source/skills/animate".to_string(),
        ];
        let compacted = compact_to_common_parents(&dirs);
        assert_eq!(compacted, vec!["custom/lone-skill", "source/skills"]);
    }

    #[test]
    fn compact_parents_empty() {
        let dirs: Vec<String> = Vec::new();
        let compacted = compact_to_common_parents(&dirs);
        assert!(compacted.is_empty());
    }

    #[test]
    fn install_from_repo_does_not_replace_local_skill() -> Result<()> {
        let _guard = env_lock()
            .lock()
            .expect("environment lock should not be poisoned");

        let stamp = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
        let temp_root =
            std::env::temp_dir().join(format!("skillstar-repo-scanner-local-{}", stamp));
        let previous_home = std::env::var_os("HOME");
        set_env("HOME", temp_root.join("home"));
        #[cfg(windows)]
        let previous_userprofile = std::env::var_os("USERPROFILE");
        #[cfg(windows)]
        set_env("USERPROFILE", temp_root.join("home"));

        let result = (|| -> Result<()> {
            let _ = local_skill::create("demo-skill", Some("# local"))?;

            let cache_dir = super::get_repos_cache_dir().join(super::cache_dir_name("owner/repo"));
            let source_dir = cache_dir.join("skills/demo-skill");
            std::fs::create_dir_all(&source_dir)?;
            std::fs::write(source_dir.join("SKILL.md"), "# remote")?;

            let installed = install_from_repo(
                "owner/repo",
                "https://github.com/owner/repo.git",
                &[SkillInstallTarget {
                    id: "demo-skill".to_string(),
                    folder_path: "skills/demo-skill".to_string(),
                }],
            )?;

            assert!(
                installed.is_empty(),
                "expected repo install to skip replacing a local skill"
            );
            assert!(
                local_skill::is_local_skill("demo-skill"),
                "expected local skill link to remain intact"
            );

            Ok(())
        })();

        match previous_home {
            Some(value) => set_env("HOME", value),
            None => remove_env("HOME"),
        }
        #[cfg(windows)]
        match previous_userprofile {
            Some(value) => set_env("USERPROFILE", value),
            None => remove_env("USERPROFILE"),
        }
        let _ = std::fs::remove_dir_all(&temp_root);

        result
    }

    #[test]
    fn install_from_repo_does_not_replace_different_remote() -> Result<()> {
        let _guard = env_lock()
            .lock()
            .expect("environment lock should not be poisoned");

        let stamp = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
        let temp_root =
            std::env::temp_dir().join(format!("skillstar-repo-scanner-remote-{}", stamp));
        let previous_home = std::env::var_os("HOME");
        set_env("HOME", temp_root.join("home"));
        #[cfg(windows)]
        let previous_userprofile = std::env::var_os("USERPROFILE");
        #[cfg(windows)]
        set_env("USERPROFILE", temp_root.join("home"));

        let result = (|| -> Result<()> {
            let hub_path = sync::get_hub_skills_dir().join("demo-skill");
            std::fs::create_dir_all(&hub_path)?;
            std::fs::write(hub_path.join("SKILL.md"), "# existing")?;

            let lock_path = lockfile::lockfile_path();
            let mut lf = lockfile::Lockfile::load(&lock_path).unwrap_or_default();
            lf.upsert(lockfile::LockEntry {
                name: "demo-skill".to_string(),
                git_url: "https://github.com/existing/repo.git".to_string(),
                tree_hash: "existing-hash".to_string(),
                installed_at: chrono::Utc::now().to_rfc3339(),
                source_folder: None,
            });
            lf.save(&lock_path)?;

            let cache_dir = super::get_repos_cache_dir().join(super::cache_dir_name("owner/repo"));
            let source_dir = cache_dir.join("skills/demo-skill");
            std::fs::create_dir_all(&source_dir)?;
            std::fs::write(source_dir.join("SKILL.md"), "# incoming")?;

            let installed = install_from_repo(
                "owner/repo",
                "https://github.com/owner/repo.git",
                &[SkillInstallTarget {
                    id: "demo-skill".to_string(),
                    folder_path: "skills/demo-skill".to_string(),
                }],
            )?;

            assert!(
                installed.is_empty(),
                "expected repo install to skip replacing a skill from another remote"
            );
            assert!(
                hub_path.is_dir() && !hub_path.is_symlink(),
                "expected existing hub skill directory to remain untouched"
            );
            let updated_lock = lockfile::Lockfile::load(&lock_path)?;
            let entry = updated_lock
                .skills
                .into_iter()
                .find(|entry| entry.name == "demo-skill")
                .expect("expected original lockfile entry to remain");
            assert_eq!(entry.git_url, "https://github.com/existing/repo.git");

            Ok(())
        })();

        match previous_home {
            Some(value) => set_env("HOME", value),
            None => remove_env("HOME"),
        }
        #[cfg(windows)]
        match previous_userprofile {
            Some(value) => set_env("USERPROFILE", value),
            None => remove_env("USERPROFILE"),
        }
        let _ = std::fs::remove_dir_all(&temp_root);

        result
    }
}
