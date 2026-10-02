//! Named profiles. The file and the apply loop live in `skillstar-gateway`.
//! This module forwards that API and does not write agent files.

use serde::{Deserialize, Serialize};
use skillstar_gateway::{
    ApplyProfileError, ProfileAgent, SaveProfileError, apply_profile, profile_names, save_profile,
};

/// One agent id and the model ref to store for it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProfileAgentDto {
    pub id: String,
    pub model_ref: String,
}

/// Writers that ran, and agent ids the writer does not implement.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProfileApplyDto {
    pub applied: Vec<String>,
    pub skipped: Vec<String>,
}

/// Why a profile was not saved. The text is what the page shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SaveProfileControlError {
    Name,
    Store,
}

impl std::fmt::Display for SaveProfileControlError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Name => "profile_name",
            Self::Store => "profile_store",
        })
    }
}

/// Why a profile was not applied.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ApplyProfileControlError {
    Missing,
    Store,
    Write,
}

impl std::fmt::Display for ApplyProfileControlError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Missing => "profile_missing",
            Self::Store => "profile_store",
            Self::Write => "profile_write",
        })
    }
}

/// Names already stored. A missing file is empty.
pub fn load_profile_names() -> Vec<String> {
    profile_names()
}

/// Replace one profile's agent list.
pub fn save_profile_agents(
    name: &str,
    agents: &[ProfileAgentDto],
) -> Result<(), SaveProfileControlError> {
    let agents: Vec<ProfileAgent> = agents
        .iter()
        .map(|agent| ProfileAgent {
            id: agent.id.clone(),
            model_ref: agent.model_ref.clone(),
        })
        .collect();
    save_profile(name, &agents).map_err(|error| match error {
        SaveProfileError::Name => SaveProfileControlError::Name,
        SaveProfileError::Store => SaveProfileControlError::Store,
    })
}

/// Apply one profile through the existing writer.
pub fn apply_saved_profile(name: &str) -> Result<ProfileApplyDto, ApplyProfileControlError> {
    apply_profile(name)
        .map(|result| ProfileApplyDto {
            applied: result.applied,
            skipped: result.skipped,
        })
        .map_err(|error| match error {
            ApplyProfileError::Missing => ApplyProfileControlError::Missing,
            ApplyProfileError::Store => ApplyProfileControlError::Store,
            ApplyProfileError::Write => ApplyProfileControlError::Write,
        })
}
