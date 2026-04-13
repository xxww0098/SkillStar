//! Scan / translation budgets derived from `context_window_k`.

use super::config::AiConfig;

/// Resolved scan parameters — auto-calculated from context_window_k,
/// with optional manual overrides from AiConfig fields (when > 0).
#[derive(Debug, Clone, Copy)]
pub struct ResolvedScanParams {
    pub chunk_char_limit: usize,
    pub max_concurrent_requests: u32,
    pub scan_max_response_tokens: u32,
}

/// Derive optimal scan parameters from the user's context_window_k setting.
/// If individual override fields are > 0, they take precedence (power-user escape hatch).
/// Character budget below which SKILL.md translation prefers **one** API call with the full
/// document (when split by headings would otherwise fan out). Scales with `context_window_k`
/// so large-context models translate typical skills in a single round trip (fewer network
/// failures) while small windows still section-split earlier.
pub fn skill_translation_single_pass_char_budget(config: &AiConfig) -> usize {
    let ctx_k = config.context_window_k.max(1) as usize;
    // ~25% of context as input chars at ~2 chars/token; floor keeps 8K-window behaviour ~4K.
    (ctx_k * 500).max(4_000)
}

pub fn resolve_scan_params(config: &AiConfig) -> ResolvedScanParams {
    let ctx_k = config.context_window_k.max(1) as usize;
    let ctx_tokens = ctx_k * 1000;

    // chunk_char_limit: use ~40% of context window for file content
    // 1 token ≈ 2-4 chars; use conservative multiplier of 2
    let auto_chunk = (ctx_tokens * 2 * 40 / 100).max(10_000);
    let chunk_char_limit = if config.chunk_char_limit > 0 {
        config.chunk_char_limit
    } else {
        auto_chunk
    };

    // max_concurrent_requests: scale with context window, clamped
    let max_concurrent_requests = if config.max_concurrent_requests > 0 {
        config.max_concurrent_requests
    } else {
        4 // User requested default fallback to 4 if 0
    };

    // scan_max_response_tokens: small fraction of context, enough for JSON output
    let auto_max_response = (ctx_tokens / 20).clamp(2048, 16384) as u32;
    let scan_max_response_tokens = if config.scan_max_response_tokens > 0 {
        config.scan_max_response_tokens
    } else {
        auto_max_response
    };

    ResolvedScanParams {
        chunk_char_limit,
        max_concurrent_requests,
        scan_max_response_tokens,
    }
}

/// Estimate a reasonable max_tokens for translation output.
/// Translation output is roughly proportional to input length.
/// Uses chars/3 as a rough token estimate, adds 2x headroom, min 1024, max 32K.
pub fn estimate_translation_max_tokens(input: &str) -> u32 {
    let estimated_input_tokens = (input.len() as u32) / 3;
    let estimate = (estimated_input_tokens * 2).max(1024);
    estimate.min(32_768)
}
