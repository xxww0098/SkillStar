//! Private CSP-strict HTML tutorial artifacts keyed by skill identity.

mod store;
mod validator;

use serde::{Deserialize, Serialize};
use skillstar_core::infra::error::AppError;

use crate::identity::{ResolvedSkill, SkillIdentity, SkillRevision};

pub use validator::{ValidatedTutorialHtml, validate_html};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TutorialState {
    Missing,
    Fresh,
    Stale,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TutorialStaleReason {
    ContentChanged,
    GeneratorChanged,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GeneratorFingerprint {
    pub prompt_version: String,
    pub schema_version: String,
}

impl GeneratorFingerprint {
    pub fn new(prompt_version: impl Into<String>, schema_version: impl Into<String>) -> Self {
        Self {
            prompt_version: prompt_version.into(),
            schema_version: schema_version.into(),
        }
    }

    fn matches(&self, prompt_version: &str, schema_version: &str) -> bool {
        self.prompt_version == prompt_version && self.schema_version == schema_version
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PrivateTutorialMetadata {
    pub bound: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub identity: Option<SkillIdentity>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub generated_revision: Option<SkillRevision>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub skill_name: Option<String>,
    pub content_hash: String,
    pub prompt_version: String,
    pub schema_version: String,
    pub tutorial_style: String,
    pub agent_label: String,
    pub generated_at: String,
    pub file_count: usize,
    pub total_bytes: u64,
    #[serde(default)]
    pub source_files: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PrivateTutorial {
    pub state: TutorialState,
    pub bound: bool,
    pub html: Option<String>,
    pub metadata: Option<PrivateTutorialMetadata>,
    pub stale_reason: Option<TutorialStaleReason>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub stale_reasons: Vec<TutorialStaleReason>,
}

pub fn load_private_tutorial(
    resolved: &ResolvedSkill,
    inventory: &[String],
    total_bytes: u64,
    generator: &GeneratorFingerprint,
) -> Result<PrivateTutorial, AppError> {
    store::load(resolved, inventory, total_bytes, generator)
}

pub fn commit_private_tutorial(
    resolved: &ResolvedSkill,
    inventory: &[String],
    total_bytes: u64,
    generator: &GeneratorFingerprint,
    tutorial_style: &str,
    agent_label: &str,
    raw_html: &str,
) -> Result<PrivateTutorial, AppError> {
    if inventory.is_empty() {
        return Err(AppError::Other(
            "Private tutorial inventory is empty".to_string(),
        ));
    }
    let validated = validate_html(raw_html, inventory)?;
    store::commit(
        resolved,
        inventory,
        total_bytes,
        generator,
        tutorial_style,
        agent_label,
        validated,
    )
}

#[cfg(test)]
mod tests;
