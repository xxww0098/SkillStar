//! Unified share-code install pipeline.
//!
//! Both `ImportModal` (My Skills) and `ImportShareCodeModal` (Decks) used to
//! re-implement the "for each share entry: embedded vs repo vs skip" loop in
//! TypeScript. This command centralizes that logic in Rust so the two UIs, the
//! CLI, and future automations can share one install pipeline.

use serde::{Deserialize, Serialize};
use tracing::{debug, warn};

use crate::core::{installed_skill, local_skill, repo_scanner, skill_install};
use skillstar_core::infra::error::AppError;

/// A single skill entry in a share code payload. Keys match the TypeScript
/// `ShareCodeData` shape (abbreviated for density).
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ShareCodeSkill {
    /// Skill name (human-readable id).
    pub n: String,
    /// Git URL when the skill is git-backed. May be empty when `c` is set.
    #[serde(default)]
    pub u: String,
    /// Inline SKILL.md content (Base64 UTF-8). Optional.
    #[serde(default)]
    pub c: Option<String>,
    /// `true` if the repo requires auth (private).
    #[serde(default)]
    pub p: Option<bool>,
}

/// Per-skill install outcome returned to the frontend.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum ShareSkillOutcome {
    /// Already installed in the hub before this run.
    Existing { name: String },
    /// Freshly installed from a repo.
    Installed { name: String },
    /// Installed by decoding the embedded SKILL.md content.
    Embedded { name: String },
    /// Skipped because neither repo nor embedded content resolved.
    Skipped { name: String, reason: String },
}

#[derive(Debug, Clone, Serialize)]
pub struct ShareCodeInstallSummary {
    pub requested_count: usize,
    pub installed_names: Vec<String>,
    pub existing_names: Vec<String>,
    pub embedded_names: Vec<String>,
    pub skipped: Vec<SkippedSkill>,
    pub outcomes: Vec<ShareSkillOutcome>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SkippedSkill {
    pub name: String,
    pub reason: String,
}

fn normalize(name: &str) -> String {
    name.trim().to_lowercase()
}

fn decode_embedded(base64: &str) -> Result<String, String> {
    use base64::Engine as _;
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(base64.trim())
        .map_err(|e| format!("Base64 decode failed: {}", e))?;
    String::from_utf8(bytes).map_err(|e| format!("Embedded content is not valid UTF-8: {}", e))
}

fn is_installed_in_hub(skill_name: &str) -> bool {
    let hub_dir = skillstar_core::infra::paths::hub_skills_dir();
    hub_dir.join(skill_name).symlink_metadata().is_ok()
}

#[tauri::command]
pub async fn install_from_share_code(
    skills: Vec<ShareCodeSkill>,
) -> Result<ShareCodeInstallSummary, AppError> {
    tokio::task::spawn_blocking(move || install_share_code_sync(skills))
        .await
        .map_err(|e| AppError::Other(format!("share-code install task panicked: {e}")))?
}

fn install_share_code_sync(
    skills: Vec<ShareCodeSkill>,
) -> Result<ShareCodeInstallSummary, AppError> {
    let requested_count = skills.len();
    let mut outcomes: Vec<ShareSkillOutcome> = Vec::with_capacity(requested_count);
    let mut installed_names: Vec<String> = Vec::new();
    let mut existing_names: Vec<String> = Vec::new();
    let mut embedded_names: Vec<String> = Vec::new();
    let mut skipped: Vec<SkippedSkill> = Vec::new();

    let mut seen_installed: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut seen_existing: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut seen_embedded: std::collections::HashSet<String> = std::collections::HashSet::new();

    for entry in skills {
        let name = entry.n.trim().to_string();
        if name.is_empty() {
            continue;
        }
        let key = normalize(&name);

        // Already installed → record and continue.
        if is_installed_in_hub(&name) {
            if seen_existing.insert(key.clone()) {
                existing_names.push(name.clone());
                outcomes.push(ShareSkillOutcome::Existing { name: name.clone() });
            }
            continue;
        }

        let install_embedded = |outcomes: &mut Vec<ShareSkillOutcome>,
                                embedded_names: &mut Vec<String>,
                                seen_embedded: &mut std::collections::HashSet<String>|
         -> Option<String> {
            let encoded = entry.c.as_deref()?;
            match decode_embedded(encoded) {
                Ok(content) => match local_skill::create(&name, Some(&content)) {
                    Ok(_) => {
                        installed_skill::invalidate_cache();
                        if seen_embedded.insert(key.clone()) {
                            embedded_names.push(name.clone());
                            outcomes.push(ShareSkillOutcome::Embedded { name: name.clone() });
                        }
                        Some(name.clone())
                    }
                    Err(err) => {
                        warn!(
                            target: "share_install",
                            skill = %name,
                            error = %err,
                            "embedded create failed"
                        );
                        None
                    }
                },
                Err(err) => {
                    warn!(
                        target: "share_install",
                        skill = %name,
                        error = %err,
                        "embedded decode failed"
                    );
                    None
                }
            }
        };

        // Git-backed entry → use the scan+install path.
        if !entry.u.trim().is_empty() {
            match skill_install::install_skills_batch(&entry.u, std::slice::from_ref(&name)) {
                Ok(result) if !result.is_empty() => {
                    installed_skill::invalidate_cache();
                    for skill in result {
                        if seen_installed.insert(normalize(&skill.name)) {
                            installed_names.push(skill.name.clone());
                            outcomes.push(ShareSkillOutcome::Installed { name: skill.name });
                        }
                    }
                    continue;
                }
                Ok(_) => {
                    debug!(target: "share_install", skill = %name, "repo scan produced no matches, trying embedded");
                }
                Err(err) => {
                    warn!(target: "share_install", skill = %name, error = %err, "repo install failed, trying embedded");
                }
            }

            if install_embedded(&mut outcomes, &mut embedded_names, &mut seen_embedded).is_some() {
                continue;
            }

            skipped.push(SkippedSkill {
                name: name.clone(),
                reason: "install_failed".to_string(),
            });
            outcomes.push(ShareSkillOutcome::Skipped {
                name: name.clone(),
                reason: "install_failed".to_string(),
            });
            continue;
        }

        // No URL → must have embedded content to install.
        if entry.c.is_some() {
            if install_embedded(&mut outcomes, &mut embedded_names, &mut seen_embedded).is_some() {
                continue;
            }
            skipped.push(SkippedSkill {
                name: name.clone(),
                reason: "embedded_failed".to_string(),
            });
            outcomes.push(ShareSkillOutcome::Skipped {
                name: name.clone(),
                reason: "embedded_failed".to_string(),
            });
            continue;
        }

        skipped.push(SkippedSkill {
            name: name.clone(),
            reason: "no_source".to_string(),
        });
        outcomes.push(ShareSkillOutcome::Skipped {
            name,
            reason: "no_source".to_string(),
        });
    }

    // Silence unused warning for the repo_scanner import used by skill_install internals.
    let _: Option<repo_scanner::ScanResult> = None;

    Ok(ShareCodeInstallSummary {
        requested_count,
        installed_names,
        existing_names,
        embedded_names,
        skipped,
        outcomes,
    })
}
