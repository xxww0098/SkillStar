//! Reusable AI inference core for SkillStar.
//!
//! Pure inference logic (chat completion, summarisation, skill pick, OpenAI /
//! Anthropic / local clients). Model **provider configuration** lives in the
//! sibling [`skillstar-models`](../skillstar_models) crate; this crate depends
//! on it through `skillstar_models::providers` for provider resolution and
//! re-exports [`AiProviderRef`](crate::ai_provider::AiProviderRef) for backward
//! compatibility.

pub mod ai_provider;
