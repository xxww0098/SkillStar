use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use super::*;
use crate::{Skill, SkillCategory, SkillType};

#[derive(Debug, Serialize, Deserialize)]
pub struct MarketplaceResult {
    pub skills: Vec<Skill>,
    pub total_count: u32,
    pub page: u32,
    pub has_more: bool,
}

// ── skills.sh Integration ──────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub(crate) struct SkillsShSearchResponse {
    pub(crate) skills: Vec<SkillsShSkill>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct SkillsShSkill {
    #[serde(rename = "skillId")]
    _skill_id: String,
    pub(crate) name: String,
    pub(crate) description: Option<String>,
    #[serde(rename = "source")]
    pub(crate) source: String,
    pub(crate) installs: u32,
    #[serde(rename = "repoUrl")]
    pub(crate) repo_url: Option<String>,
}

impl From<SkillsShSkill> for Skill {
    fn from(skill_entry: SkillsShSkill) -> Self {
        let git_url = skill_entry.repo_url.unwrap_or_else(|| {
            // source is "org/repo" (e.g. "vercel/ai"), the actual GitHub repo
            format!("https://github.com/{}", skill_entry.source)
        });
        let source = Some(skill_entry.source.clone());
        Skill {
            name: skill_entry.name,
            description: skill_entry.description.unwrap_or_default(),
            localized_description: None,
            skill_type: SkillType::Hub,
            stars: skill_entry.installs,
            installed: false,
            update_available: false,
            last_updated: chrono::Utc::now().to_rfc3339(),
            git_url,
            tree_hash: None,
            category: SkillCategory::None,
            author: Some(skill_entry.source),
            topics: vec![],
            agent_links: Some(Vec::new()),
            rank: None,
            source,
        }
    }
}

/// Search skills.sh registry via official API
/// API endpoint: GET /api/search?q={query}&limit={limit}
/// Note: empty query returns 400 — a query is always required
pub async fn search_skills_sh(query: &str, limit: u32) -> Result<MarketplaceResult> {
    Ok(search_skills_sh_with_meta(query, limit, None).await?.0)
}

/// Search fetch with content-addressing metadata.
pub async fn search_skills_sh_with_meta(
    query: &str,
    limit: u32,
    etag: Option<&str>,
) -> Result<(MarketplaceResult, FetchMeta)> {
    let path = format!(
        "/api/search?q={}&limit={}",
        url_encode_query_component(query),
        limit
    );

    let (body, meta) = fetch_with_failover(&path, etag).await?;
    if meta.payload_sha256.is_empty() {
        return Ok((
            MarketplaceResult {
                skills: Vec::new(),
                total_count: 0,
                page: 1,
                has_more: false,
            },
            meta,
        ));
    }

    let response: SkillsShSearchResponse =
        serde_json::from_str(&body).context("Failed to parse skills.sh response")?;

    let mut skills: Vec<Skill> = response.skills.into_iter().map(Skill::from).collect();
    // Sort by installs descending and assign ranks
    skills.sort_by_key(|s| std::cmp::Reverse(s.stars));
    for (i, skill) in skills.iter_mut().enumerate() {
        skill.rank = Some((i + 1) as u32);
    }
    let total_count = skills.len() as u32;

    Ok((
        MarketplaceResult {
            skills,
            total_count,
            page: 1,
            has_more: false,
        },
        meta,
    ))
}

fn url_encode_query_component(raw_query: &str) -> String {
    raw_query
        .replace(' ', "+")
        .replace(':', "%3A")
        .replace('>', "%3E")
        .replace('<', "%3C")
}
