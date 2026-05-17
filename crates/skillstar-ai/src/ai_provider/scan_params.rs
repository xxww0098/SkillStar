//! Scan budgets derived from `context_window_k`.

use super::config::AiConfig;

/// Resolved scan parameters — auto-calculated from context_window_k,
/// with optional manual overrides from AiConfig fields (when > 0).
#[derive(Debug, Clone, Copy)]
pub struct ResolvedScanParams {
    pub chunk_char_limit: usize,
    pub max_concurrent_requests: u32,
    pub scan_max_response_tokens: u32,
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
