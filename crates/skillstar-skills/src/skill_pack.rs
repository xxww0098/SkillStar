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

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
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
    skillstar_core::infra::paths::packs_path()
}

// ── Store I/O ────────────────────────────────────────────────────────

fn load_store() -> PackStore {
    load_store_strict().unwrap_or_default()
}

fn load_store_strict() -> Result<PackStore> {
    let path = store_path();
    if !path.exists() {
        return Ok(PackStore::default());
    }
    let content = std::fs::read_to_string(&path)
        .with_context(|| format!("Failed to read pack registry '{}'", path.display()))?;
    let store: PackStore = serde_json::from_str(&content)
        .with_context(|| format!("Failed to parse pack registry '{}'", path.display()))?;
    for pack in &store.packs {
        let mut names = std::collections::BTreeSet::new();
        for skill in &pack.skills {
            crate::content::validate_skill_name(&skill.name).with_context(|| {
                format!(
                    "Pack registry contains an unsafe Skill name in '{}': {:?}",
                    pack.name, skill.name
                )
            })?;
            let key = if cfg!(windows) {
                skill.name.to_ascii_lowercase()
            } else {
                skill.name.clone()
            };
            if !names.insert(key) {
                bail!(
                    "Pack registry contains duplicate Skill name in '{}': {:?}",
                    pack.name,
                    skill.name
                );
            }
            validate_pack_relative_path(&skill.path).with_context(|| {
                format!(
                    "Pack registry contains an unsafe Skill path in '{}': {:?}",
                    pack.name, skill.path
                )
            })?;
        }
    }
    Ok(store)
}

fn validate_pack_relative_path(value: &str) -> Result<()> {
    let path = Path::new(value);
    if value.is_empty()
        || (value != "."
            && (path.is_absolute()
                || value.contains(['\\', '\0'])
                || !path
                    .components()
                    .all(|component| matches!(component, std::path::Component::Normal(_)))))
    {
        bail!("unsafe pack-relative path: {value:?}");
    }
    Ok(())
}

fn save_store(store: &PackStore) -> Result<()> {
    let path = store_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let content = serde_json::to_string_pretty(store)?;
    skillstar_core::infra::fs_ops::atomic_write(&path, content.as_bytes())
        .context("Failed to write packs.json atomically")?;
    Ok(())
}

fn restore_persisted_file(path: &Path, previous: Option<&[u8]>) {
    match previous {
        Some(content) => {
            let _ = skillstar_core::infra::fs_ops::atomic_write(path, content);
        }
        None if path.symlink_metadata().is_ok() => {
            let _ = skillstar_core::infra::fs_ops::remove_link_or_copy(path);
        }
        None => {}
    }
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
    let mut skill_names = std::collections::BTreeSet::new();
    for skill in &manifest.skills {
        if skill.name.is_empty() {
            bail!("skillpack.toml: each [[skills]] entry must have a 'name'");
        }
        crate::content::validate_skill_name(&skill.name)
            .with_context(|| format!("skillpack.toml: invalid Skill name '{}'", skill.name))?;
        let uniqueness_key = if cfg!(windows) {
            skill.name.to_ascii_lowercase()
        } else {
            skill.name.clone()
        };
        if !skill_names.insert(uniqueness_key) {
            bail!(
                "skillpack.toml: duplicate Skill name '{}' is not allowed",
                skill.name
            );
        }
        if skill.path.is_empty() {
            bail!("skillpack.toml: each [[skills]] entry must have a 'path'");
        }
        let skill_dir = resolve_pack_skill_source(repo_dir, &skill.path).with_context(|| {
            format!(
                "skillpack.toml: invalid path '{}' for Skill '{}'",
                skill.path, skill.name
            )
        })?;
        let skill_md = skill_dir.join("SKILL.md");
        if !skill_md.exists() {
            bail!(
                "skillpack.toml: skill '{}' at '{}' is missing SKILL.md",
                skill.name,
                skill.path
            );
        }
    }
    if let Some(post_install) = &manifest.post_install {
        validate_pack_relative_path(&post_install.script).with_context(|| {
            format!(
                "skillpack.toml: unsafe post_install script path {:?}",
                post_install.script
            )
        })?;
        let repo_root = std::fs::canonicalize(repo_dir)
            .with_context(|| format!("Failed to resolve pack repo '{}'", repo_dir.display()))?;
        let script = repo_root.join(&post_install.script);
        if script.exists() {
            let script = std::fs::canonicalize(&script).with_context(|| {
                format!(
                    "Failed to resolve post_install script '{}'",
                    script.display()
                )
            })?;
            if !script.starts_with(&repo_root) {
                bail!("skillpack.toml: post_install script escapes the repository");
            }
        }
    }

    Ok(Some(manifest))
}

