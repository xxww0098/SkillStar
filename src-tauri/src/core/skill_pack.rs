use anyhow::{Context, Result, anyhow, bail};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

// ── Types ───────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillPackManifest {
    pub name: String,
    pub version: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub author: Option<String>,
    #[serde(default)]
    pub homepage: Option<String>,
    #[serde(default)]
    pub license: Option<String>,
    pub skills: Vec<PackSkill>,
    #[serde(default)]
    pub dependencies: Vec<PackDependency>,
    #[serde(default)]
    pub conflicts: Vec<PackConflict>,
    #[serde(default)]
    pub post_install: Option<PostInstall>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PackSkill {
    pub name: String,
    pub path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PackDependency {
    pub name: String,
    #[serde(default)]
    pub pack: Option<String>,
    #[serde(default)]
    pub version: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PackConflict {
    pub name: String,
    #[serde(default)]
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PostInstall {
    pub script: String,
    #[serde(default = "default_timeout")]
    pub timeout_secs: u64,
}

fn default_timeout() -> u64 {
    300
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PackEntry {
    pub name: String,
    pub version: String,
    pub description: String,
    pub git_url: String,
    pub tree_hash: String,
    pub installed_at: String,
    pub updated_at: String,
    pub skills: Vec<PackSkillEntry>,
    #[serde(default)]
    pub post_install: Option<PostInstallResult>,
    #[serde(default)]
    pub repo_cache_path: String,
    #[serde(default)]
    pub status: PackStatus,
    #[serde(default)]
    pub last_error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum PackStatus {
    #[default]
    Installed,
    InstallFailed,
    PartiallyInstalled,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PackSkillEntry {
    pub name: String,
    pub path: String,
    #[serde(default)]
    pub symlink_valid: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PostInstallResult {
    pub last_exit_code: i32,
    pub last_run_at: String,
}

#[derive(Debug, Serialize, Deserialize, Default)]
struct PackStore {
    version: u32,
    packs: Vec<PackEntry>,
}

// ── Mutex ────────────────────────────────────────────────────────────

static PACK_MUTEX: OnceLock<Mutex<()>> = OnceLock::new();

fn get_mutex() -> &'static Mutex<()> {
    PACK_MUTEX.get_or_init(|| Mutex::new(()))
}

// ── Paths ────────────────────────────────────────────────────────────

fn store_path() -> PathBuf {
    super::paths::packs_path()
}

// ── Store I/O ────────────────────────────────────────────────────────

fn load_store() -> PackStore {
    let path = store_path();
    if !path.exists() {
        return PackStore::default();
    }
    let content = std::fs::read_to_string(&path).unwrap_or_default();
    serde_json::from_str(&content).unwrap_or_default()
}

fn save_store(store: &PackStore) -> Result<()> {
    let path = store_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let content = serde_json::to_string_pretty(store)?;
    std::fs::write(&path, content).context("Failed to write packs.json")?;
    Ok(())
}

// ── Manifest Parsing ─────────────────────────────────────────────────

/// Detect and parse a skillpack.toml in the given directory.
/// Returns None if no skillpack.toml exists.
pub fn detect_pack(repo_dir: &Path) -> Result<Option<SkillPackManifest>> {
    let toml_path = repo_dir.join("skillpack.toml");
    if !toml_path.exists() {
        return Ok(None);
    }

    let content = std::fs::read_to_string(&toml_path)
        .with_context(|| format!("Failed to read skillpack.toml: {:?}", toml_path))?;

    let manifest: SkillPackManifest = toml::from_str(&content)
        .with_context(|| format!("Failed to parse skillpack.toml: {:?}", toml_path))?;

    // Validate required fields
    if manifest.name.is_empty() {
        bail!("skillpack.toml: 'name' is required");
    }
    if manifest.version.is_empty() {
        bail!("skillpack.toml: 'version' is required");
    }
    if manifest.skills.is_empty() {
        bail!("skillpack.toml: at least one [[skills]] entry is required");
    }

    // Validate skill paths exist
    for skill in &manifest.skills {
        if skill.name.is_empty() {
            bail!("skillpack.toml: each [[skills]] entry must have a 'name'");
        }
        if skill.path.is_empty() {
            bail!("skillpack.toml: each [[skills]] entry must have a 'path'");
        }
        let skill_dir = repo_dir.join(&skill.path);
        if !skill_dir.exists() {
            bail!(
                "skillpack.toml: skill '{}' path '{}' does not exist in repo",
                skill.name,
                skill.path
            );
        }
        let skill_md = skill_dir.join("SKILL.md");
        if !skill_md.exists() {
            bail!(
                "skillpack.toml: skill '{}' at '{}' is missing SKILL.md",
                skill.name,
                skill.path
            );
        }
    }

    Ok(Some(manifest))
}

// ── Install ──────────────────────────────────────────────────────────

/// Install a skill pack from a cloned repo directory.
/// Follows atomic staging pattern from skill_bundle.rs.
pub fn install_pack(repo_dir: &Path, source: &str, repo_url: &str) -> Result<Vec<String>> {
    let manifest =
        detect_pack(repo_dir)?.ok_or_else(|| anyhow::anyhow!("No skillpack.toml found in repo"))?;

    let _lock = get_mutex()
        .lock()
        .map_err(|_| anyhow!("Pack store mutex poisoned"))?;

    let hub = super::paths::hub_skills_dir();
    std::fs::create_dir_all(&hub).context("Failed to create hub skills directory")?;

    // Check for existing pack with same name
    let mut store = load_store();
    if store.packs.iter().any(|p| p.name == manifest.name) {
        bail!(
            "Pack '{}' is already installed. Remove it first with 'skillstar pack remove {}'",
            manifest.name,
            manifest.name
        );
    }

    // Stage all skills to temp dirs
    let mut staged: Vec<(String, PathBuf, PathBuf)> = Vec::new(); // (name, temp_dir, target_dir)
    let mut installed_names = Vec::new();

    for skill in &manifest.skills {
        let src_path = repo_dir.join(&skill.path);
        let target_dir = hub.join(&skill.name);
        let temp_dir = hub.join(format!(".importing-pack-{}-{}", manifest.name, skill.name));

        // Clean stale temp dir if exists
        if temp_dir.exists() {
            let _ = std::fs::remove_dir_all(&temp_dir);
        }

        // Stage: symlink temp_dir → src_path (same pattern as repo_scanner.rs:710)
        super::paths::create_symlink(&src_path, &temp_dir)
            .with_context(|| format!("Failed to stage skill '{}' for pack", skill.name))?;

        staged.push((skill.name.clone(), temp_dir, target_dir));
    }

    // Validate: check for conflicts with existing skills
    for (name, temp_dir, target_dir) in &staged {
        if target_dir.exists() && !super::paths::is_link(target_dir) {
            // Existing non-symlink skill conflicts — abort
            // Clean up all staged temp dirs
            for (_, td, _) in &staged {
                let _ = super::paths::remove_symlink(td);
            }
            bail!(
                "Skill '{}' already exists in hub (not a symlink). Remove it before installing pack '{}'.",
                name,
                manifest.name
            );
        }
    }

    // Rename all temp dirs to target dirs (atomic-ish, same as skill_bundle.rs)
    let mut renamed = Vec::new();
    for (name, temp_dir, target_dir) in &staged {
        // Remove existing symlink if present
        if super::paths::is_link(target_dir) || target_dir.exists() {
            if super::paths::is_link(target_dir) {
                let _ = super::paths::remove_symlink(target_dir);
            } else {
                let _ = std::fs::remove_dir_all(target_dir);
            }
        }

        match std::fs::rename(temp_dir, target_dir) {
            Ok(()) => {
                renamed.push(target_dir.clone());
                installed_names.push(name.clone());
            }
            Err(e) => {
                // Best-effort cleanup of already-renamed dirs
                for dir in &renamed {
                    let _ = std::fs::remove_dir(dir);
                }
                // Clean remaining temp dirs
                for (_, td, _) in &staged {
                    let _ = std::fs::remove_dir(td);
                }
                bail!(
                    "Failed to install skill '{}' for pack '{}': {}. Rolled back {} skills.",
                    name,
                    manifest.name,
                    e,
                    renamed.len()
                );
            }
        }
    }

    // Update lockfile for each installed skill
    let _lock2 = super::lockfile::get_mutex()
        .lock()
        .map_err(|_| anyhow!("Lockfile mutex poisoned"))?;
    let lf_path = super::lockfile::lockfile_path();
    let mut lf = super::lockfile::Lockfile::load(&lf_path)?;

    let cache_dir_name = super::repo_scanner::cache_dir_name(source);
    let tree_hash = super::git_ops::compute_tree_hash(repo_dir).unwrap_or_default();

    for skill in &manifest.skills {
        let source_folder = if skill.path == "." || skill.path.is_empty() {
            None
        } else {
            Some(skill.path.clone())
        };
        lf.upsert(super::lockfile::LockEntry {
            name: skill.name.clone(),
            git_url: repo_url.to_string(),
            tree_hash: tree_hash.clone(),
            installed_at: chrono::Utc::now().to_rfc3339(),
            source_folder,
        });
    }
    lf.save(&lf_path)?;

    // Write pack entry to packs.json
    let now = chrono::Utc::now().to_rfc3339();
    let pack_entry = PackEntry {
        name: manifest.name.clone(),
        version: manifest.version.clone(),
        description: manifest.description.clone(),
        git_url: repo_url.to_string(),
        tree_hash: tree_hash.clone(),
        installed_at: now.clone(),
        updated_at: now,
        skills: manifest
            .skills
            .iter()
            .map(|s| PackSkillEntry {
                name: s.name.clone(),
                path: s.path.clone(),
                symlink_valid: true,
            })
            .collect(),
        post_install: None,
        repo_cache_path: format!("repos/{}", cache_dir_name),
        status: PackStatus::Installed,
        last_error: None,
    };
    store.packs.push(pack_entry);
    save_store(&store)?;

    // Execute post_install if present
    if let Some(ref post_install) = manifest.post_install {
        let script_path = repo_dir.join(&post_install.script);
        if script_path.exists() {
            let result = execute_post_install(&script_path, repo_dir, post_install.timeout_secs);
            let mut store = load_store();
            if let Some(entry) = store.packs.iter_mut().find(|p| p.name == manifest.name) {
                entry.post_install = Some(PostInstallResult {
                    last_exit_code: result,
                    last_run_at: chrono::Utc::now().to_rfc3339(),
                });
                if result != 0 {
                    entry.status = PackStatus::PartiallyInstalled;
                    entry.last_error =
                        Some(format!("post_install script exited with code {}", result));
                }
                save_store(&store)?;
            }
        }
    }

    // Invalidate installed skill cache
    super::installed_skill::invalidate_cache();

    Ok(installed_names)
}

/// Execute a post-install script with timeout.
/// Returns the exit code (0 = success).
fn execute_post_install(script: &Path, working_dir: &Path, _timeout_secs: u64) -> i32 {
    use crate::core::path_env::command_with_path;

    let output = command_with_path("sh")
        .arg("-c")
        .arg(script.to_string_lossy().as_ref())
        .current_dir(working_dir)
        .output();

    match output {
        Ok(out) => out.status.code().unwrap_or(-1),
        Err(_) => -1,
    }
}

// ── Remove ───────────────────────────────────────────────────────────

/// Remove an installed pack by name.
/// Cleans symlinks from hub, removes lockfile entries, updates packs.json.
/// Does NOT delete the repo cache (allows reinstall without re-clone).
pub fn remove_pack(name: &str) -> Result<Vec<String>> {
    let _lock = get_mutex()
        .lock()
        .map_err(|_| anyhow!("Pack store mutex poisoned"))?;

    let mut store = load_store();
    let pack_idx = store
        .packs
        .iter()
        .position(|p| p.name == name)
        .ok_or_else(|| anyhow!("Pack '{}' not found", name))?;

    let pack = &store.packs[pack_idx];
    let hub = super::paths::hub_skills_dir();
    let mut removed = Vec::new();

    for skill in &pack.skills {
        let skill_path = hub.join(&skill.name);
        if super::paths::is_link(&skill_path) || skill_path.exists() {
            if super::paths::is_link(&skill_path) {
                match super::paths::remove_symlink(&skill_path) {
                    Ok(()) => removed.push(skill.name.clone()),
                    Err(e) => {
                        tracing::warn!("Failed to remove skill symlink '{}': {}", skill.name, e);
                    }
                }
            } else {
                match std::fs::remove_dir_all(&skill_path) {
                    Ok(()) => removed.push(skill.name.clone()),
                    Err(e) => {
                        tracing::warn!("Failed to remove skill dir '{}': {}", skill.name, e);
                    }
                }
            }
        }
    }

    // Remove from lockfile
    let _lock2 = super::lockfile::get_mutex()
        .lock()
        .map_err(|_| anyhow!("Lockfile mutex poisoned"))?;
    let lf_path = super::lockfile::lockfile_path();
    let mut lf = super::lockfile::Lockfile::load(&lf_path)?;
    for skill in &pack.skills {
        lf.remove(&skill.name);
    }
    lf.save(&lf_path)?;

    // Remove from packs.json
    store.packs.remove(pack_idx);
    save_store(&store)?;

    // Invalidate cache
    super::installed_skill::invalidate_cache();

    Ok(removed)
}

// ── List ─────────────────────────────────────────────────────────────

/// List all installed packs.
pub fn list_packs() -> Vec<PackEntry> {
    load_store().packs
}

// ── Doctor ───────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DoctorReport {
    pub pack_name: String,
    pub version: String,
    pub overall_healthy: bool,
    pub checks: Vec<DoctorCheck>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DoctorCheck {
    pub name: String,
    pub passed: bool,
    pub message: Option<String>,
}

fn resolve_repo_cache_path(pack: &PackEntry) -> PathBuf {
    let configured = pack.repo_cache_path.trim();
    let hub_root = super::paths::hub_root();
    let direct = hub_root.join(configured);
    if direct.exists() {
        return direct;
    }

    if let Some(suffix) = configured.strip_prefix(".repos/") {
        let migrated = super::paths::repos_cache_dir().join(suffix);
        if migrated.exists() {
            return migrated;
        }
    }

    if let Some(suffix) = configured.strip_prefix("repos/") {
        let current = super::paths::repos_cache_dir().join(suffix);
        if current.exists() {
            return current;
        }
    }

    direct
}

/// Run health checks on an installed pack.
pub fn doctor_pack(name: &str) -> Result<DoctorReport> {
    let store = load_store();
    let pack = store
        .packs
        .iter()
        .find(|p| p.name == name)
        .ok_or_else(|| anyhow!("Pack '{}' not found", name))?;

    let hub = super::paths::hub_skills_dir();
    let mut checks = Vec::new();
    let mut all_healthy = true;

    // Check 1: Every skill has a valid symlink in hub
    let mut missing_skills = Vec::new();
    let mut broken_links = Vec::new();
    for skill in &pack.skills {
        let skill_path = hub.join(&skill.name);
        if !skill_path.exists() && !super::paths::is_link(&skill_path) {
            missing_skills.push(skill.name.clone());
        } else if super::paths::is_link(&skill_path) {
            // Check if symlink target resolves
            if std::fs::read_link(&skill_path)
                .map(|t| !t.exists())
                .unwrap_or(true)
            {
                broken_links.push(skill.name.clone());
            }
        }
    }
    checks.push(DoctorCheck {
        name: "skill_symlinks".into(),
        passed: missing_skills.is_empty() && broken_links.is_empty(),
        message: if !missing_skills.is_empty() {
            all_healthy = false;
            Some(format!("Missing skills: {}", missing_skills.join(", ")))
        } else if !broken_links.is_empty() {
            all_healthy = false;
            Some(format!("Broken symlinks: {}", broken_links.join(", ")))
        } else {
            Some(format!("All {} skill symlinks valid", pack.skills.len()))
        },
    });

    // Check 2: Repo cache exists and skillpack.toml readable
    let repo_path = resolve_repo_cache_path(pack);
    let toml_path = repo_path.join("skillpack.toml");
    let repo_ok = repo_path.exists() && toml_path.exists();
    checks.push(DoctorCheck {
        name: "repo_cache".into(),
        passed: repo_ok,
        message: if repo_ok {
            Some("Repo cache and skillpack.toml accessible".into())
        } else {
            all_healthy = false;
            Some("Repo cache or skillpack.toml missing".into())
        },
    });

    // Check 3: post_install last exit code
    if let Some(ref pi) = pack.post_install {
        checks.push(DoctorCheck {
            name: "post_install".into(),
            passed: pi.last_exit_code == 0,
            message: Some(format!(
                "Last exit code: {} (run at {})",
                pi.last_exit_code, pi.last_run_at
            )),
        });
        if pi.last_exit_code != 0 {
            all_healthy = false;
        }
    }

    // Check 4: Pack status
    checks.push(DoctorCheck {
        name: "pack_status".into(),
        passed: pack.status == PackStatus::Installed,
        message: Some(format!("Status: {:?}", pack.status)),
    });
    if pack.status != PackStatus::Installed {
        all_healthy = false;
    }

    // Check 5: Error message if any
    if let Some(ref err) = pack.last_error {
        all_healthy = false;
        checks.push(DoctorCheck {
            name: "last_error".into(),
            passed: false,
            message: Some(err.clone()),
        });
    }

    Ok(DoctorReport {
        pack_name: pack.name.clone(),
        version: pack.version.clone(),
        overall_healthy: all_healthy,
        checks,
    })
}

/// Run doctor on all installed packs.
pub fn doctor_all() -> Vec<DoctorReport> {
    let store = load_store();
    store
        .packs
        .iter()
        .filter_map(|p| doctor_pack(&p.name).ok())
        .collect()
}
