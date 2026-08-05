use crate::deployment;
use crate::frontmatter::{render_with_front_matter, split_front_matter};
use crate::git::ops as git_ops;
use crate::{installed_skill, local_skill, lockfile, projects, repo_scanner};
use serde_yaml::{Mapping, Value};
use skillstar_core::infra::{fs_ops, paths};
use skillstar_core::types::{
    Skill, SkillCategory, SkillType, extract_github_source_from_url, extract_skill_description,
};
use std::path::{Path, PathBuf};
use tracing::warn;

fn derive_name_hint(url: &str, name: Option<&str>) -> String {
    name.map(str::to_string).unwrap_or_else(|| {
        url.rsplit('/')
            .next()
            .unwrap_or("skill")
            .trim_end_matches(".git")
            .to_string()
    })
}

fn find_target_skill<'a>(
    skills_found: &'a [repo_scanner::DiscoveredSkill],
    requested_name: Option<&str>,
    name_hint: &str,
) -> Option<&'a repo_scanner::DiscoveredSkill> {
    // Single-skill repo: return it directly without name matching.
    if skills_found.len() == 1 {
        return skills_found.first();
    }

    // Single-pass: match exact first, then case-insensitive fallback.
    // Avoids double iteration of skills_found.
    let search_key = requested_name.unwrap_or(name_hint);
    let search_key_lower = search_key.to_lowercase();

    skills_found
        .iter()
        .find(|s| s.id == search_key || s.id.to_lowercase() == search_key_lower)
}

/// Normalize URL, materialize repo cache, run lockfile-aware scan.
pub fn fetch_repo_scanned(
    url: &str,
    full_depth: bool,
) -> Result<(String, String, PathBuf, Vec<repo_scanner::DiscoveredSkill>), String> {
    fetch_repo_scanned_in_session(
        url,
        full_depth,
        &crate::git::transport::GitOperationSession::public(),
    )
}

pub fn fetch_repo_scanned_in_session(
    url: &str,
    full_depth: bool,
    session: &crate::git::transport::GitOperationSession,
) -> Result<(String, String, PathBuf, Vec<repo_scanner::DiscoveredSkill>), String> {
    fetch_repo_scanned_detailed_in_session(url, full_depth, session)
        .map_err(|error| format!("{error:#}"))
}

pub(crate) fn fetch_repo_scanned_detailed_in_session(
    url: &str,
    full_depth: bool,
    session: &crate::git::transport::GitOperationSession,
) -> anyhow::Result<(String, String, PathBuf, Vec<repo_scanner::DiscoveredSkill>)> {
    use anyhow::Context as _;
    let parsed = crate::source_resolver::Source::parse(url)
        .map_err(|error| anyhow::anyhow!("Invalid source: {error}"))?;
    let repo_dir = repo_scanner::clone_or_fetch_repo_at_in_session(
        &parsed.repo_url,
        &parsed.short,
        parsed.git_ref.as_deref(),
        session,
    )
    .context("Failed to fetch repo")?;
    let mut skills_found = match parsed.subpath.as_deref() {
        Some(subpath) => {
            repo_scanner::scan_skills_in_repo_at(&repo_dir, &parsed.repo_url, subpath, full_depth)
        }
        None => repo_scanner::scan_skills_in_repo(&repo_dir, &parsed.repo_url, full_depth),
    };
    if let Some(skill_filter) = parsed.skill_filter.as_deref() {
        skills_found.retain(|skill| skill.id.eq_ignore_ascii_case(skill_filter));
    }
    Ok((parsed.repo_url, parsed.short, repo_dir, skills_found))
}

#[inline]
fn local_skill_blocks_repo_install(skill_id: &str) -> bool {
    paths::local_skills_dir().join(skill_id).exists()
}

/// Compute tree hash directly from an installed skill's path.
///
/// For symlinked (repo-cached) skills this resolves the symlink target
/// and computes the hash from the real directory, avoiding a redundant
/// lockfile re-read that `install_from_repo` has just written to.
fn compute_tree_hash_for(skills_dir: &Path, installed_name: &str) -> Option<String> {
    let skill_path = skills_dir.join(installed_name);
    let effective_path = fs_ops::read_link_resolved(&skill_path).unwrap_or(skill_path);
    git_ops::compute_tree_hash(&effective_path).ok()
}

