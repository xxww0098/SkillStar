//! Token budgets and tuning constants for AI calls.

pub(crate) const AI_MAX_TOKENS: u32 = 196_608;
pub(crate) const SUMMARY_MAX_TOKENS: u32 = 4_096;

pub(crate) const AI_CONFIG_CACHE_TTL: std::time::Duration = std::time::Duration::from_secs(5);

pub(crate) const MARKETPLACE_SEARCH_MAX_TOKENS: u32 = 256;
