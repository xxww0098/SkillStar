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