fn resolve_pack_skill_source(repo_dir: &Path, relative: &str) -> Result<PathBuf> {
    validate_pack_relative_path(relative)
        .with_context(|| format!("unsafe pack Skill path: {relative:?}"))?;
    let repo_root = std::fs::canonicalize(repo_dir)
        .with_context(|| format!("Failed to resolve pack repo '{}'", repo_dir.display()))?;
    let source = std::fs::canonicalize(repo_root.join(relative))
        .with_context(|| format!("Pack Skill path does not exist: {relative:?}"))?;
    if !source.starts_with(&repo_root) {
        bail!("pack Skill path escapes its repository: {relative:?}");
    }
    Ok(source)
}

// ── Install ──────────────────────────────────────────────────────────

/// Install a skill pack from a cloned repo directory.
/// Follows atomic staging pattern from skill_bundle.rs.
pub fn install_pack(repo_dir: &Path, source: &str, repo_url: &str) -> Result<Vec<String>> {
    let _transaction_guard = crate::skill_update::acquire_update_transaction_lock()?;
    install_pack_locked(repo_dir, source, repo_url)
}

pub(crate) fn install_pack_locked(
    repo_dir: &Path,
    source: &str,
    repo_url: &str,
) -> Result<Vec<String>> {
    crate::skill_mutation::policy().ensure_repository_mutation_allowed(repo_url)?;
    let manifest =
        detect_pack(repo_dir)?.ok_or_else(|| anyhow::anyhow!("No skillpack.toml found in repo"))?;

    let _lock = get_mutex()
        .lock()
        .map_err(|_| anyhow!("Pack store mutex poisoned"))?;

    let hub = skillstar_core::infra::paths::hub_skills_dir();
    std::fs::create_dir_all(&hub).context("Failed to create hub skills directory")?;

    // Check for existing pack with same name
    let mut store = load_store_strict()?;
    if store.packs.iter().any(|p| p.name == manifest.name) {
        bail!(
            "Pack '{}' is already installed. Remove it first with 'skillstar pack remove {}'",
            manifest.name,
            manifest.name
        );
    }

    // A generic pack install must never replace content owned by a shared
    // channel, regardless of that subscription's current remote state.
    for skill in &manifest.skills {
        crate::skill_mutation::policy().ensure_skill_mutation_allowed(&skill.name)?;
    }

    // Validate complete baselines before any hub entry is replaced. A snapshot
    // failure must leave every previously usable Skill untouched.
    let baselines: Vec<(String, String)> = manifest
        .skills
        .iter()
        .map(|skill| {
            let source = resolve_pack_skill_source(repo_dir, &skill.path)?;
            // Frontmatter quality gate (shared with the repo-install path):
            // a pack may only install valid skills.
            crate::validation::ensure_installable(&source).map_err(|reason| {
                anyhow::anyhow!("Skill '{}' is not installable: {reason}", skill.name)
            })?;
            let hash = crate::content::snapshot_path(&skill.name, &source)
                .with_context(|| {
                    format!("Failed to capture content baseline for '{}'", skill.name)
                })?
                .content_hash;
            Ok((skill.name.clone(), hash))
        })
        .collect::<Result<_>>()?;

    // Load every persisted input before changing hub links. A malformed or
    // unreadable lockfile must fail closed without a partial pack install.
    let _lock2 = crate::lockfile::get_mutex()
        .lock()
        .map_err(|_| anyhow!("Lockfile mutex poisoned"))?;
    let lf_path = crate::lockfile::lockfile_path();
    let previous_lockfile = std::fs::read(&lf_path).ok();
    let mut lf = crate::lockfile::Lockfile::load(&lf_path)?;

    // Stage all skills to temp dirs
    // (name, temp_dir, target_dir, previous symlink target)
    let mut staged: Vec<(String, PathBuf, PathBuf, Option<PathBuf>)> = Vec::new();
    let mut installed_names = Vec::new();

    for (index, skill) in manifest.skills.iter().enumerate() {
        let src_path = resolve_pack_skill_source(repo_dir, &skill.path)?;
        let target_dir = hub.join(&skill.name);
        let temp_dir = hub.join(format!(".importing-pack-{index}"));

        // Clean stale temp dir if exists
        if temp_dir.symlink_metadata().is_ok()
            && let Err(error) = skillstar_core::infra::fs_ops::remove_link_or_copy(&temp_dir)
        {
            rollback_pack_links(&staged, 0);
            return Err(error).with_context(|| {
                format!(
                    "Failed to clear stale pack staging '{}'",
                    temp_dir.display()
                )
            });
        }

        // Stage: symlink temp_dir → src_path (same pattern as repo_scanner.rs:710)
        if let Err(error) = skillstar_core::infra::fs_ops::create_symlink(&src_path, &temp_dir) {
            rollback_pack_links(&staged, 0);
            return Err(error)
                .with_context(|| format!("Failed to stage skill '{}' for pack", skill.name));
        }

        let previous_target = if skillstar_core::infra::fs_ops::is_link(&target_dir) {
            Some(
                skillstar_core::infra::fs_ops::read_link_resolved(&target_dir).with_context(
                    || format!("Failed to record existing Skill link '{}'", skill.name),
                )?,
            )
        } else {
            None
        };

        staged.push((skill.name.clone(), temp_dir, target_dir, previous_target));
    }

    // Validate: check for conflicts with existing skills
    for (name, _temp_dir, target_dir, _) in &staged {
        if target_dir.exists() && !skillstar_core::infra::fs_ops::is_link(target_dir) {
            // Existing non-symlink skill conflicts — abort
            // Clean up all staged temp dirs
            for (_, td, _, _) in &staged {
                let _ = skillstar_core::infra::fs_ops::remove_link_or_copy(td);
            }
            bail!(
                "Skill '{}' already exists in hub (not a symlink). Remove it before installing pack '{}'.",
                name,
                manifest.name
            );
        }
    }

    // Rename all temp dirs to target dirs (atomic-ish, same as skill_bundle.rs)
    let mut renamed_count = 0usize;
    for (name, temp_dir, target_dir, _) in &staged {
        // Remove existing symlink if present
        if skillstar_core::infra::fs_ops::is_link(target_dir) || target_dir.exists() {
            if skillstar_core::infra::fs_ops::is_link(target_dir) {
                let _ = skillstar_core::infra::fs_ops::remove_symlink(target_dir);
            } else {
                let _ = std::fs::remove_dir_all(target_dir);
            }
        }

        match std::fs::rename(temp_dir, target_dir) {
            Ok(()) => {
                renamed_count += 1;
                installed_names.push(name.clone());
            }
            Err(e) => {
                // The current target may already have had its previous link
                // removed, so include it in the restoration set.
                rollback_pack_links(&staged, renamed_count + 1);
                bail!(
                    "Failed to install skill '{}' for pack '{}': {}. Rolled back {} skills.",
                    name,
                    manifest.name,
                    e,
                    renamed_count
                );
            }
        }
    }

    // Update lockfile for each installed skill
    let cache_dir_name = crate::repo_scanner::cache_dir_name(source);
    let tree_hash = crate::git::ops::compute_tree_hash(repo_dir).unwrap_or_default();

    for skill in &manifest.skills {
        let source_folder = if skill.path == "." || skill.path.is_empty() {
            None
        } else {
            Some(skill.path.clone())
        };
        let content_hash = baselines
            .iter()
            .find(|(name, _)| name == &skill.name)
            .map(|(_, hash)| hash.clone())
            .expect("every manifest Skill was preflighted");
        lf.upsert(crate::lockfile::LockEntry {
            name: skill.name.clone(),
            git_url: repo_url.to_string(),
            git_ref: None,
            tree_hash: tree_hash.clone(),
            content_hash: Some(content_hash),
            content_hash_version: Some(crate::content::SNAPSHOT_HASH_VERSION),
            installed_at: chrono::Utc::now().to_rfc3339(),
            source_folder,
        });
    }
    if let Err(error) = lf.save(&lf_path) {
        rollback_pack_links(&staged, renamed_count);
        return Err(error).context("Failed to save lockfile while installing pack");
    }

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
    if let Err(error) = save_store(&store) {
        restore_persisted_file(&lf_path, previous_lockfile.as_deref());
        rollback_pack_links(&staged, renamed_count);
        return Err(error).context("Failed to save pack registry; installation was rolled back");
    }

    // Execute post_install if present
    if let Some(ref post_install) = manifest.post_install {
        let script_path = repo_dir.join(&post_install.script);
        if script_path.exists() {
            let result = execute_post_install(&script_path, repo_dir, post_install.timeout_secs);
            let mut store = match load_store_strict() {
                Ok(store) => store,
                Err(error) => {
                    tracing::warn!(
                        pack = %manifest.name,
                        error = %error,
                        "pack installed but post-install registry could not be reloaded"
                    );
                    crate::installed_skill::invalidate_cache();
                    return Ok(installed_names);
                }
            };
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
                if let Err(error) = save_store(&store) {
                    tracing::warn!(
                        pack = %manifest.name,
                        error = %error,
                        "pack installed but post-install result could not be persisted"
                    );
                }
            }
        }
    }

    // Invalidate installed skill cache
    crate::installed_skill::invalidate_cache();

    Ok(installed_names)
}