fn new_skill_from_install(
    name: String,
    description: String,
    git_url: String,
    tree_hash: Option<String>,
) -> Skill {
    let source = extract_github_source_from_url(&git_url);
    Skill {
        name,
        description,
        localized_description: None,
        skill_type: SkillType::Hub,
        stars: 0,
        installed: true,
        update_available: false,
        last_updated: chrono::Utc::now().to_rfc3339(),
        git_url,
        tree_hash,
        category: SkillCategory::None,
        author: None,
        topics: Vec::new(),
        agent_links: Some(Vec::new()),
        rank: None,
        source,
    }
}

#[derive(Debug, Clone, Copy)]
struct RepoInstallProvenance<'a> {
    git_url: &'a str,
    source_folder: Option<&'a str>,
}

fn skill_markdown_path(skill_dir: &Path) -> PathBuf {
    skill_dir.join("SKILL.md")
}

fn repo_install_provenance_mapping(provenance: RepoInstallProvenance<'_>) -> Mapping {
    let mut mapping = Mapping::new();
    mapping.insert(
        Value::String("repository_url".to_string()),
        Value::String(provenance.git_url.to_string()),
    );

    if let Some(source_folder) = provenance.source_folder.filter(|value| !value.is_empty()) {
        mapping.insert(
            Value::String("source_folder".to_string()),
            Value::String(source_folder.to_string()),
        );
    }

    mapping
}

fn merge_provenance_value(existing: Option<Value>, provenance: RepoInstallProvenance<'_>) -> Value {
    let mut merged = match existing {
        Some(Value::Mapping(mapping)) => mapping,
        _ => Mapping::new(),
    };

    for (key, value) in repo_install_provenance_mapping(provenance) {
        merged.insert(key, value);
    }

    Value::Mapping(merged)
}

#[cfg(test)]
fn write_repo_install_provenance(
    skill_dir: &Path,
    provenance: RepoInstallProvenance<'_>,
) -> Result<(), String> {
    let skill_md_path = skill_markdown_path(skill_dir);
    let existing = std::fs::read_to_string(&skill_md_path)
        .map_err(|e| format!("Failed to read '{}': {}", skill_md_path.display(), e))?;
    let rendered = render_repo_install_provenance(&existing, provenance);
    skillstar_core::infra::fs_ops::atomic_write(&skill_md_path, rendered.as_bytes())
        .map_err(|e| format!("Failed to write '{}': {}", skill_md_path.display(), e))
}

fn render_repo_install_provenance(existing: &str, provenance: RepoInstallProvenance<'_>) -> String {
    let split = split_front_matter(existing);
    let mut front_matter = split.data;
    let existing_provenance = front_matter.remove("provenance");
    front_matter.insert(
        "provenance".to_string(),
        merge_provenance_value(existing_provenance, provenance),
    );

    render_with_front_matter(Some(&front_matter), &split.body)
}

struct ProvenanceEdit {
    path: PathBuf,
    original: Vec<u8>,
    rendered: Vec<u8>,
}

fn rollback_repo_cache_installs(skills_dir: &Path, names: &[String], edits: &[ProvenanceEdit]) {
    for edit in edits {
        let _ = fs_ops::atomic_write(&edit.path, &edit.original);
    }
    for name in names {
        let path = skills_dir.join(name);
        if path.symlink_metadata().is_ok() {
            let _ = fs_ops::remove_link_or_copy(&path);
        }
    }

    if let Ok(_lock) = lockfile::get_mutex().lock() {
        let lock_path = lockfile::lockfile_path();
        if let Ok(mut lockfile) = lockfile::Lockfile::load(&lock_path) {
            for name in names {
                lockfile.remove(name);
            }
            let _ = lockfile.save(&lock_path);
        }
    }
    installed_skill::invalidate_cache();
}

