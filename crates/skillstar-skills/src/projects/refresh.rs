//! Keep copy-deployed skills in sync with the hub (post-git-pull refresh).

use anyhow::{Context, Result};

use super::index::list_projects;
use super::store::load_skills_list;
use super::types::ensure_project_root_exists;
use skillstar_agents as agent_profile;
use skillstar_core::infra::{fs_ops, paths as fs_paths};

/// Refresh copy-deployed skills whose content has drifted from the hub.
///
/// For each skill in the project's `skills-list.json`:
/// 1. Skip symlinks — they always point to the live hub entry.
/// 2. Skip skills that no longer exist in the project directory (the user
///    removed them on purpose; we must not re-copy).
/// 3. For remaining copy-deployed skills, hash both the project copy and the
///    hub source. If they differ, delete the project copy and re-deploy.
///
/// Returns the number of skills that were refreshed.
pub fn refresh_stale_copies(project_path: &str) -> Result<u32> {
    let report = refresh_stale_copies_inner(project_path, None)?;
    Ok(report.refreshed)
}

#[derive(Debug, Default)]
pub(super) struct RefreshStaleCopiesReport {
    pub refreshed: u32,
    pub failures: Vec<String>,
}

/// Strict, scoped variant used by subscription transactions. Unlike the
/// user-initiated maintenance command, every failed copy reconciliation is
/// returned to the caller so it can abort and roll back the installation.
pub(super) fn refresh_stale_copies_strict(
    project_path: &str,
    skills: &[String],
) -> Result<RefreshStaleCopiesReport> {
    refresh_stale_copies_inner(project_path, Some(skills))
}

fn refresh_stale_copies_inner(
    project_path: &str,
    only_skills: Option<&[String]>,
) -> Result<RefreshStaleCopiesReport> {
    let hub_dir = fs_paths::hub_skills_dir();
    let profiles = agent_profile::list_profiles();
    let project = ensure_project_root_exists(project_path)?;

    // Find project name for loading skills-list.json
    let projects = list_projects();
    let entry = projects.iter().find(|p| p.path == project_path);
    let Some(entry) = entry else {
        // Not a registered project — nothing to refresh
        return Ok(RefreshStaleCopiesReport::default());
    };
    let skills_list = match load_skills_list(&entry.name) {
        Some(list) => list,
        None => return Ok(RefreshStaleCopiesReport::default()),
    };

    let mut report = RefreshStaleCopiesReport::default();

    for (agent_id, skill_names) in &skills_list.agents {
        let Some(profile) = profiles.iter().find(|p| &p.id == agent_id) else {
            continue;
        };
        if !profile.has_project_skills() {
            continue;
        }

        let target_dir = project.join(&profile.project_skills_rel);
        for skill_name in skill_names {
            if only_skills.is_some_and(|selected| !selected.iter().any(|name| name == skill_name)) {
                continue;
            }
            let target = target_dir.join(skill_name);

            // 1. Skip symlinks — they are always up-to-date
            if fs_ops::is_link(&target) {
                continue;
            }

            // 2. Skip if not present in project (user deleted it)
            if !target.is_dir() {
                continue;
            }

            // 3. Check the hub source exists
            let source = hub_dir.join(skill_name);
            if !source.exists() {
                if only_skills.is_some() {
                    report
                        .failures
                        .push(format!("{agent_id}/{skill_name}: hub source is missing"));
                }
                continue;
            }

            // 4. Compare hashes
            let hub_hash = match dir_content_hash(&source) {
                Ok(h) => h,
                Err(e) => {
                    report
                        .failures
                        .push(format!("{agent_id}/{skill_name}: hash hub copy: {e:#}"));
                    tracing::warn!(
                        target: "sync",
                        skill = %skill_name,
                        error = %e,
                        "Failed to hash hub skill, skipping refresh"
                    );
                    continue;
                }
            };
            let project_hash = match dir_content_hash(&target) {
                Ok(h) => h,
                Err(e) => {
                    report
                        .failures
                        .push(format!("{agent_id}/{skill_name}: hash Project copy: {e:#}"));
                    tracing::warn!(
                        target: "sync",
                        skill = %skill_name,
                        error = %e,
                        "Failed to hash project skill copy, skipping refresh"
                    );
                    continue;
                }
            };

            if hub_hash == project_hash {
                continue;
            }

            // 5. Hashes differ → refresh: remove old copy, re-deploy
            tracing::info!(
                target: "sync",
                skill = %skill_name,
                agent = %agent_id,
                "Copy-deployed skill is stale, refreshing from hub"
            );
            if let Err(e) = fs_ops::remove_link_or_copy(&target) {
                report.failures.push(format!(
                    "{agent_id}/{skill_name}: remove stale Project copy: {e:#}"
                ));
                tracing::warn!(
                    target: "sync",
                    skill = %skill_name,
                    error = %e,
                    "Failed to remove stale copy, skipping"
                );
                continue;
            }
            // This branch is reached only for copy deployments (symlinks and
            // absent entries are skipped above), so re-deploy as a copy. Going
            // through the symlink-first path here would silently downgrade a
            // copy the user explicitly chose, while `deploy_modes` still says
            // `copy`.
            match fs_ops::create_copy_deploy(&source, &target) {
                Ok(()) => report.refreshed += 1,
                Err(e) => {
                    report.failures.push(format!(
                        "{agent_id}/{skill_name}: redeploy Project copy: {e:#}"
                    ));
                    tracing::warn!(
                        target: "sync",
                        skill = %skill_name,
                        error = %e,
                        "Failed to re-deploy skill after stale copy removal"
                    );
                }
            }
        }
    }

    if report.refreshed > 0 {
        tracing::info!(
            target: "sync",
            refreshed = report.refreshed,
            project = %project_path,
            "Refreshed stale copy-deployed skills"
        );
    }

    Ok(report)
}