fn rollback_pack_links(
    staged: &[(String, PathBuf, PathBuf, Option<PathBuf>)],
    renamed_count: usize,
) {
    for (_, _, target, previous_target) in staged.iter().take(renamed_count).rev() {
        if target.symlink_metadata().is_ok() {
            let _ = skillstar_core::infra::fs_ops::remove_link_or_copy(target);
        }
        if let Some(previous_target) = previous_target {
            let _ = skillstar_core::infra::fs_ops::create_symlink(previous_target, target);
        }
    }
    for (_, temp, _, _) in staged {
        if temp.symlink_metadata().is_ok() {
            let _ = skillstar_core::infra::fs_ops::remove_link_or_copy(temp);
        }
    }
}

/// Which interpreter `execute_post_install` will use for a given script, given
/// the current platform and the script's file extension.
///
/// This is split out purely so the platform/extension selection logic has test
/// coverage on every OS — the actual `Command` construction is not unit
/// testable because it shells out.
//
// `PowerShell` / `Cmd` are only constructed under `#[cfg(windows)]`, so on
// other targets they look unused; silence the per-platform dead-code warning.
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
#[allow(dead_code)]
enum PostInstallInterpreter {
    /// `sh -c <script>` — the Unix default and the Windows fallback for `.sh`
    /// (works when Git Bash / WSL is installed).
    Sh,
    /// `powershell -NoProfile -ExecutionPolicy Bypass -File <script>`
    PowerShell,
    /// `cmd /C <script>`
    Cmd,
}

