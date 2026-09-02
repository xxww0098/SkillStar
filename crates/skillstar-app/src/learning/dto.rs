//! Frontend DTOs for Learn / Guide / Draft. Domain types stay in skillstar-learning.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use skillstar_learning::{
    CalloutTone, ChannelReleaseRef, ContentRevision, ConversionPreview, GitTrackingRef, Guide,
    GuideBlock, GuideDraft, GuideStep, GuideStepKind, GuideSummary, LearningProgress,
    PracticeInstallPreview, ProgressSnapshot, SkillIdentity, SkillIdentitySource, SkillRevision,
    SkillSourceRevision,
};

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "GitTrackingRef.ts")]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum GitTrackingRefDto {
    DefaultBranch,
    Named { name: String },
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "SkillIdentitySource.ts")]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum SkillIdentitySourceDto {
    #[serde(rename_all = "camelCase")]
    Git {
        repository: String,
        tracking_ref: GitTrackingRefDto,
        content_root: String,
    },
    #[serde(rename_all = "camelCase")]
    Local { local_id: String },
    #[serde(rename_all = "camelCase")]
    Channel {
        #[ts(type = "number")]
        repository_id: u64,
        content_root: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "SkillIdentity.ts")]
#[serde(rename_all = "camelCase")]
pub struct SkillIdentityDto {
    pub key: String,
    pub source: SkillIdentitySourceDto,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "ContentRevision.ts")]
#[serde(rename_all = "camelCase")]
pub struct ContentRevisionDto {
    pub hash_version: u32,
    pub content_hash: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "ChannelReleaseRef.ts")]
#[serde(rename_all = "camelCase")]
pub struct ChannelReleaseRefDto {
    #[ts(type = "number")]
    pub revision: u64,
    pub tag_name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "SkillSourceRevision.ts")]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum SkillSourceRevisionDto {
    #[serde(rename_all = "camelCase")]
    Git {
        commit_sha: String,
        tree_hash: String,
    },
    Local,
    #[serde(rename_all = "camelCase")]
    Channel {
        commit_sha: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        release: Option<ChannelReleaseRefDto>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "SkillRevision.ts")]
#[serde(rename_all = "camelCase")]
pub struct SkillRevisionDto {
    pub key: String,
    pub skill_key: String,
    pub content: ContentRevisionDto,
    pub source: SkillSourceRevisionDto,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "GuideBlock.ts")]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum GuideBlockDto {
    Heading { level: u8, text: String },
    Paragraph { text: String },
    List { ordered: bool, items: Vec<String> },
    Code { language: String, code: String },
    Callout { tone: String, text: String },
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "GuideStep.ts")]
#[serde(rename_all = "camelCase")]
pub struct GuideStepDto {
    pub id: String,
    pub kind: String,
    pub title: String,
    pub requires_skill: bool,
    pub blocks: Vec<GuideBlockDto>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "GuideSummary.ts")]
#[serde(rename_all = "camelCase")]
pub struct GuideSummaryDto {
    pub id: String,
    pub title: String,
    pub locale: String,
    pub summary: String,
    pub display_name: String,
    pub skill_identity: SkillIdentityDto,
    pub skill_revision: SkillRevisionDto,
    pub revision_key: String,
    pub step_count: u32,
    pub first_step_id: String,
    pub installed: bool,
    pub skill_drift: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "Guide.ts")]
#[serde(rename_all = "camelCase")]
pub struct GuideDto {
    pub id: String,
    pub title: String,
    pub locale: String,
    pub summary: String,
    pub schema_version: String,
    pub skill_identity: SkillIdentityDto,
    pub skill_revision: SkillRevisionDto,
    pub revision_key: String,
    pub steps: Vec<GuideStepDto>,
    pub installed: bool,
    pub skill_drift: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "LearningProgress.ts")]
#[serde(rename_all = "camelCase")]
pub struct LearningProgressDto {
    pub guide_id: String,
    pub guide_revision_key: String,
    pub current_step_id: String,
    pub completed_step_ids: Vec<String>,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "ProgressSnapshot.ts")]