/// Compute a lightweight content hash of a directory tree.
///
/// Walks all files recursively (sorted by relative path for determinism),
/// hashing each file's relative path and contents. Skips `.git` directories.
/// Returns a hex-encoded SHA-256 digest.
fn dir_content_hash(dir: &std::path::Path) -> Result<String> {
    use sha2::{Digest, Sha256};
    use std::collections::BTreeSet;
    use std::io::Read;

    let mut file_paths = BTreeSet::new();
    collect_files(dir, dir, &mut file_paths)?;

    let mut hasher = Sha256::new();
    for rel in &file_paths {
        hasher.update(rel.as_bytes());
        let abs = dir.join(rel);
        let mut file = std::fs::File::open(&abs)
            .with_context(|| format!("failed to open '{}' while hashing", abs.display()))?;
        let mut buf = [0u8; 8192];
        loop {
            let n = file
                .read(&mut buf)
                .with_context(|| format!("failed to read '{}' while hashing", abs.display()))?;
            if n == 0 {
                break;
            }
            hasher.update(&buf[..n]);
        }
    }
    let digest = hasher.finalize();
    let mut hex = String::with_capacity(64);
    const HEX_TABLE: &[u8; 16] = b"0123456789abcdef";
    for &byte in &digest {
        hex.push(HEX_TABLE[(byte >> 4) as usize] as char);
        hex.push(HEX_TABLE[(byte & 0xf) as usize] as char);
    }
    Ok(hex)
}

/// Recursively collect relative file paths under `root`, skipping `.git`.
fn collect_files(
    base: &std::path::Path,
    dir: &std::path::Path,
    out: &mut std::collections::BTreeSet<String>,
) -> Result<()> {
    let entries = std::fs::read_dir(dir)
        .with_context(|| format!("failed to list '{}' while hashing", dir.display()))?;
    for entry in entries {
        let entry =
            entry.with_context(|| format!("failed to read an entry in '{}'", dir.display()))?;
        let path = entry.path();
        let name = entry.file_name();
        if name == ".git" {
            continue;
        }
        if path.is_dir() && !fs_ops::is_link(&path) {
            collect_files(base, &path, out)?;
        } else if path.is_file()
            && let Ok(rel) = path.strip_prefix(base)
        {
            out.insert(rel.to_string_lossy().to_string());
        }
    }
    Ok(())
}
