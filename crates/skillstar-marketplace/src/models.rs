use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum SkillCategory {
    Hot,
    Popular,
    Rising,
    New,
    None,
}

impl std::fmt::Display for SkillCategory {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SkillCategory::Hot => write!(f, "Hot"),
            SkillCategory::Popular => write!(f, "Popular"),
            SkillCategory::Rising => write!(f, "Rising"),
            SkillCategory::New => write!(f, "New"),
            SkillCategory::None => write!(f, ""),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "lowercase")]
pub enum SkillType {
    #[default]
    Hub,
    Local,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Skill {
    pub name: String,
    pub description: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub localized_description: Option<String>,
    #[serde(default)]
    pub skill_type: SkillType,
    pub stars: u32,
    pub installed: bool,
    pub update_available: bool,
    pub last_updated: String,
    pub git_url: String,
    pub tree_hash: Option<String>,
    pub category: SkillCategory,
    pub author: Option<String>,
    pub topics: Vec<String>,
    pub agent_links: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rank: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
}

impl Skill {
    pub fn from_skills_sh(
        name: String,
        description: String,
        stars: u32,
        author: String,
        git_url: String,
    ) -> Self {
        let mut skill = Self {
            name,
            description,
            localized_description: None,
            skill_type: SkillType::Hub,
            stars,
            installed: false,
            update_available: false,
            last_updated: chrono::Utc::now().to_rfc3339(),
            git_url,
            tree_hash: None,
            category: SkillCategory::None,
            source: Some(author.clone()),
            author: Some(author),
            topics: Vec::new(),
            agent_links: Some(Vec::new()),
            rank: None,
        };
        skill.classify();
        skill
    }

    pub fn classify(&mut self) {
        if self.stars > 500 {
            self.category = SkillCategory::Hot;
        } else if self.stars > 100 {
            self.category = SkillCategory::Popular;
        } else if self.stars > 10 {
            self.category = SkillCategory::Rising;
        } else if self.stars > 0 {
            self.category = SkillCategory::New;
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OfficialPublisher {
    pub name: String,
    pub repo: String,
    pub repo_count: u32,
    pub skill_count: u32,
    pub url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CuratedRegistryKind {
    SkillsSh,
    GitHub,
    Custom,
}

impl Default for CuratedRegistryKind {
    fn default() -> Self {
        Self::SkillsSh
    }
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

pub fn extract_github_source_from_url(url: &str) -> Option<String> {
    let lower = url.to_lowercase();
    let prefix = "https://github.com/";
    if !lower.starts_with(prefix) {
        return None;
    }

    let rest = &url[prefix.len()..];
    let clean = rest.trim_end_matches(".git").trim_end_matches('/');
    if clean.contains('/') {
        Some(clean.to_string())
    } else {
        None
    }
}
