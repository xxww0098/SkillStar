use super::*;

// ── Remove ───────────────────────────────────────────────────────────

/// Remove an installed pack by name.
/// Cleans symlinks from hub, removes lockfile entries, updates packs.json.
/// Does NOT delete the repo cache (allows reinstall without re-clone).
pub fn remove_pack(name: &str) -> Result<Vec<String>> {
    let _transaction_guard = crate::skill_update::acquire_update_transaction_lock()?;
    let _lock = get_mutex()
        .lock()
        .map_err(|_| anyhow!("Pack store mutex poisoned"))?;

    let mut store = load_store_strict()?;
    let pack_idx = store
        .packs
        .iter()
        .position(|p| p.name == name)
        .ok_or_else(|| anyhow!("Pack '{}' not found", name))?;

    let pack = &store.packs[pack_idx];
    for skill in &pack.skills {
        crate::shared_channels::ensure_generic_skill_mutation_allowed(&skill.name)?;
    }
    let hub = skillstar_core::infra::paths::hub_skills_dir();
    let mut removed = Vec::new();

    for skill in &pack.skills {
        let skill_path = hub.join(&skill.name);
        if skillstar_core::infra::fs_ops::is_link(&skill_path) || skill_path.exists() {
            if skillstar_core::infra::fs_ops::is_link(&skill_path) {
                match skillstar_core::infra::fs_ops::remove_symlink(&skill_path) {
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
    let _lock2 = crate::lockfile::get_mutex()
        .lock()
        .map_err(|_| anyhow!("Lockfile mutex poisoned"))?;
    let lf_path = crate::lockfile::lockfile_path();
    let mut lf = crate::lockfile::Lockfile::load(&lf_path)?;
    for skill in &pack.skills {
        lf.remove(&skill.name);
    }
    lf.save(&lf_path)?;

    // Remove from packs.json
    store.packs.remove(pack_idx);
    save_store(&store)?;

    // Invalidate cache
    crate::installed_skill::invalidate_cache();

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
    let hub_root = skillstar_core::infra::paths::hub_root();
    let direct = hub_root.join(configured);
    if direct.exists() {
        return direct;
    }

    if let Some(suffix) = configured.strip_prefix(".repos/") {
        let migrated = skillstar_core::infra::paths::repos_cache_dir().join(suffix);
        if migrated.exists() {
            return migrated;
        }
    }

    if let Some(suffix) = configured.strip_prefix("repos/") {
        let current = skillstar_core::infra::paths::repos_cache_dir().join(suffix);
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

    let hub = skillstar_core::infra::paths::hub_skills_dir();
    let mut checks = Vec::new();
    let mut all_healthy = true;

    // Check 1: Every skill has a valid symlink in hub
    let mut missing_skills = Vec::new();
    let mut broken_links = Vec::new();
    for skill in &pack.skills {
        let skill_path = hub.join(&skill.name);
        if !skill_path.exists() && !skillstar_core::infra::fs_ops::is_link(&skill_path) {
            missing_skills.push(skill.name.clone());
        } else if skillstar_core::infra::fs_ops::is_link(&skill_path) {
            let broken = match skillstar_core::infra::fs_ops::read_link_resolved(&skill_path) {
                Ok(p) => !p.exists(),
                Err(_) => true,
            };
            if broken {
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
