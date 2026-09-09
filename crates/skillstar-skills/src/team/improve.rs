//! Friction capture, learnings, usage, skill health, and digest.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use skillstar_core::infra::error::AppError;
use uuid::Uuid;

use super::FRICTION_THRESHOLD;
use super::recall::installed_skill_names;
use super::store::{self, UsageEvent};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Learning {
    pub id: String,
    pub title: String,
    pub body: String,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub skill_name: Option<String>,
    pub confidence: f32,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub struct LearningDraft {
    pub title: String,
    pub body: String,
    pub tags: Vec<String>,
    pub skill_name: Option<String>,
    pub confidence: f32,
}

#[derive(Debug, Clone)]
pub struct FrictionInput {
    pub interrupts: u32,
    pub rejects: u32,
    pub retries: u32,
    pub corrections: u32,
    pub task: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FrictionRecord {
    pub id: String,
    pub interrupts: u32,
    pub rejects: u32,
    pub retries: u32,
    pub corrections: u32,
    pub score: u32,
    pub worth_documenting: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub task: Option<String>,
    pub at: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SkillHealthStatus {
    Healthy,
    Aging,
    Silent,
    Stale,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillHealth {
    pub name: String,
    pub usage_count: u32,
    pub recall_count: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_used_at: Option<DateTime<Utc>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_recalled_at: Option<DateTime<Utc>>,
    pub freshness: f32,
    pub score: f32,
    pub status: SkillHealthStatus,
    pub silent: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TeamDigest {
    pub generated_at: DateTime<Utc>,
    pub skill_count: usize,
    pub learning_count: usize,
    pub coverage: f32,
    pub friction_sessions: usize,
    pub worth_documenting: usize,
    pub silent_skills: Vec<String>,
    pub top_skills: Vec<SkillUsageRow>,
    pub recent_learnings: Vec<Learning>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillUsageRow {
    pub name: String,
    pub usage_count: u32,
}

#[must_use]
pub fn score_friction(interrupts: u32, rejects: u32, retries: u32, corrections: u32) -> u32 {
    interrupts.saturating_mul(2)
        + rejects.saturating_mul(2)
        + retries
        + corrections.saturating_mul(2)
}

#[must_use]
pub fn is_worth_documenting(score: u32) -> bool {
    score >= FRICTION_THRESHOLD
}

pub fn share_learning(draft: LearningDraft, now: DateTime<Utc>) -> Result<Learning, AppError> {
    let title = draft.title.trim();
    let body = draft.body.trim();
    if title.is_empty() {
        return Err(AppError::Other(
            "Learning title cannot be empty. Pass --title with a short summary.".to_string(),
        ));
    }
    if body.is_empty() {
        return Err(AppError::Other(
            "Learning body cannot be empty. Pass --body with what the session taught.".to_string(),
        ));
    }
    if let Some(name) = draft.skill_name.as_deref() {
        crate::content::validate_skill_name(name)?;
    }
    let confidence = draft.confidence.clamp(0.0, 1.0);
    let learning = Learning {
        id: Uuid::new_v4().to_string(),
        title: title.to_string(),
        body: body.to_string(),
        tags: draft
            .tags
            .into_iter()
            .map(|tag| tag.trim().to_string())
            .filter(|tag| !tag.is_empty())
            .collect(),
        skill_name: draft
            .skill_name
            .map(|name| name.trim().to_string())
            .filter(|name| !name.is_empty()),
        confidence,
        created_at: now,
    };
    store::mutate(|store| {
        store.learnings.push(learning.clone());
        Ok(())
    })?;
    Ok(learning)
}

pub fn list_learnings() -> Result<Vec<Learning>, AppError> {
    let mut learnings = store::load()?.learnings;
    learnings.sort_by(|a, b| b.created_at.cmp(&a.created_at));
    Ok(learnings)
}

pub fn record_friction(
    input: FrictionInput,
    now: DateTime<Utc>,
) -> Result<FrictionRecord, AppError> {
    let score = score_friction(
        input.interrupts,
        input.rejects,
        input.retries,
        input.corrections,
    );
    let record = FrictionRecord {
        id: Uuid::new_v4().to_string(),
        interrupts: input.interrupts,
        rejects: input.rejects,
        retries: input.retries,
        corrections: input.corrections,
        score,
        worth_documenting: is_worth_documenting(score),
        task: input
            .task
            .map(|task| task.trim().to_string())
            .filter(|task| !task.is_empty()),
        at: now,
    };
    store::mutate(|store| {
        store.friction.push(record.clone());
        Ok(())
    })?;
    Ok(record)
}

pub fn record_usage(skill_name: &str, now: DateTime<Utc>) -> Result<(), AppError> {
    crate::content::validate_skill_name(skill_name)?;
    if !installed_skill_names()
        .iter()
        .any(|name| name == skill_name)
    {
        return Err(AppError::SkillNotFound {
            name: skill_name.to_string(),
        });
    }
    store::mutate(|store| {
        store.usage.push(UsageEvent {
            skill_name: skill_name.to_string(),
            at: now,
        });
        Ok(())
    })
}

pub fn health(now: DateTime<Utc>) -> Result<Vec<SkillHealth>, AppError> {
    let store = store::load()?;
    let names = installed_skill_names();
    let mut rows: Vec<SkillHealth> = names
        .iter()
        .map(|name| skill_health(name, &store, now))
        .collect();
    rows.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.name.cmp(&b.name))
    });
    Ok(rows)
}

pub fn digest(now: DateTime<Utc>) -> Result<TeamDigest, AppError> {
    let rows = health(now)?;
    let store = store::load()?;
    let skill_count = rows.len();
    let recalled = rows.iter().filter(|row| row.recall_count > 0).count();
    let coverage = if skill_count == 0 {
        0.0
    } else {
        recalled as f32 / skill_count as f32
    };
    let silent_skills = rows
        .iter()
        .filter(|row| row.silent)
        .map(|row| row.name.clone())
        .collect();
    let mut top_skills: Vec<SkillUsageRow> = rows
        .iter()
        .filter(|row| row.usage_count > 0)
        .map(|row| SkillUsageRow {
            name: row.name.clone(),
            usage_count: row.usage_count,
        })
        .collect();
    top_skills.sort_by(|a, b| b.usage_count.cmp(&a.usage_count).then(a.name.cmp(&b.name)));
    top_skills.truncate(8);

    let learning_count = store.learnings.len();
    let mut recent_learnings = store.learnings;
    recent_learnings.sort_by(|a, b| b.created_at.cmp(&a.created_at));
    recent_learnings.truncate(8);

    Ok(TeamDigest {
        generated_at: now,
        skill_count,
        learning_count,
        coverage,
        friction_sessions: store.friction.len(),
        worth_documenting: store
            .friction
            .iter()
            .filter(|item| item.worth_documenting)
            .count(),
        silent_skills,
        top_skills,
        recent_learnings,
    })
}

fn skill_health(name: &str, store: &store::TeamStore, now: DateTime<Utc>) -> SkillHealth {
    let usage: Vec<&UsageEvent> = store
        .usage
        .iter()
        .filter(|event| event.skill_name == name)
        .collect();
    let recalls: Vec<&store::RecallEvent> = store
        .recall_events
        .iter()
        .filter(|event| event.id == name)
        .collect();
    let last_used_at = usage.iter().map(|event| event.at).max();
    let last_recalled_at = recalls.iter().map(|event| event.at).max();
    let last_signal = [last_used_at, last_recalled_at, content_mtime(name)]
        .into_iter()
        .flatten()
        .max();
    let days = last_signal
        .map(|at| (now - at).num_seconds().max(0) as f32 / 86_400.0)
        .unwrap_or(90.0);
    let freshness = (-days / 30.0).exp();
    let usage_count = usage.len() as u32;
    let recall_count = recalls.len() as u32;
    let usage_norm = (1.0 + usage_count as f32).ln() / (1.0 + 12.0_f32).ln();
    let recall_norm = (1.0 + recall_count as f32).ln() / (1.0 + 12.0_f32).ln();
    let score = (0.45 * usage_norm + 0.35 * freshness + 0.20 * recall_norm).clamp(0.0, 1.0);
    let silent = recall_count == 0 && usage_count == 0;
    let status = if silent {
        SkillHealthStatus::Silent
    } else if score >= 0.6 {
        SkillHealthStatus::Healthy
    } else if score >= 0.3 {
        SkillHealthStatus::Aging
    } else {
        SkillHealthStatus::Stale
    };
    SkillHealth {
        name: name.to_string(),
        usage_count,
        recall_count,
        last_used_at,
        last_recalled_at,
        freshness,
        score,
        status,
        silent,
    }
}

fn content_mtime(name: &str) -> Option<DateTime<Utc>> {
    let path = crate::content::resolve_content_dir(name)?.join("SKILL.md");
    let meta = std::fs::metadata(path).ok()?;
    let modified = meta.modified().ok()?;
    Some(DateTime::<Utc>::from(modified))
}
