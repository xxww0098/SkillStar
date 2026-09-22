//! Local team intelligence distilled from teamai-cli's Context + Improvement loop.
//!
//! This is **not** the deleted Learn/tutorial domain ([D-053] in `docs/decisions.md`).
//! It does not read `~/.skillstar/learning/`, spawn ACP, or generate Guide HTML.
//! Persistence is rebuildable `state/team.json` owned by this module.
//!
//! | Surface | Job |
//! |---|---|
//! | [`recall`] | BM25 over installed `SKILL.md` + local learnings, with neighbor boost |
//! | [`health`] | usage × freshness × recall coverage for each installed skill |
//! | [`record_friction`] / [`share_learning`] / [`digest`] | friction capture, notes, weekly-style digest |
//!
//! Callers: CLI (`skillstar team …`). No Tauri command in this slice.

mod improve;
mod recall;
mod store;

pub use improve::{
    FrictionInput, FrictionRecord, Learning, LearningDraft, SkillHealth, SkillHealthStatus,
    TeamDigest, digest, health, is_worth_documenting, list_learnings, record_friction,
    record_usage, score_friction, share_learning,
};
pub use recall::{InstalledSkillHit, RecallHit, RecallKind, recall, search_installed_skills};

pub const STORE_SCHEMA_VERSION: u32 = 1;
pub const FRICTION_THRESHOLD: u32 = 3;
pub const DEFAULT_RECALL_LIMIT: usize = 12;

#[cfg(test)]
mod tests;
