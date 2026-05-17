//! Reusable Git operations for SkillStar.
//!
//! Provides clone, fetch, pull, sparse-checkout, tree-hash, and update-check
//! helpers that are agnostic to the caller's application context.

pub mod dismissed_skills;
pub mod gh_manager;
pub mod ops;
pub mod repo_history;
