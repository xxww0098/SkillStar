//! Adopt skill(s) from a local folder into the user's hub as local-authored skills.
//!
//! Mirrors the CLI `skillstar install <local-dir>` flow so GUI users get the
//! same capability without shelling out.

use serde::Serialize;
use std::path::PathBuf;

use crate::core::{installed_skill, local_skill};
use skillstar_core::infra::error::AppError;

#[derive(Debug, Clone, Serialize)]
pub struct AdoptedSkill {
    pub name: String,
    pub folder_path: String,
    pub description: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct AdoptLocalFolderResult {
    pub adopted: Vec<AdoptedSkill>,
    pub skipped: Vec<SkippedLocalSkill>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SkippedLocalSkill {
    pub name: String,
    pub reason: String,
}

#[tauri::command]
pub async fn adopt_local_folder(
    folder_path: String,
    names: Option<Vec<String>>,
) -> Result<AdoptLocalFolderResult, AppError> {
    tokio::task::spawn_blocking(move || adopt_local_folder_sync(folder_path, names))
        .await
        .map_err(|e| AppError::Other(format!("adopt folder task panicked: {e}")))?
}

fn adopt_local_folder_sync(
    folder_path: String,
    names: Option<Vec<String>>,
) -> Result<AdoptLocalFolderResult, AppError> {
    let path = PathBuf::from(&folder_path);
    if !path.is_dir() {
        return Err(AppError::Other(format!(
            "Not a directory: {}",
            path.display()
        )));
    }
    let canonical = std::fs::canonicalize(&path)
        .map_err(|e| AppError::Other(format!("Failed to resolve {}: {}", path.display(), e)))?;

    let skills = skillstar_skills::discover_skills(&canonical, false);
    if skills.is_empty() {
        return Err(AppError::Other(format!(
            "No SKILL.md found in {} (root or priority dirs)",
            canonical.display()
        )));
    }

    let requested: Option<Vec<String>> = names.map(|ns| {
        ns.into_iter()
            .map(|s| s.trim().to_lowercase())
            .filter(|s| !s.is_empty())
            .collect()
    });

    let selected: Vec<&skillstar_skills::DiscoveredSkill> = match &requested {
        Some(want) if !want.is_empty() => skills
            .iter()
            .filter(|s| want.iter().any(|w| w == &s.id.to_lowercase()))
            .collect(),
        _ => skills.iter().collect(),
    };

    if selected.is_empty() {
        return Err(AppError::Other(
            "None of the requested skills were found in the folder".to_string(),
        ));
    }

    let mut adopted: Vec<AdoptedSkill> = Vec::new();
    let mut skipped: Vec<SkippedLocalSkill> = Vec::new();

    for skill in selected {
        let source_dir = if skill.folder_path.is_empty() {
            canonical.clone()
        } else {
            canonical.join(&skill.folder_path)
        };
        let skill_md = source_dir.join("SKILL.md");
        let content = match std::fs::read_to_string(&skill_md) {
            Ok(c) => c,
            Err(e) => {
                skipped.push(SkippedLocalSkill {
                    name: skill.id.clone(),
                    reason: format!("read_failed: {}", e),
                });
                continue;
            }
        };
        match local_skill::create(&skill.id, Some(&content)) {
            Ok(created) => {
                adopted.push(AdoptedSkill {
                    name: created.name,
                    folder_path: skill.folder_path.clone(),
                    description: created.description,
                });
            }
            Err(err) => {
                skipped.push(SkippedLocalSkill {
                    name: skill.id.clone(),
                    reason: err.to_string(),
                });
            }
        }
    }

    if !adopted.is_empty() {
        installed_skill::invalidate_cache();
    }

    Ok(AdoptLocalFolderResult { adopted, skipped })
}
