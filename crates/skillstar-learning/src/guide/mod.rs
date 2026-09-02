//! Guides, local progress and Block JSON Drafts.
//!
//! Product reading does not depend on `skill.installed`. Progress is isolated
//! by Guide revision. Draft conversion is explicit and fail-closed.

mod blocks;
mod convert;
mod seed;
mod store;

#[cfg(test)]
mod tests;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use skillstar_core::infra::error::AppError;

use crate::identity::{SkillIdentity, SkillRevision, SkillRevisionKey};
use crate::tutorial::PrivateTutorial;

pub use blocks::{CalloutTone, GuideBlock};
pub use convert::ConversionPreview;
pub use seed::{SEED_DISPLAY_NAME, SEED_GUIDE_ID, frontend_design_first_success};

const GUIDE_REVISION_DOMAIN: &[u8] = b"skillstar.guide-revision.v1\0";
const DRAFT_REVISION_DOMAIN: &[u8] = b"skillstar.guide-draft.v1\0";

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct GuideId(String);

impl GuideId {
    pub fn new(id: impl Into<String>) -> Result<Self, AppError> {
        let id = id.into();
        if id.is_empty() || id.contains('\0') || id.contains('/') || id.contains('\\') {
            return Err(AppError::Other(
                "Guide id is empty or contains a path/NUL character".to_string(),
            ));
        }
        Ok(Self(id))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn storage_segment(&self) -> String {
        self.0.replace(':', "-")
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct GuideRevisionKey(String);

impl GuideRevisionKey {
    pub(crate) fn from_digest(digest: &[u8]) -> Self {
        Self(format!("gkr:v1:{}", hex_digest(digest)))
    }

    pub fn from_wire(value: impl Into<String>) -> Result<Self, AppError> {
        let value = value.into();
        if !value.starts_with("gkr:v1:") || value.len() != 71 {
            return Err(AppError::Other(
                "Guide revision key is malformed".to_string(),
            ));
        }
        if !value[7..].chars().all(|ch| ch.is_ascii_hexdigit()) {
            return Err(AppError::Other(
                "Guide revision key is malformed".to_string(),
            ));
        }
        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn storage_segment(&self) -> String {
        self.0.replace(':', "-")
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum GuideStepKind {
    Reading,
    Practice,
    Verify,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GuideStep {
    pub id: String,
    pub kind: GuideStepKind,
    pub title: String,
    pub requires_skill: bool,
    pub blocks: Vec<GuideBlock>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Guide {
    pub id: GuideId,
    pub title: String,
    pub locale: String,
    pub summary: String,
    pub schema_version: String,
    pub skill_identity: SkillIdentity,
    pub skill_revision: SkillRevision,
    pub revision_key: GuideRevisionKey,
    pub steps: Vec<GuideStep>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GuideSummary {
    pub id: GuideId,
    pub title: String,
    pub locale: String,
    pub summary: String,
    pub display_name: String,
    pub skill_identity: SkillIdentity,
    pub skill_revision: SkillRevision,
    pub revision_key: GuideRevisionKey,
    pub step_count: usize,
    pub first_step_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LearningProgress {
    pub guide_id: GuideId,
    pub guide_revision_key: GuideRevisionKey,
    pub current_step_id: String,
    pub completed_step_ids: Vec<String>,
    pub updated_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProgressSnapshot {
    pub current: Option<LearningProgress>,
    pub stale: Option<LearningProgress>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GuideDraft {
    pub id: String,
    pub title: String,
    pub locale: String,
    pub schema_version: String,
    pub skill_identity: SkillIdentity,
    pub skill_revision: SkillRevision,
    pub source_tutorial_key: String,
    pub converted_at: String,
    pub revision_key: String,
    pub steps: Vec<GuideStep>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PracticeInstallPreview {
    pub required: bool,
    pub skill_identity: SkillIdentity,
    pub skill_revision: SkillRevision,
    pub display_name: String,
    pub install_url: String,
    pub content_root: String,
    pub runs_author_commands: bool,
}

impl Guide {
    pub fn new(
        id: GuideId,
        title: impl Into<String>,
        locale: impl Into<String>,
        summary: impl Into<String>,
        schema_version: impl Into<String>,
        skill_identity: SkillIdentity,
        skill_revision: SkillRevision,
        steps: Vec<GuideStep>,
    ) -> Result<Self, AppError> {
        let skill_identity = skill_identity.verified()?;
        let skill_revision = skill_revision.verified(&skill_identity)?;
        let steps = verify_steps(steps)?;
        let title = title.into();
        let locale = locale.into();
        let summary = summary.into();
        let schema_version = schema_version.into();
        let revision_key = guide_revision_key(
            &id,
            &title,
            &locale,
            &schema_version,
            &skill_revision.key,
            &steps,
        );
        Ok(Self {
            id,
            title,
            locale,
            summary,
            schema_version,
            skill_identity,
            skill_revision,
            revision_key,
            steps,
        })
    }

    pub fn summary(&self, display_name: impl Into<String>) -> GuideSummary {
        GuideSummary {
            id: self.id.clone(),
            title: self.title.clone(),
            locale: self.locale.clone(),
            summary: self.summary.clone(),
            display_name: display_name.into(),
            skill_identity: self.skill_identity.clone(),
            skill_revision: self.skill_revision.clone(),
            revision_key: self.revision_key.clone(),
            step_count: self.steps.len(),
            first_step_id: self
                .steps
                .first()
                .map(|step| step.id.clone())
                .unwrap_or_default(),
        }
    }

    pub fn step(&self, step_id: &str) -> Result<&GuideStep, AppError> {
        self.steps
            .iter()
            .find(|step| step.id == step_id)
            .ok_or_else(|| AppError::Other(format!("Guide step '{step_id}' was not found")))
    }
}

pub fn list_guides() -> Result<Vec<GuideSummary>, AppError> {
    let seed = seed::frontend_design_first_success();
    Ok(vec![seed.summary(seed::SEED_DISPLAY_NAME)])
}

pub fn get_guide(id: &str) -> Result<Option<Guide>, AppError> {
    let wanted = GuideId::new(id)?;
    let seed = seed::frontend_design_first_success();
    if seed.id == wanted {
        return Ok(Some(seed));
    }
    Ok(None)
}

pub fn load_progress(
    guide_id: &GuideId,
    guide_revision: &GuideRevisionKey,
) -> Result<ProgressSnapshot, AppError> {
    store::load_progress(guide_id, guide_revision)
}

pub fn save_progress(progress: &LearningProgress) -> Result<LearningProgress, AppError> {
    let guide = get_guide(progress.guide_id.as_str())?
        .ok_or_else(|| AppError::Other("Cannot save progress for an unknown Guide".to_string()))?;
    if progress.guide_revision_key != guide.revision_key {
        return Err(AppError::Other(
            "Learning progress must target the current Guide revision".to_string(),
        ));
    }
    let known: Vec<&str> = guide.steps.iter().map(|step| step.id.as_str()).collect();
    if !known.contains(&progress.current_step_id.as_str()) {
        return Err(AppError::Other(format!(
            "Learning progress current step '{}' is not in the Guide",
            progress.current_step_id
        )));
    }
    for step_id in &progress.completed_step_ids {
        if !known.contains(&step_id.as_str()) {
            return Err(AppError::Other(format!(
                "Learning progress completed step '{step_id}' is not in the Guide"
            )));
        }
    }
    store::save_progress(progress)
}

pub fn preview_practice_install(
    guide_id: &str,
    step_id: &str,
) -> Result<PracticeInstallPreview, AppError> {
    let guide = get_guide(guide_id)?
        .ok_or_else(|| AppError::Other(format!("Guide '{guide_id}' was not found")))?;
    let step = guide.step(step_id)?;
    let install_url = match &guide.skill_identity.source {
        crate::identity::SkillIdentitySource::Git { repository, .. } => repository.clone(),
        crate::identity::SkillIdentitySource::Channel { .. } => {
            return Err(AppError::Other(
                "Channel Skill practice install is not part of P0".to_string(),
            ));
        }
        crate::identity::SkillIdentitySource::Local { .. } => {
            return Err(AppError::Other(
                "Local Skill practice does not install from a remote".to_string(),
            ));
        }
    };
    let content_root = match &guide.skill_identity.source {
        crate::identity::SkillIdentitySource::Git { content_root, .. }
        | crate::identity::SkillIdentitySource::Channel { content_root, .. } => {
            content_root.clone()
        }
        crate::identity::SkillIdentitySource::Local { .. } => String::new(),
    };
    Ok(PracticeInstallPreview {
        required: step.requires_skill,
        skill_identity: guide.skill_identity.clone(),
        skill_revision: guide.skill_revision.clone(),
        display_name: seed::SEED_DISPLAY_NAME.to_string(),
        install_url,
        content_root,
        runs_author_commands: false,
    })
}

pub fn preview_guide_draft_from_tutorial(
    tutorial: &PrivateTutorial,
    locale: &str,
) -> Result<ConversionPreview, AppError> {
    convert::preview(tutorial, locale)
}

pub fn create_guide_draft_from_tutorial(
    tutorial: &PrivateTutorial,
    locale: &str,
) -> Result<GuideDraft, AppError> {
    let preview = convert::preview(tutorial, locale)?;
    store::commit_draft(preview)
}

pub fn list_guide_drafts() -> Result<Vec<GuideDraft>, AppError> {
    store::list_drafts()
}

fn verify_steps(steps: Vec<GuideStep>) -> Result<Vec<GuideStep>, AppError> {
    if steps.is_empty() {
        return Err(AppError::Other(
            "A Guide must contain at least one step".to_string(),
        ));
    }
    let mut seen = std::collections::BTreeSet::new();
    let mut verified = Vec::with_capacity(steps.len());
    for mut step in steps {
        if step.id.is_empty() || !seen.insert(step.id.clone()) {
            return Err(AppError::Other(
                "Guide steps must have unique, non-empty ids".to_string(),
            ));
        }
        if step.title.trim().is_empty() {
            return Err(AppError::Other("Guide step title is empty".to_string()));
        }
        if step.kind != GuideStepKind::Practice && step.requires_skill {
            return Err(AppError::Other(
                "Only practice steps may require a local Skill".to_string(),
            ));
        }
        if step.kind == GuideStepKind::Practice && !step.requires_skill {
            return Err(AppError::Other(
                "Practice steps must require the bound Skill revision".to_string(),
            ));
        }
        step.blocks = blocks::verify_blocks(step.blocks)?;
        verified.push(step);
    }
    Ok(verified)
}

fn guide_revision_key(
    id: &GuideId,
    title: &str,
    locale: &str,
    schema_version: &str,
    skill_revision: &SkillRevisionKey,
    steps: &[GuideStep],
) -> GuideRevisionKey {
    let payload = serde_json::json!({
        "id": id.as_str(),
        "title": title,
        "locale": locale,
        "schemaVersion": schema_version,
        "skillRevision": skill_revision.as_str(),
        "steps": steps,
    });
    let mut hasher = Sha256::new();
    hasher.update(GUIDE_REVISION_DOMAIN);
    hasher.update(payload.to_string().as_bytes());
    GuideRevisionKey::from_digest(&hasher.finalize())
}

pub(crate) fn draft_revision_key(draft: &GuideDraft) -> String {
    let payload = serde_json::json!({
        "id": draft.id,
        "locale": draft.locale,
        "schemaVersion": draft.schema_version,
        "skillRevision": draft.skill_revision.key.as_str(),
        "sourceTutorialKey": draft.source_tutorial_key,
        "convertedAt": draft.converted_at,
        "steps": draft.steps,
    });
    let mut hasher = Sha256::new();
    hasher.update(DRAFT_REVISION_DOMAIN);
    hasher.update(payload.to_string().as_bytes());
    format!("gdr:v1:{}", hex_digest(&hasher.finalize()))
}

fn hex_digest(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        output.push(HEX[(byte >> 4) as usize] as char);
        output.push(HEX[(byte & 0x0f) as usize] as char);
    }
    output
}