#[serde(rename_all = "camelCase")]
pub struct ProgressSnapshotDto {
    pub current: Option<LearningProgressDto>,
    pub stale: Option<LearningProgressDto>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "PracticeInstallPreview.ts")]
#[serde(rename_all = "camelCase")]
pub struct PracticeInstallPreviewDto {
    pub required: bool,
    pub skill_identity: SkillIdentityDto,
    pub skill_revision: SkillRevisionDto,
    pub display_name: String,
    pub install_url: String,
    pub content_root: String,
    pub runs_author_commands: bool,
    pub installed: bool,
    pub skill_drift: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "GuideDraft.ts")]
#[serde(rename_all = "camelCase")]
pub struct GuideDraftDto {
    pub id: String,
    pub title: String,
    pub locale: String,
    pub schema_version: String,
    pub skill_identity: SkillIdentityDto,
    pub skill_revision: SkillRevisionDto,
    pub source_tutorial_key: String,
    pub converted_at: String,
    pub revision_key: String,
    pub steps: Vec<GuideStepDto>,
}

impl From<GitTrackingRef> for GitTrackingRefDto {
    fn from(value: GitTrackingRef) -> Self {
        match value {
            GitTrackingRef::DefaultBranch => Self::DefaultBranch,
            GitTrackingRef::Named { name } => Self::Named { name },
        }
    }
}

impl From<SkillIdentity> for SkillIdentityDto {
    fn from(value: SkillIdentity) -> Self {
        Self {
            key: value.key.as_str().to_string(),
            source: match value.source {
                SkillIdentitySource::Git {
                    repository,
                    tracking_ref,
                    content_root,
                } => SkillIdentitySourceDto::Git {
                    repository,
                    tracking_ref: tracking_ref.into(),
                    content_root,
                },
                SkillIdentitySource::Local { local_id } => SkillIdentitySourceDto::Local {
                    local_id: local_id.to_string(),
                },
                SkillIdentitySource::Channel {
                    repository_id,
                    content_root,
                } => SkillIdentitySourceDto::Channel {
                    repository_id,
                    content_root,
                },
            },
        }
    }
}

impl From<SkillRevision> for SkillRevisionDto {
    fn from(value: SkillRevision) -> Self {
        Self {
            key: value.key.as_str().to_string(),
            skill_key: value.skill_key.as_str().to_string(),
            content: ContentRevisionDto::from(value.content),
            source: match value.source {
                SkillSourceRevision::Git {
                    commit_sha,
                    tree_hash,
                } => SkillSourceRevisionDto::Git {
                    commit_sha,
                    tree_hash,
                },
                SkillSourceRevision::Local => SkillSourceRevisionDto::Local,
                SkillSourceRevision::Channel { commit_sha, release } => {
                    SkillSourceRevisionDto::Channel {
                        commit_sha,
                        release: release.map(ChannelReleaseRefDto::from),
                    }
                }
            },
        }
    }
}

impl From<ContentRevision> for ContentRevisionDto {
    fn from(value: ContentRevision) -> Self {
        Self {
            hash_version: value.hash_version,
            content_hash: value.content_hash,
        }
    }
}

impl From<ChannelReleaseRef> for ChannelReleaseRefDto {
    fn from(value: ChannelReleaseRef) -> Self {
        Self {
            revision: value.revision,
            tag_name: value.tag_name,
        }
    }
}

impl From<GuideBlock> for GuideBlockDto {
    fn from(value: GuideBlock) -> Self {
        match value {
            GuideBlock::Heading { level, text } => Self::Heading { level, text },
            GuideBlock::Paragraph { text } => Self::Paragraph { text },
            GuideBlock::List { ordered, items } => Self::List { ordered, items },
            GuideBlock::Code { language, code } => Self::Code { language, code },
            GuideBlock::Callout { tone, text } => Self::Callout {
                tone: match tone {
                    CalloutTone::Info => "info".into(),
                    CalloutTone::Warning => "warning".into(),
                    CalloutTone::Danger => "danger".into(),
                },
                text,
            },
        }
    }
}

