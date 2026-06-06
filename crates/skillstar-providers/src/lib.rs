//! Single source of truth for provider metadata across SkillStar.
//!
//! Historically this information was duplicated across three places that drifted
//! apart: the usage-side `catalog` (subscription accounts), the models-side
//! `get_all_presets_flat` (API endpoints), and the per-fetcher `const ENDPOINT`
//! strings. This crate centralizes the shared, behaviour-bearing facts so the
//! other layers can derive from one table instead of re-declaring them.
//!
//! It is intentionally dependency-free — every layer (usage, models, ai) sits
//! above it, so it must not depend on any of them.

pub mod balance;
pub mod identity;