impl PostInstallInterpreter {
    fn program(self) -> &'static str {
        match self {
            PostInstallInterpreter::Sh => "sh",
            PostInstallInterpreter::PowerShell => "powershell",
            PostInstallInterpreter::Cmd => "cmd",
        }
    }
}

/// Pick the interpreter for a post-install script based on the current
/// platform and the script's extension.
fn post_install_interpreter(ext: &str) -> PostInstallInterpreter {
    #[cfg(windows)]
    {
        match ext {
            "ps1" => PostInstallInterpreter::PowerShell,
            "bat" | "cmd" => PostInstallInterpreter::Cmd,
            // Extensionless or `.sh` — try `sh` so Git Bash can still run it.
            _ => PostInstallInterpreter::Sh,
        }
    }
    #[cfg(not(windows))]
    {
        let _ = ext;
        PostInstallInterpreter::Sh
    }
}

/// Execute a post-install script with a hard timeout.
/// Returns the exit code (0 = success; -1 = spawn/interpreter failure; -2 =
/// killed after `timeout_secs` elapsed).
///
/// On Unix the script is run by `sh <path>` (never `sh -c`, so the script
/// *path* can never be interpreted as shell code — the path has already been
/// validated as repository-relative, but shell metacharacters are legal in
/// paths). On Windows the interpreter is selected by the script file
/// extension: `.ps1` → PowerShell, `.bat`/`.cmd` → `cmd /C "<path>"`, anything
/// else (including extensionless `.sh`) falls back to `sh <path>` so a Git
/// Bash install can still run bash-style scripts.
///
/// The child is killed once `timeout_secs` elapses: a hung `while true` pack
/// script must not stall the install forever — the caller holds the global
/// update transaction lock while this runs, so an unbounded wait would block
/// every skill mutation in the app.
fn execute_post_install(script: &Path, working_dir: &Path, timeout_secs: u64) -> i32 {
    use skillstar_core::infra::path_env::command_with_path;
    use std::time::{Duration, Instant};

    let ext = script.extension().and_then(|e| e.to_str()).unwrap_or("");
    let script_str = script.to_string_lossy();
    let interpreter = post_install_interpreter(ext);

    let mut cmd = command_with_path(interpreter.program());
    match interpreter {
        PostInstallInterpreter::Sh => {
            cmd.arg(script_str.as_ref());
        }
        PostInstallInterpreter::PowerShell => {
            cmd.args([
                "-NoProfile",
                "-NonInteractive",
                "-ExecutionPolicy",
                "Bypass",
                "-File",
                script_str.as_ref(),
            ]);
        }
        PostInstallInterpreter::Cmd => {
            // Quote the path: cmd treats `&`/`|` as separators outside quotes.
            // The script path is repository-relative and never contains `"`.
            cmd.arg("/C").arg(format!("\"{script_str}\""));
        }
    }

    cmd.current_dir(working_dir);

    let mut child = match cmd.spawn() {
        Ok(child) => child,
        Err(e) => {
            tracing::warn!(
                "post_install script {:?} could not be executed via {} (not on PATH? {})",
                script,
                interpreter.program(),
                e
            );
            // Distinct sentinel so the caller's "exited with code -1" message
            // stays accurate while the cause is logged above.
            return -1;
        }
    };

    let deadline = Instant::now() + Duration::from_secs(timeout_secs.max(1));
    loop {
        match child.try_wait() {
            Ok(Some(status)) => return status.code().unwrap_or(-1),
            Ok(None) => {
                if Instant::now() >= deadline {
                    let _ = child.kill();
                    let _ = child.wait();
                    tracing::warn!(
                        "post_install script {:?} killed after exceeding its {timeout_secs}s timeout",
                        script
                    );
                    return -2;
                }
                std::thread::sleep(Duration::from_millis(100));
            }
            Err(e) => {
                tracing::warn!("post_install script {:?} wait failed: {e}", script);
                return -1;
            }
        }
    }
}

mod management;
pub use management::{DoctorCheck, DoctorReport, doctor_all, doctor_pack, list_packs, remove_pack};
#[cfg(test)]
#[path = "skill_pack/tests.rs"]
mod tests;