fn finalize_repo_cache_installs(
    skills_dir: &Path,
    repo_url: &str,
    installed: &[String],
    targets: &[repo_scanner::SkillInstallTarget],
) -> Result<Vec<Skill>, String> {
    let mut edits = Vec::with_capacity(installed.len());
    let result = (|| {
        // Render every edit before the first write. This keeps malformed or
        // unreadable Skill metadata from producing a partially finalized batch.
        for name in installed {
            let source_folder = targets
                .iter()
                .find(|target| target.id == *name)
                .map(|target| target.folder_path.as_str());
            let path = skill_markdown_path(&skills_dir.join(name));
            let original = std::fs::read(&path)
                .map_err(|error| format!("Failed to read '{}': {error}", path.display()))?;
            let existing = std::str::from_utf8(&original).map_err(|error| {
                format!("Skill metadata '{}' is not UTF-8: {error}", path.display())
            })?;
            let rendered = render_repo_install_provenance(
                existing,
                RepoInstallProvenance {
                    git_url: repo_url,
                    source_folder,
                },
            )
            .into_bytes();
            edits.push(ProvenanceEdit {
                path,
                original,
                rendered,
            });
        }

        for edit in &edits {
            fs_ops::atomic_write(&edit.path, &edit.rendered)
                .map_err(|error| format!("Failed to write '{}': {error}", edit.path.display()))?;
        }

        let baselines = installed
            .iter()
            .map(|name| {
                crate::content::snapshot(name)
                    .map(|snapshot| (name.clone(), snapshot.content_hash))
                    .map_err(|error| {
                        format!("Failed to capture installed Skill '{name}' baseline: {error}")
                    })
            })
            .collect::<Result<Vec<_>, _>>()?;

        {
            let _lock = lockfile::get_mutex()
                .lock()
                .map_err(|_| "Lockfile mutex poisoned".to_string())?;
            let lock_path = lockfile::lockfile_path();
            let mut lockfile = lockfile::Lockfile::load(&lock_path).map_err(|error| {
                format!("Failed to load lockfile '{}': {error}", lock_path.display())
            })?;
            for (name, content_hash) in &baselines {
                let entry = lockfile
                    .skills
                    .iter_mut()
                    .find(|entry| entry.name == *name)
                    .ok_or_else(|| {
                        format!("Installed Skill '{name}' is missing from the lockfile")
                    })?;
                entry.content_hash = Some(content_hash.clone());
                entry.content_hash_version = Some(crate::content::SNAPSHOT_HASH_VERSION);
            }
            lockfile.save(&lock_path).map_err(|error| {
                format!("Failed to save lockfile '{}': {error}", lock_path.display())
            })?;
        }

        let skills = installed
            .iter()
            .map(|name| {
                let dest = skills_dir.join(name);
                new_skill_from_install(
                    name.clone(),
                    extract_skill_description(&dest),
                    repo_url.to_string(),
                    compute_tree_hash_for(skills_dir, name),
                )
            })
            .collect();
        Ok(skills)
    })();

    if result.is_err() {
        rollback_repo_cache_installs(skills_dir, installed, &edits);
    }
    result
}

fn try_install_from_repo_cache(
    url: &str,
    requested_name: Option<&str>,
    name_hint: &str,
    skills_dir: &Path,
    session: &crate::git::transport::GitOperationSession,
) -> Result<Option<Skill>, String> {
    let Ok((repo_url, _source, repo_dir, skills_found)) =
        fetch_repo_scanned_in_session(url, false, session)
    else {
        return Ok(None);
    };
    let parsed =
        crate::source_resolver::Source::parse(url).map_err(|e| format!("Invalid source: {e}"))?;
    let target = find_target_skill(&skills_found, requested_name, name_hint);

    // Guard against overwriting a local skill whose name matches the repo skill
    if let Some(skill) = &target
        && local_skill_blocks_repo_install(&skill.id)
    {
        warn!(
            target: "install_skill",
            skill_id = %skill.id,
            "repo-cache skill would collide with existing local skill, skipping"
        );
        return Ok(None);
    }

    let Some(skill) = target else {
        warn!(
            target: "install_skill",
            hint = %name_hint,
            found = ?skills_found.iter().map(|s| &s.id).collect::<Vec<_>>(),
            "skill not found in repo, falling back to direct clone"
        );
        return Ok(None);
    };

    // This install path owns only new hub entries. In particular, do not let
    // the scanner replace a same-source link: finalization rollback is allowed
    // to remove the entries it just created, never a pre-existing install.
    let existing_path = skills_dir.join(&skill.id);
    if existing_path.symlink_metadata().is_ok() {
        return Err(format!("Skill '{}' is already installed", skill.id));
    }

    let targets = vec![repo_scanner::SkillInstallTarget {
        id: skill.id.clone(),
        folder_path: skill.folder_path.clone(),
    }];

    match repo_scanner::install_from_repo_at(
        &repo_dir,
        &repo_url,
        parsed.git_ref.as_deref(),
        &targets,
    ) {
        Ok(installed) if !installed.is_empty() => {
            let mut installed_skills =
                finalize_repo_cache_installs(skills_dir, &repo_url, &installed, &targets)?;
            installed_skill::invalidate_cache();
            Ok(installed_skills.pop())
        }
        Ok(_) => Ok(None),
        Err(err) => {
            warn!(target: "install_skill", error = %err, "repo-cache install failed, falling back");
            Ok(None)
        }
    }
}

