use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
#[derive(Default)]
pub enum CuratedRegistryKind {
    #[default]
    SkillsSh,
    GitHub,
    Custom,
}

impl CuratedRegistryKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::SkillsSh => "skills_sh",
            Self::GitHub => "github",
            Self::Custom => "custom",
        }
    }
}

impl std::str::FromStr for CuratedRegistryKind {
    type Err = ();

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value.trim().to_ascii_lowercase().as_str() {
            "skills_sh" | "skills.sh" | "skillssh" => Ok(Self::SkillsSh),
            "github" | "git_hub" => Ok(Self::GitHub),
            "custom" => Ok(Self::Custom),
            _ => Err(()),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CuratedRegistryEntry {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub kind: CuratedRegistryKind,
    #[serde(default)]
    pub endpoint: String,
    #[serde(default = "default_curated_registry_enabled")]
    pub enabled: bool,
    #[serde(default = "default_curated_registry_priority")]
    pub priority: i64,
    #[serde(default)]
    pub trust: String,
    #[serde(default)]
    pub last_sync_at: Option<String>,
    #[serde(default)]
    pub last_error: Option<String>,
    #[serde(default)]
    pub created_at: Option<String>,
    #[serde(default)]
    pub updated_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CuratedRegistryUpsert {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub kind: CuratedRegistryKind,
    #[serde(default)]
    pub endpoint: String,
    #[serde(default = "default_curated_registry_enabled")]
    pub enabled: bool,
    #[serde(default = "default_curated_registry_priority")]
    pub priority: i64,
    #[serde(default)]
    pub trust: String,
    #[serde(default)]
    pub last_sync_at: Option<String>,
    #[serde(default)]
    pub last_error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MarketplaceSourceObservation {
    pub source_id: String,
    pub source_skill_id: String,
    pub skill_key: String,
    #[serde(default)]
    pub source_url: String,
    #[serde(default)]
    pub repo_url: String,
    #[serde(default)]
    pub version: Option<String>,
    #[serde(default)]
    pub sha: Option<String>,
    #[serde(default)]
    pub metadata_json: Option<String>,
    #[serde(default)]
    pub fetched_at: Option<String>,
    #[serde(default)]
    pub created_at: Option<String>,
    #[serde(default)]
    pub updated_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MarketplaceSourceObservationUpsert {
    pub source_id: String,
    pub source_skill_id: String,
    pub skill_key: String,
    #[serde(default)]
    pub source_url: String,
    #[serde(default)]
    pub repo_url: String,
    #[serde(default)]
    pub version: Option<String>,
    #[serde(default)]
    pub sha: Option<String>,
    #[serde(default)]
    pub metadata_json: Option<String>,
    #[serde(default)]
    pub fetched_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MarketplaceSourceSummary {
    pub source_id: String,
    pub observation_count: i64,
    pub last_fetched_at: Option<String>,
    pub last_updated_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MarketplaceCategory {
    pub id: String,
    pub label: String,
    pub slug: String,
    #[serde(default)]
    pub parent_id: Option<String>,
    pub position: i64,
    #[serde(default)]
    pub created_at: Option<String>,
    #[serde(default)]
    pub updated_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MarketplaceCategoryUpsert {
    pub label: String,
    #[serde(default)]
    pub slug: Option<String>,
    #[serde(default)]
    pub parent_id: Option<String>,
    #[serde(default)]
    pub position: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MarketplaceSkillCategoryAssignment {
    pub skill_key: String,
    pub category_id: String,
    #[serde(default)]
    pub assigned_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MarketplaceSkillCategoryAssignmentInput {
    pub skill_key: String,
    pub category_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MarketplaceTag {
    pub slug: String,
    pub label: String,
    pub usage_count: i64,
    #[serde(default)]
    pub created_at: Option<String>,
    #[serde(default)]
    pub updated_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MarketplaceTagUpsert {
    pub label: String,
    #[serde(default)]
    pub slug: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MarketplaceSkillTagAssignment {
    pub skill_key: String,
    pub tag_slug: String,
    #[serde(default)]
    pub source_id: Option<String>,
    #[serde(default)]
    pub assigned_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MarketplaceSkillTagAssignmentInput {
    pub skill_key: String,
    pub tag_slugs: Vec<String>,
    #[serde(default)]
    pub source_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MarketplaceRatingSummary {
    pub skill_key: String,
    #[serde(default)]
    pub source_id: Option<String>,
    pub rating_avg: f64,
    pub rating_count: i64,
    pub review_count: i64,
    #[serde(default)]
    pub last_review_at: Option<String>,
    #[serde(default)]
    pub updated_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MarketplaceRatingSummaryUpsert {
    pub skill_key: String,
    #[serde(default)]
    pub source_id: Option<String>,
    pub rating_avg: f64,
    pub rating_count: i64,
    pub review_count: i64,
    #[serde(default)]
    pub last_review_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MarketplaceReview {
    pub review_id: String,
    pub skill_key: String,
    #[serde(default)]
    pub source_id: Option<String>,
    #[serde(default)]
    pub author_hash: Option<String>,
    pub rating: i64,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub body: Option<String>,
    #[serde(default)]
    pub locale: Option<String>,
    #[serde(default)]
    pub status: String,
    #[serde(default)]
    pub reviewed_at: Option<String>,
    #[serde(default)]
    pub created_at: Option<String>,
    #[serde(default)]
    pub updated_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MarketplaceReviewUpsert {
    pub review_id: String,
    pub skill_key: String,
    #[serde(default)]
    pub source_id: Option<String>,
    #[serde(default)]
    pub author_hash: Option<String>,
    pub rating: i64,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub body: Option<String>,
    #[serde(default)]
    pub locale: Option<String>,
    #[serde(default)]
    pub status: Option<String>,
    #[serde(default)]
    pub reviewed_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MarketplaceUpdateNotification {
    pub skill_key: String,
    pub source_id: String,
    #[serde(default)]
    pub installed_version: Option<String>,
    #[serde(default)]
    pub available_version: Option<String>,
    #[serde(default)]
    pub installed_hash: Option<String>,
    #[serde(default)]
    pub available_hash: Option<String>,
    pub detected_at: String,
    #[serde(default)]
    pub dismissed_at: Option<String>,
    #[serde(default)]
    pub message: Option<String>,
    #[serde(default)]
    pub metadata_json: Option<String>,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MarketplaceUpdateNotificationUpsert {
    pub skill_key: String,
    pub source_id: String,
    #[serde(default)]
    pub installed_version: Option<String>,
    #[serde(default)]
    pub available_version: Option<String>,
    #[serde(default)]
    pub installed_hash: Option<String>,
    #[serde(default)]
    pub available_hash: Option<String>,
    #[serde(default)]
    pub detected_at: Option<String>,
    #[serde(default)]
    pub message: Option<String>,
    #[serde(default)]
    pub metadata_json: Option<String>,
}

fn default_curated_registry_enabled() -> bool {
    true
}

fn default_curated_registry_priority() -> i64 {
    100
}
