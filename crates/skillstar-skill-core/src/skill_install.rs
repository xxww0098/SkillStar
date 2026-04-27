use crate::{installed_skill, local_skill, lockfile, project_manifest, repo_scanner};
use skillstar_core_types::{Skill, SkillCategory, SkillType, extract_github_source_from_url, extract_skill_description};
use skillstar_git::ops as git_ops;
use skillstar_infra::{fs_ops, paths};
use skillstar_projects::sync;
use markdown_translator::parser::frontmatter::{render_with_front_matter, split_front_matter};
use serde_yaml::{Mapping, Value};
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
    let (repo_url, source) =
        repo_scanner::normalize_repo_url(url).map_err(|e| format!("Invalid URL: {}", e))?;
    let repo_dir = repo_scanner::clone_or_fetch_repo(&repo_url, &source)
        .map_err(|e| format!("Failed to fetch repo: {}", e))?;
    let skills_found = repo_scanner::scan_skills_in_repo(&repo_dir, &repo_url, full_depth);
    Ok((repo_url, source, repo_dir, skills_found))
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

fn write_repo_install_provenance(
    skill_dir: &Path,
    provenance: RepoInstallProvenance<'_>,
) -> Result<(), String> {
    let skill_md_path = skill_markdown_path(skill_dir);
    let existing = std::fs::read_to_string(&skill_md_path)
        .map_err(|e| format!("Failed to read '{}': {}", skill_md_path.display(), e))?;
    let split = split_front_matter(&existing);
    let mut front_matter = split.data;
    let existing_provenance = front_matter.remove("provenance");
    front_matter.insert(
        "provenance".to_string(),
        merge_provenance_value(existing_provenance, provenance),
    );

    let rendered = render_with_front_matter(Some(&front_matter), &split.body);
    std::fs::write(&skill_md_path, rendered)
        .map_err(|e| format!("Failed to write '{}': {}", skill_md_path.display(), e))
}