pub fn install_skill(url: String, name: Option<String>) -> Result<Skill, String> {
    install_skill_in_session(
        url,
        name,
        &crate::git::transport::GitOperationSession::public(),
    )
}

pub fn install_skill_in_session(
    url: String,
    name: Option<String>,
    session: &crate::git::transport::GitOperationSession,
) -> Result<Skill, String> {
    let skills_dir = paths::hub_skills_dir();
    let name_hint = derive_name_hint(&url, name.as_deref());
    crate::content::validate_skill_name(&name_hint)
        .map_err(|error| format!("Invalid Skill name: {error}"))?;

    if skills_dir.join(&name_hint).symlink_metadata().is_ok() {
        return Err(format!("Skill '{}' is already installed", name_hint));
    }
    if local_skill_blocks_repo_install(&name_hint) {
        return Err(format!(
            "Skill '{}' already exists as a local skill",
            name_hint
        ));
    }

    if let Some(skill) =
        try_install_from_repo_cache(&url, name.as_deref(), &name_hint, &skills_dir, session)?
    {
        return Ok(skill);
    }

    let dest = skills_dir.join(&name_hint);
    if dest.symlink_metadata().is_ok() {
        return Err(format!("Skill '{}' is already installed", name_hint));
    }

    if let Err(error) = git_ops::clone_repo_in_session(&url, &dest, session) {
        let _ = fs_ops::remove_dir_all_retry(&dest);
        return Err(error.to_string());
    }
    let result = (|| -> Result<Skill, String> {
        let tree_hash = git_ops::compute_tree_hash(&dest).map_err(|e| e.to_string())?;
        let content_hash = crate::content::snapshot(&name_hint)
            .map_err(|e| format!("Failed to capture installed Skill baseline: {e}"))?
            .content_hash;

        let _lock = lockfile::get_mutex()
            .lock()
            .map_err(|_| "Lockfile mutex poisoned".to_string())?;
        let lock_path = lockfile::lockfile_path();
        let mut lockfile = lockfile::Lockfile::load(&lock_path)
            .map_err(|e| format!("Failed to load lockfile '{}': {}", lock_path.display(), e))?;
        lockfile.upsert(crate::lockfile::LockEntry {
            name: name_hint.clone(),
            git_url: url.clone(),
            git_ref: None,
            tree_hash: tree_hash.clone(),
            content_hash: Some(content_hash),
            content_hash_version: Some(crate::content::SNAPSHOT_HASH_VERSION),
            installed_at: chrono::Utc::now().to_rfc3339(),
            source_folder: None,
        });
        lockfile
            .save(&lock_path)
            .map_err(|e| format!("Failed to save lockfile '{}': {}", lock_path.display(), e))?;
        installed_skill::invalidate_cache();

        let description = extract_skill_description(&dest);
        Ok(new_skill_from_install(
            name_hint.clone(),
            description,
            url.clone(),
            Some(tree_hash),
        ))
    })();
    if result.is_err() {
        let _ = skillstar_core::infra::fs_ops::remove_dir_all_retry(&dest);
    }
    result
}

/// Install multiple skills from the same repository URL in a single batch.
/// This prevents git clone/fetch overlap and lockfile serialization issues when
/// multiple skills share the same repository.
pub fn install_skills_batch(url: &str, names: &[String]) -> Result<Vec<Skill>, String> {
    install_skills_batch_in_session(
        url,
        names,
        &crate::git::transport::GitOperationSession::public(),
    )
}