impl From<GuideStep> for GuideStepDto {
    fn from(value: GuideStep) -> Self {
        Self {
            id: value.id,
            kind: match value.kind {
                GuideStepKind::Reading => "reading".into(),
                GuideStepKind::Practice => "practice".into(),
                GuideStepKind::Verify => "verify".into(),
            },
            title: value.title,
            requires_skill: value.requires_skill,
            blocks: value.blocks.into_iter().map(GuideBlockDto::from).collect(),
        }
    }
}

impl GuideSummaryDto {
    pub fn from_summary(summary: GuideSummary, installed: bool, skill_drift: bool) -> Self {
        Self {
            id: summary.id.as_str().to_string(),
            title: summary.title,
            locale: summary.locale,
            summary: summary.summary,
            display_name: summary.display_name,
            skill_identity: summary.skill_identity.into(),
            skill_revision: summary.skill_revision.into(),
            revision_key: summary.revision_key.as_str().to_string(),
            step_count: summary.step_count as u32,
            first_step_id: summary.first_step_id,
            installed,
            skill_drift,
        }
    }
}

impl GuideDto {
    pub fn from_guide(guide: Guide, installed: bool, skill_drift: bool) -> Self {
        Self {
            id: guide.id.as_str().to_string(),
            title: guide.title,
            locale: guide.locale,
            summary: guide.summary,
            schema_version: guide.schema_version,
            skill_identity: guide.skill_identity.into(),
            skill_revision: guide.skill_revision.into(),
            revision_key: guide.revision_key.as_str().to_string(),
            steps: guide.steps.into_iter().map(GuideStepDto::from).collect(),
            installed,
            skill_drift,
        }
    }
}

impl From<LearningProgress> for LearningProgressDto {
    fn from(value: LearningProgress) -> Self {
        Self {
            guide_id: value.guide_id.as_str().to_string(),
            guide_revision_key: value.guide_revision_key.as_str().to_string(),
            current_step_id: value.current_step_id,
            completed_step_ids: value.completed_step_ids,
            updated_at: value.updated_at,
        }
    }
}

impl From<ProgressSnapshot> for ProgressSnapshotDto {
    fn from(value: ProgressSnapshot) -> Self {
        Self {
            current: value.current.map(Into::into),
            stale: value.stale.map(Into::into),
        }
    }
}

impl PracticeInstallPreviewDto {
    pub fn from_preview(
        preview: PracticeInstallPreview,
        installed: bool,
        skill_drift: bool,
    ) -> Self {
        Self {
            required: preview.required,
            skill_identity: preview.skill_identity.into(),
            skill_revision: preview.skill_revision.into(),
            display_name: preview.display_name,
            install_url: preview.install_url,
            content_root: preview.content_root,
            runs_author_commands: preview.runs_author_commands,
            installed,
            skill_drift,
        }
    }
}

impl From<GuideDraft> for GuideDraftDto {
    fn from(value: GuideDraft) -> Self {
        Self {
            id: value.id,
            title: value.title,
            locale: value.locale,
            schema_version: value.schema_version,
            skill_identity: value.skill_identity.into(),
            skill_revision: value.skill_revision.into(),
            source_tutorial_key: value.source_tutorial_key,
            converted_at: value.converted_at,
            revision_key: value.revision_key,
            steps: value.steps.into_iter().map(GuideStepDto::from).collect(),
        }
    }
}

impl From<ConversionPreview> for GuideDraftDto {
    fn from(value: ConversionPreview) -> Self {
        Self {
            id: format!("draft:{}", value.source_tutorial_key),
            title: value.title,
            locale: value.locale,
            schema_version: "1".into(),
            skill_identity: value.skill_identity.into(),
            skill_revision: value.skill_revision.into(),
            source_tutorial_key: value.source_tutorial_key,
            converted_at: String::new(),
            revision_key: String::new(),
            steps: value.steps.into_iter().map(GuideStepDto::from).collect(),
        }
    }
}
