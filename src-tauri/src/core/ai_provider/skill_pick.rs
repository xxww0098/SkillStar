//! AI-driven skill recommendation engine.
//!
//! Pre-ranks installed skills locally, sends bounded candidates to the AI model,
//! aggregates multi-round votes into a stable ranking, and falls back to
//! deterministic local ranking when AI output is partial or invalid.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use tracing::warn;

use super::constants::{
    SKILL_PICK_LOW_SIGNAL_MAX_CANDIDATES, SKILL_PICK_MAX_CANDIDATES,
    SKILL_PICK_MAX_RECOMMENDATIONS, SKILL_PICK_ROUND_MAX_TOKENS,
};
use super::{AiConfig, build_skill_pick_system_prompt, chat_completion_deterministic};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillPickCandidate {
    pub name: String,
    pub description: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillPickRecommendation {
    pub name: String,
    pub score: u8,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillPickResponse {
    pub recommendations: Vec<SkillPickRecommendation>,
    pub fallback_used: bool,
    pub rounds_succeeded: usize,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct SkillPickCatalogEntry {
    name: String,
    description: String,
    local_score: u8,
}

#[derive(Debug, Clone)]
pub(super) struct RankedSkillPickCandidate {
    pub(super) name: String,
    pub(super) description: String,
    pub(super) local_score: u8,
}

#[derive(Debug, Clone)]
pub(super) struct SkillPickRoundRecommendation {
    pub(super) name: String,
    pub(super) score: u8,
    pub(super) reason: String,
    pub(super) rank: usize,
}

#[derive(Debug, Default)]
struct AggregatedSkillPick {
    votes: usize,
    score_sum: u32,
    best_rank: usize,
    local_score: u8,
    reason: String,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum ParsedSkillPickEnvelope {
    Array(Vec<ParsedSkillPickItem>),
    Wrapped {
        recommendations: Vec<ParsedSkillPickItem>,
    },
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum ParsedSkillPickItem {
    Name(String),
    Rich {
        name: String,
        #[serde(default)]
        score: Option<u8>,
        #[serde(default)]
        reason: Option<String>,
    },
}

fn is_low_signal_match_token(token: &str) -> bool {
    matches!(
        token,
        "a" | "an"
            | "and"
            | "app"
            | "apps"
            | "assistant"
            | "build"
            | "for"
            | "from"
            | "help"
            | "in"
            | "into"
            | "of"
            | "on"
            | "or"
            | "project"
            | "skill"
            | "skills"
            | "system"
            | "the"
            | "to"
            | "tool"
            | "tools"
            | "use"
            | "using"
            | "with"
            | "workflow"
            | "workflows"
            | "ai"
    )
}

fn push_match_token_variant(
    tokens: &mut Vec<String>,
    seen: &mut std::collections::HashSet<String>,
    raw_token: &str,
) {
    let token = raw_token
        .trim_matches(|c: char| matches!(c, '.' | '-' | '_' | '/'))
        .trim();
    if token.len() < 2 || is_low_signal_match_token(token) {
        return;
    }

    let owned = token.to_string();
    if seen.insert(owned.clone()) {
        tokens.push(owned.clone());
    }

    for part in token.split(['.', '-', '_', '/']) {
        let part = part.trim();
        if part.len() < 2 || is_low_signal_match_token(part) {
            continue;
        }
        let owned = part.to_string();
        if seen.insert(owned.clone()) {
            tokens.push(owned);
        }
    }
}

fn extract_match_tokens(text: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut seen = std::collections::HashSet::new();
    let mut current = String::new();

    for ch in text.to_lowercase().chars() {
        if ch.is_ascii_alphanumeric() || matches!(ch, '+' | '#' | '.' | '-' | '_' | '/') {
            current.push(ch);
            continue;
        }

        if !current.is_empty() {
            push_match_token_variant(&mut tokens, &mut seen, &current);
            current.clear();
        }
    }

    if !current.is_empty() {
        push_match_token_variant(&mut tokens, &mut seen, &current);
    }

    tokens
}

fn compute_local_skill_pick_score(
    prompt_lower: &str,
    prompt_tokens: &std::collections::HashSet<String>,
    skill: &SkillPickCandidate,
) -> u8 {
    let skill_name_lower = skill.name.to_lowercase();
    let name_tokens = extract_match_tokens(&skill.name);
    let description_tokens = extract_match_tokens(&skill.description);
    let mut score = 0u32;
    let mut name_hits = 0u32;

    if !skill_name_lower.is_empty() && prompt_lower.contains(&skill_name_lower) {
        score += 70;
        name_hits += 1;
    }

    for token in &name_tokens {
        if prompt_tokens.contains(token) {
            name_hits += 1;
            score += 18 + (token.len() as u32).min(10) * 2;
        }
    }

    for token in description_tokens.iter().take(24) {
        if prompt_tokens.contains(token) {
            score += 6 + (token.len() as u32).min(8);
        }
    }

    if name_hits >= 2 {
        score += 12;
    }

    if !name_tokens.is_empty()
        && name_tokens
            .iter()
            .all(|token| prompt_tokens.contains(token))
    {
        score += 10;
    }

    score.min(100) as u8
}

pub(super) fn shortlist_skill_pick_candidates(
    prompt: &str,
    skills: Vec<SkillPickCandidate>,
) -> Vec<RankedSkillPickCandidate> {
    let prompt_lower = prompt.to_lowercase();
    let prompt_tokens: std::collections::HashSet<String> =
        extract_match_tokens(prompt).into_iter().collect();

    let mut ranked: Vec<RankedSkillPickCandidate> = skills
        .into_iter()
        .map(|skill| RankedSkillPickCandidate {
            local_score: compute_local_skill_pick_score(&prompt_lower, &prompt_tokens, &skill),
            name: skill.name,
            description: skill.description,
        })
        .collect();

    ranked.sort_by(|a, b| {
        b.local_score
            .cmp(&a.local_score)
            .then_with(|| a.name.cmp(&b.name))
    });

    if ranked.len() <= SKILL_PICK_MAX_CANDIDATES {
        return ranked;
    }

    let top_score = ranked.first().map(|skill| skill.local_score).unwrap_or(0);
    if top_score == 0 {
        ranked.sort_by(|a, b| a.name.cmp(&b.name));
        ranked.truncate(SKILL_PICK_LOW_SIGNAL_MAX_CANDIDATES.min(ranked.len()));
        return ranked;
    }

    ranked.truncate(SKILL_PICK_MAX_CANDIDATES);
    ranked
}

fn extract_json_payload(raw: &str) -> &str {
    let trimmed = raw.trim();
    let array_start = trimmed.find('[');
    let object_start = trimmed.find('{');

    match (array_start, object_start) {
        (Some(array_idx), Some(object_idx)) if object_idx < array_idx => trimmed
            .rfind('}')
            .map(|end| &trimmed[object_idx..=end])
            .unwrap_or(trimmed),
        (Some(array_idx), _) => trimmed
            .rfind(']')
            .map(|end| &trimmed[array_idx..=end])
            .unwrap_or(trimmed),
        (_, Some(object_idx)) => trimmed
            .rfind('}')
            .map(|end| &trimmed[object_idx..=end])
            .unwrap_or(trimmed),
        _ => trimmed,
    }
}

fn default_skill_pick_score(rank: usize) -> u8 {
    std::cmp::max(80u8.saturating_sub((rank as u8).saturating_mul(6)), 55)
}

pub(super) fn fallback_skill_pick_rank_score(rank: usize) -> u8 {
    std::cmp::max(82u8.saturating_sub((rank as u8).saturating_mul(4)), 40)
}

fn normalize_skill_pick_reason(reason: &str) -> String {
    reason.split_whitespace().collect::<Vec<_>>().join(" ")
}

pub(super) fn parse_skill_pick_response(
    raw: &str,
    valid_names: &std::collections::HashSet<String>,
) -> Result<Vec<SkillPickRoundRecommendation>> {
    let json_str = extract_json_payload(raw);
    let envelope: ParsedSkillPickEnvelope = serde_json::from_str(json_str).with_context(|| {
        format!(
            "Failed to parse AI skill-pick response as structured JSON: {}",
            json_str
        )
    })?;

    let items = match envelope {
        ParsedSkillPickEnvelope::Array(items) => items,
        ParsedSkillPickEnvelope::Wrapped { recommendations } => recommendations,
    };

    let mut seen = std::collections::HashSet::new();
    let mut parsed = Vec::new();

    for (rank, item) in items.into_iter().enumerate() {
        let (name, score, reason) = match item {
            ParsedSkillPickItem::Name(name) => {
                (name, default_skill_pick_score(rank), String::new())
            }
            ParsedSkillPickItem::Rich {
                name,
                score,
                reason,
            } => (
                name,
                score
                    .unwrap_or_else(|| default_skill_pick_score(rank))
                    .clamp(0, 100),
                reason.unwrap_or_default(),
            ),
        };

        if !valid_names.contains(&name) || !seen.insert(name.clone()) {
            continue;
        }

        parsed.push(SkillPickRoundRecommendation {
            name,
            score,
            reason: normalize_skill_pick_reason(&reason),
            rank,
        });
    }

    Ok(parsed)
}

pub(super) fn fallback_skill_pick(
    ranked: &[RankedSkillPickCandidate],
) -> Vec<SkillPickRecommendation> {
    let mut recommendations: Vec<SkillPickRecommendation> = ranked
        .iter()
        .filter(|skill| skill.local_score > 0)
        .take(SKILL_PICK_MAX_RECOMMENDATIONS)
        .enumerate()
        .map(|(rank, skill)| SkillPickRecommendation {
            name: skill.name.clone(),
            score: fallback_skill_pick_rank_score(rank).max(skill.local_score),
            reason: String::new(),
        })
        .collect();

    if recommendations.is_empty() {
        recommendations = ranked
            .iter()
            .take(std::cmp::min(SKILL_PICK_MAX_RECOMMENDATIONS, 6))
            .enumerate()
            .map(|(rank, skill)| SkillPickRecommendation {
                name: skill.name.clone(),
                score: fallback_skill_pick_rank_score(rank),
                reason: String::new(),
            })
            .collect();
    }

    recommendations
}

/// Pick the most relevant skills from installed skills based on a user-provided project description.
/// The picker first applies a deterministic local shortlist, then runs a 3-round AI consensus pass,
/// and finally falls back to the deterministic shortlist if the AI output is partial or invalid.
pub async fn pick_skills(
    config: &AiConfig,
    prompt: &str,
    skills: Vec<SkillPickCandidate>,
) -> Result<SkillPickResponse> {
    if skills.is_empty() {
        return Ok(SkillPickResponse {
            recommendations: Vec::new(),
            fallback_used: false,
            rounds_succeeded: 0,
        });
    }

    let ranked_candidates = shortlist_skill_pick_candidates(prompt, skills);
    let valid_names: std::collections::HashSet<String> = ranked_candidates
        .iter()
        .map(|skill| skill.name.clone())
        .collect();
    let skill_catalog = serde_json::to_string_pretty(
        &ranked_candidates
            .iter()
            .map(|skill| SkillPickCatalogEntry {
                name: skill.name.clone(),
                description: skill.description.clone(),
                local_score: skill.local_score,
            })
            .collect::<Vec<_>>(),
    )
    .context("Failed to serialize skill-pick catalog")?;
    let system_prompt = build_skill_pick_system_prompt(&skill_catalog);

    let seeds = [42u64, 123, 7];
    let mut handles = Vec::new();

    for &seed in &seeds {
        let cfg = config.clone();
        let sp = system_prompt.clone();
        let user_prompt = prompt.to_string();
        handles.push(tokio::spawn(async move {
            chat_completion_deterministic(
                &cfg,
                &sp,
                &user_prompt,
                Some(seed),
                SKILL_PICK_ROUND_MAX_TOKENS,
            )
            .await
        }));
    }

    let local_score_lookup: std::collections::HashMap<&str, u8> = ranked_candidates
        .iter()
        .map(|skill| (skill.name.as_str(), skill.local_score))
        .collect();
    let mut aggregated: std::collections::HashMap<String, AggregatedSkillPick> =
        std::collections::HashMap::new();
    let mut raw_success_count = 0usize;
    let mut parse_success_count = 0usize;

    for handle in handles {
        let result = handle
            .await
            .map_err(|e| anyhow::anyhow!("Skill-pick task panicked: {}", e))?;

        match result {
            Ok(raw) => {
                raw_success_count += 1;
                match parse_skill_pick_response(&raw, &valid_names) {
                    Ok(round_recommendations) => {
                        parse_success_count += 1;
                        for recommendation in round_recommendations {
                            let entry = aggregated
                                .entry(recommendation.name.clone())
                                .or_insert_with(|| AggregatedSkillPick {
                                    best_rank: recommendation.rank,
                                    local_score: *local_score_lookup
                                        .get(recommendation.name.as_str())
                                        .unwrap_or(&0),
                                    ..Default::default()
                                });

                            entry.votes += 1;
                            entry.score_sum += recommendation.score as u32;
                            entry.best_rank = entry.best_rank.min(recommendation.rank);
                            if entry.reason.is_empty() && !recommendation.reason.is_empty() {
                                entry.reason = recommendation.reason.clone();
                            }
                        }
                    }
                    Err(err) => {
                        warn!(target: "ai_pick_skills", error = %err, "failed to parse round response");
                    }
                }
            }
            Err(err) => {
                warn!(target: "ai_pick_skills", error = %err, "round failed");
            }
        }
    }

    if raw_success_count == 0 {
        anyhow::bail!("All 3 AI skill-pick rounds failed. Please check your AI provider settings.");
    }

    let threshold = if parse_success_count >= 2 { 2 } else { 1 };
    let mut recommendations: Vec<SkillPickRecommendation> = aggregated
        .into_iter()
        .filter(|(_, aggregate)| aggregate.votes >= threshold)
        .map(|(name, aggregate)| {
            let average_score = (aggregate.score_sum / aggregate.votes as u32) as u8;
            SkillPickRecommendation {
                name,
                score: average_score.max(aggregate.local_score),
                reason: aggregate.reason,
            }
        })
        .collect();

    recommendations.sort_by(|a, b| {
        let left = local_score_lookup
            .get(a.name.as_str())
            .copied()
            .unwrap_or(0);
        let right = local_score_lookup
            .get(b.name.as_str())
            .copied()
            .unwrap_or(0);
        b.score
            .cmp(&a.score)
            .then_with(|| right.cmp(&left))
            .then_with(|| a.name.cmp(&b.name))
    });
    recommendations.truncate(SKILL_PICK_MAX_RECOMMENDATIONS);

    let fallback_used = parse_success_count == 0 || recommendations.is_empty();
    if fallback_used {
        recommendations = fallback_skill_pick(&ranked_candidates);
    }

    Ok(SkillPickResponse {
        recommendations,
        fallback_used,
        rounds_succeeded: parse_success_count,
    })
}