pub fn install_skills_batch_in_session(
    url: &str,
    names: &[String],
    session: &crate::git::transport::GitOperationSession,
) -> Result<Vec<Skill>, String> {
    if names.is_empty() {
        return Ok(Vec::new());
    }

    let skills_dir = paths::hub_skills_dir();
    let (repo_url, _source, repo_dir, skills_found) =
        fetch_repo_scanned_in_session(url, false, session)?;
    let parsed =
        crate::source_resolver::Source::parse(url).map_err(|e| format!("Invalid source: {e}"))?;
    let existing_lock = lockfile::Lockfile::load(&lockfile::lockfile_path())
        .map_err(|error| format!("Failed to load Skill lockfile: {error}"))?;

    let mut targets = Vec::new();
    let mut fallback_names = Vec::new();
    let mut missing_names = Vec::new();

    for name in names {
        // First try to find a match in the scanned repo
        let target = find_target_skill(&skills_found, Some(name), name);
        if let Some(skill) = target {
            if local_skill_blocks_repo_install(&skill.id) {
                warn!(
                    target: "install_skills_batch",
                    skill_id = %skill.id,
                    "repo-cache skill would collide with existing local skill, skipping"
                );
                continue;
            }
            if skills_dir.join(&skill.id).symlink_metadata().is_ok() {
                let same_source = existing_lock.skills.iter().any(|entry| {
                    entry.name == skill.id
                        && crate::source_resolver::same_remote_url(&entry.git_url, &repo_url)
                });
                if same_source {
                    continue;
                }
                return Err(format!(
                    "Skill '{}' already exists from a different source; remove it or choose another skill",
                    skill.id
                ));
            }
            targets.push(repo_scanner::SkillInstallTarget {
                id: skill.id.clone(),
                folder_path: skill.folder_path.clone(),
            });
        } else {
            missing_names.push(name.clone());
        }
    }

    if !missing_names.is_empty() {
        return Err(format!(
            "Requested skills not found in scanned repository: {}",
            missing_names.join(", ")
        ));
    }

    let mut installed_skills = Vec::new();

    if !targets.is_empty() {
        match repo_scanner::install_from_repo_at(
            &repo_dir,
            &repo_url,
            parsed.git_ref.as_deref(),
            &targets,
        ) {
            Ok(installed) => {
                installed_skills.extend(finalize_repo_cache_installs(
                    &skills_dir,
                    &repo_url,
                    &installed,
                    &targets,
                )?);
                installed_skill::invalidate_cache();
            }
            Err(e) => {
                warn!(target: "install_skills_batch", error = %e, "batch repo install failed");
                // Fallback: all targets must be installed via direct fallback
                for t in targets {
                    fallback_names.push(t.id);
                }
            }
        }
    }

    // Process fallbacks one by one
    let mut fallback_installed = Vec::new();
    for name in fallback_names {
        match install_skill_in_session(url.to_string(), Some(name), session) {
            Ok(skill) => {
                fallback_installed.push(skill.name.clone());
                installed_skills.push(skill);
            }
            Err(error) => {
                let mut rollback_errors = Vec::new();
                for installed_name in fallback_installed.iter().rev() {
                    if let Err(rollback_error) = uninstall_skill(installed_name) {
                        rollback_errors.push(format!("{installed_name}: {rollback_error}"));
                    }
                }
                let rollback = if rollback_errors.is_empty() {
                    String::new()
                } else {
                    format!("; rollback also failed: {}", rollback_errors.join(", "))
                };
                return Err(format!("Fallback install failed: {error}{rollback}"));
            }
        }
    }

    Ok(installed_skills)
}

/// Install all skills from a repo that contains a skillpack.toml manifest.
/// Returns the list of installed skill names.
pub fn install_skill_pack(url: String) -> Result<Vec<String>, String> {
    install_skill_pack_in_session(url, &crate::git::transport::GitOperationSession::public())
}