fn try_install_from_repo_cache(
    url: &str,
    requested_name: Option<&str>,
    name_hint: &str,
    skills_dir: &Path,
) -> Result<Option<Skill>, String> {
    let Ok((repo_url, source, _, skills_found)) = fetch_repo_scanned(url, false) else {
        return Ok(None);
    };
    let target = find_target_skill(&skills_found, requested_name, name_hint);

    // Guard against overwriting a local skill whose name matches the repo skill
    if let Some(skill) = &target {
        if local_skill_blocks_repo_install(&skill.id) {
            warn!(
                target: "install_skill",
                skill_id = %skill.id,
                "repo-cache skill would collide with existing local skill, skipping"
            );
            return Ok(None);
        }
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

    let targets = vec![repo_scanner::SkillInstallTarget {
        id: skill.id.clone(),
        folder_path: skill.folder_path.clone(),
    }];

    match repo_scanner::install_from_repo(&source, &repo_url, &targets) {
        Ok(installed) if !installed.is_empty() => {
            let installed_name = installed[0].clone();
            let dest = skills_dir.join(&installed_name);
            write_repo_install_provenance(
                &dest,
                RepoInstallProvenance {
                    git_url: &repo_url,
                    source_folder: Some(&skill.folder_path),
                },
            )?;
            let description = extract_skill_description(&dest);
            installed_skill::invalidate_cache();
            let tree_hash = compute_tree_hash_for(skills_dir, &installed_name);
            Ok(Some(new_skill_from_install(
                installed_name,
                description,
                repo_url,
                tree_hash,
            )))
        }
        Ok(_) => Ok(None),
        Err(err) => {
            warn!(target: "install_skill", error = %err, "repo-cache install failed, falling back");
            Ok(None)
        }
    }
}

pub fn install_skill(url: String, name: Option<String>) -> Result<Skill, String> {
    let skills_dir = paths::hub_skills_dir();
    let name_hint = derive_name_hint(&url, name.as_deref());

    if skills_dir.join(&name_hint).exists() {
        return Err(format!("Skill '{}' is already installed", name_hint));
    }
    if local_skill_blocks_repo_install(&name_hint) {
        return Err(format!(
            "Skill '{}' already exists as a local skill",
            name_hint
        ));
    }

    if let Some(skill) =
        try_install_from_repo_cache(&url, name.as_deref(), &name_hint, &skills_dir)?
    {
        return Ok(skill);
    }

    let dest = skills_dir.join(&name_hint);
    if dest.exists() {
        return Err(format!("Skill '{}' is already installed", name_hint));
    }

    git_ops::clone_repo(&url, &dest).map_err(|e| e.to_string())?;
    let tree_hash = git_ops::compute_tree_hash(&dest).map_err(|e| e.to_string())?;

    let _lock = lockfile::get_mutex()
        .lock()
        .map_err(|_| "Lockfile mutex poisoned".to_string())?;
    let lock_path = lockfile::lockfile_path();
    let mut lockfile = lockfile::Lockfile::load(&lock_path)
        .map_err(|e| format!("Failed to load lockfile '{}': {}", lock_path.display(), e))?;
    lockfile.upsert(lockfile::LockEntry {
        name: name_hint.clone(),
        git_url: url.clone(),
        tree_hash: tree_hash.clone(),
        installed_at: chrono::Utc::now().to_rfc3339(),
        source_folder: None,
    });
    lockfile
        .save(&lock_path)
        .map_err(|e| format!("Failed to save lockfile '{}': {}", lock_path.display(), e))?;
    installed_skill::invalidate_cache();

    let description = extract_skill_description(&dest);
    Ok(new_skill_from_install(
        name_hint,
        description,
        url,
        Some(tree_hash),
    ))
}

/// Install multiple skills from the same repository URL in a single batch.
/// This prevents git clone/fetch overlap and lockfile serialization issues when
/// multiple skills share the same repository.
pub fn install_skills_batch(url: &str, names: &[String]) -> Result<Vec<Skill>, String> {
    if names.is_empty() {
        return Ok(Vec::new());
    }

    let skills_dir = paths::hub_skills_dir();
    let (repo_url, source, _, skills_found) = fetch_repo_scanned(url, false)?;

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
            if skills_dir.join(&skill.id).exists() {
                // Already installed, skip
                continue;
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
        match repo_scanner::install_from_repo(&source, &repo_url, &targets) {
            Ok(installed) => {
                installed_skill::invalidate_cache();
                for installed_name in installed {
                    let dest = skills_dir.join(&installed_name);
                    let source_folder = targets
                        .iter()
                        .find(|target| target.id == installed_name)
                        .map(|target| target.folder_path.as_str());
                    write_repo_install_provenance(
                        &dest,
                        RepoInstallProvenance {
                            git_url: &repo_url,
                            source_folder,
                        },
                    )?;
                    let description = extract_skill_description(&dest);
                    let tree_hash = compute_tree_hash_for(&skills_dir, &installed_name);
                    installed_skills.push(new_skill_from_install(
                        installed_name,
                        description,
                        repo_url.clone(),
                        tree_hash,
                    ));
                }
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
    for name in fallback_names {
        match install_skill(url.to_string(), Some(name)) {
            Ok(skill) => installed_skills.push(skill),
            Err(e) => warn!(target: "install_skills_batch", error = %e, "fallback install failed"),
        }
    }

    Ok(installed_skills)
}

/// Install all skills from a repo that contains a skillpack.toml manifest.
/// Returns the list of installed skill names.
pub fn install_skill_pack(url: String) -> Result<Vec<String>, String> {
    let (_repo_url, source, repo_dir, _) = fetch_repo_scanned(&url, false)?;

    // Detect pack manifest
    crate::skill_pack::detect_pack(&repo_dir)
        .map_err(|e| format!("Failed to detect skill pack: {}", e))?
        .ok_or_else(|| "No skillpack.toml found in repository".to_string())?;

    // Install via skill_pack module
    crate::skill_pack::install_pack(&repo_dir, &source, &url)
        .map_err(|e| format!("Pack install failed: {}", e))
}

pub fn uninstall_skill(name: &str) -> Result<(), String> {
    if local_skill::is_local_skill(name) {
        local_skill::delete(name).map_err(|e| e.to_string())?;
        installed_skill::invalidate_cache();
        skillstar_security_scan::invalidate_skill_cache(name);
        return Ok(());
    }

    let _ = sync::remove_skill_from_all_agents(name);

    let skills_dir = paths::hub_skills_dir();
    let path = skills_dir.join(name);

    if fs_ops::is_link(&path) {
        fs_ops::remove_symlink(&path).map_err(|e| e.to_string())?;
    } else if path.exists() {
        fs_ops::remove_dir_all_retry(&path).map_err(|e| e.to_string())?;
    }

    let _lock = lockfile::get_mutex()
        .lock()
        .map_err(|_| "Lockfile mutex poisoned".to_string())?;
    let lock_path = lockfile::lockfile_path();
    let mut lf = lockfile::Lockfile::load(&lock_path)
        .map_err(|e| format!("Failed to load lockfile '{}': {}", lock_path.display(), e))?;
    lf.remove(name);
    lf.save(&lock_path)
        .map_err(|e| format!("Failed to save lockfile '{}': {}", lock_path.display(), e))?;

    let _ = project_manifest::remove_skill_from_all_projects(name);
    installed_skill::invalidate_cache();
    skillstar_security_scan::invalidate_skill_cache(name);

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{
        RepoInstallProvenance, derive_name_hint, find_target_skill, write_repo_install_provenance,
    };
    use crate::repo_scanner::DiscoveredSkill;
    use markdown_translator::parser::frontmatter::split_front_matter;
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
        assert!(split.line_count > 0);
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