pub fn install_skill_pack_in_session(
    url: String,
    session: &crate::git::transport::GitOperationSession,
) -> Result<Vec<String>, String> {
    let (_repo_url, source, repo_dir, _) = fetch_repo_scanned_in_session(&url, false, session)?;

    // Detect pack manifest
    crate::skill_pack::detect_pack(&repo_dir)
        .map_err(|e| format!("Failed to detect skill pack: {}", e))?
        .ok_or_else(|| "No skillpack.toml found in repository".to_string())?;

    // Install via skill_pack module
    crate::skill_pack::install_pack(&repo_dir, &source, &url)
        .map_err(|e| format!("Pack install failed: {}", e))
}

pub fn uninstall_skill(name: &str) -> Result<(), String> {
    crate::content::validate_skill_name(name)
        .map_err(|error| format!("Invalid Skill name: {error}"))?;
    if local_skill::is_local_skill(name) {
        local_skill::delete(name).map_err(|e| e.to_string())?;
        installed_skill::invalidate_cache();
        return Ok(());
    }

    let skills_dir = paths::hub_skills_dir();
    let path = skills_dir.join(name);
    let _lock = lockfile::get_mutex()
        .lock()
        .map_err(|_| "Lockfile mutex poisoned".to_string())?;
    let lock_path = lockfile::lockfile_path();
    let mut lf = lockfile::Lockfile::load(&lock_path)
        .map_err(|e| format!("Failed to load lockfile '{}': {}", lock_path.display(), e))?;
    let staging = skills_dir.join(format!(".skillstar-remove-{name}-{}", std::process::id()));
    if staging.symlink_metadata().is_ok() {
        return Err(format!(
            "A previous removal staging path still exists: '{}'",
            staging.display()
        ));
    }
    let moved = path.symlink_metadata().is_ok();
    if moved {
        std::fs::rename(&path, &staging)
            .map_err(|error| format!("Failed to stage Skill '{name}' for removal: {error}"))?;
    }
    lf.remove(name);
    if let Err(error) = lf.save(&lock_path) {
        let restore_error = moved
            .then(|| std::fs::rename(&staging, &path).err())
            .flatten()
            .map(|restore| format!("; restoring the Skill also failed: {restore}"))
            .unwrap_or_default();
        return Err(format!(
            "Failed to save lockfile '{}': {error}{restore_error}",
            lock_path.display(),
        ));
    }
    drop(_lock);

    let mut cleanup_failures = Vec::new();
    if moved && let Err(error) = fs_ops::remove_link_or_copy(&staging) {
        cleanup_failures.push(format!(
            "remove staged hub content '{}': {error}",
            staging.display()
        ));
        warn!(
            target: "uninstall_skill",
            path = %staging.display(),
            error = %error,
            "Skill was removed but its staging path could not be cleaned"
        );
    }

    if let Err(error) = deployment::remove_skill_from_all_agents(name) {
        cleanup_failures.push(format!("remove Agent deployments: {error:#}"));
    }
    if let Err(error) = projects::remove_skill_from_all_projects(name) {
        cleanup_failures.push(format!("remove Project deployments: {error:#}"));
    }
    installed_skill::invalidate_cache();

    if cleanup_failures.is_empty() {
        Ok(())
    } else {
        Err(format!(
            "Skill '{name}' was removed from the hub, but cleanup is incomplete: {}",
            cleanup_failures.join(", ")
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::{
        RepoInstallProvenance, derive_name_hint, find_target_skill, write_repo_install_provenance,
    };
    use crate::frontmatter::split_front_matter;
    use crate::repo_scanner::DiscoveredSkill;
    use serde_yaml::Value;

    fn discovered(id: &str) -> DiscoveredSkill {
        DiscoveredSkill {
            id: id.to_string(),
            folder_path: format!("skills/{id}"),
            description: String::new(),
            already_installed: false,
        }
    }

    #[test]
    fn derive_name_hint_prefers_explicit_name() {
        let hint = derive_name_hint(
            "https://github.com/example/skills.git",
            Some("explicit-name"),
        );
        assert_eq!(hint, "explicit-name");
    }

    #[test]
    fn derive_name_hint_falls_back_to_repo_tail() {
        let hint = derive_name_hint("https://github.com/example/awesome-skill.git", None);
        assert_eq!(hint, "awesome-skill");
    }

    #[test]
    fn find_target_skill_prefers_requested_name_case_insensitive() {
        let skills = vec![discovered("frontend-ui"), discovered("security-review")];
        let target = find_target_skill(&skills, Some("FRONTEND-UI"), "unused-name-hint");
        assert_eq!(target.map(|skill| skill.id.as_str()), Some("frontend-ui"));
    }

    #[test]
    fn find_target_skill_uses_single_skill_fallback() {
        let skills = vec![discovered("only-one")];
        let target = find_target_skill(&skills, None, "no-match-hint");
        assert_eq!(target.map(|skill| skill.id.as_str()), Some("only-one"));
    }

    fn write_skill_md(dir: &std::path::Path, content: &str) {
        std::fs::write(dir.join("SKILL.md"), content).unwrap();
    }

    fn read_skill_md(dir: &std::path::Path) -> String {
        std::fs::read_to_string(dir.join("SKILL.md")).unwrap()
    }

    #[test]
    fn provenance_writer_adds_frontmatter_when_absent() {
        let dir = tempfile::tempdir().unwrap();
        write_skill_md(dir.path(), "# Skill\n\nBody\n");

        write_repo_install_provenance(
            dir.path(),
            RepoInstallProvenance {
                git_url: "https://github.com/example/skill-repo",
                source_folder: None,
            },
        )
        .unwrap();

        let rendered = read_skill_md(dir.path());
        let split = split_front_matter(&rendered);
        assert_eq!(split.body, "# Skill\n\nBody\n");
        assert_eq!(
            split
                .data
                .get("provenance")
                .and_then(Value::as_mapping)
                .and_then(|mapping| mapping.get(Value::String("repository_url".to_string())))
                .and_then(Value::as_str),
            Some("https://github.com/example/skill-repo")
        );
    }

    #[test]
    fn provenance_writer_preserves_existing_frontmatter_keys_and_body() {
        let dir = tempfile::tempdir().unwrap();
        write_skill_md(
            dir.path(),
            "---\ntitle: Existing\ntags:\n  - rust\n---\n# Heading\n\nOriginal body\n",
        );

        write_repo_install_provenance(
            dir.path(),
            RepoInstallProvenance {
                git_url: "https://github.com/example/skill-repo",
                source_folder: Some("skills/rust"),
            },
        )
        .unwrap();

        let rendered = read_skill_md(dir.path());
        let split = split_front_matter(&rendered);

        assert_eq!(split.body, "# Heading\n\nOriginal body\n");
        assert_eq!(
            split.data.get("title").and_then(Value::as_str),
            Some("Existing")
        );
        assert_eq!(
            split
                .data
                .get("tags")
                .and_then(Value::as_sequence)
                .and_then(|tags| tags.first())
                .and_then(Value::as_str),
            Some("rust")
        );

        let provenance = split
            .data
            .get("provenance")
            .and_then(Value::as_mapping)
            .unwrap();
        assert_eq!(
            provenance
                .get(Value::String("repository_url".to_string()))
                .and_then(Value::as_str),
            Some("https://github.com/example/skill-repo")
        );
        assert_eq!(
            provenance
                .get(Value::String("source_folder".to_string()))
                .and_then(Value::as_str),
            Some("skills/rust")
        );
    }

    #[test]
    fn provenance_writer_merges_existing_provenance_mapping() {
        let dir = tempfile::tempdir().unwrap();
        write_skill_md(
            dir.path(),
            "---\nprovenance:\n  imported_by: skillstar\n  repository_url: stale\n---\n# Heading\n",
        );

        write_repo_install_provenance(
            dir.path(),
            RepoInstallProvenance {
                git_url: "https://github.com/example/skill-repo",
                source_folder: Some("nested/skill"),
            },
        )
        .unwrap();

        let rendered = read_skill_md(dir.path());
        let split = split_front_matter(&rendered);
        let provenance = split
            .data
            .get("provenance")
            .and_then(Value::as_mapping)
            .unwrap();

        assert_eq!(
            provenance
                .get(Value::String("imported_by".to_string()))
                .and_then(Value::as_str),
            Some("skillstar")
        );
        assert_eq!(
            provenance
                .get(Value::String("repository_url".to_string()))
                .and_then(Value::as_str),
            Some("https://github.com/example/skill-repo")
        );
        assert_eq!(
            provenance
                .get(Value::String("source_folder".to_string()))
                .and_then(Value::as_str),
            Some("nested/skill")
        );
    }
}
