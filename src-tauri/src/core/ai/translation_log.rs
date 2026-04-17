//! Append-only NDJSON logs for translation flows (`~/.skillstar/logs/translation-YYYY-MM-DD.log`).
//!
//! Retention and daily filenames are handled by [`crate::core::infra::daily_log`].
//! Used for optimization and incident review: no raw SKILL.md body or API keys,
//! only hashes, lengths, timings, and coarse outcomes.

use serde_json::{Value, json};

use crate::core::infra::daily_log;

/// Correlates mdtx pipeline logs with optional UI `request_id` and Tauri command name.
#[derive(Clone, Debug)]
pub struct TranslationMdtxLogCtx {
    pub request_id: Option<String>,
    pub command: &'static str,
}

fn ts_iso() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
}

fn truncate(s: &str, max: usize) -> String {
    if s.len() <= max {
        s.to_string()
    } else {
        format!("{}…", &s[..max.saturating_sub(1)])
    }
}

fn append_json(value: &Value) {
    let Ok(json) = serde_json::to_string(value) else {
        return;
    };
    daily_log::append_ndjson_line("translation", &json);
}

fn base_skill(
    event: &'static str,
    command: &'static str,
    request_id: Option<&str>,
    content_sha256: &str,
    content_len: usize,
    target_language: &str,
    force_refresh: bool,
) -> Value {
    json!({
        "ts": ts_iso(),
        "scope": "skill_md",
        "event": event,
        "command": command,
        "request_id": request_id,
        "content_sha256": content_sha256,
        "content_len": content_len,
        "target_language": target_language,
        "force_refresh": force_refresh,
    })
}

/// SKILL.md stream: session start (after config resolved).
pub fn skill_stream_start(
    request_id: &str,
    content_sha256: &str,
    content_len: usize,
    target_language: &str,
    force_refresh: bool,
) {
    append_json(&base_skill(
        "stream_start",
        "ai_translate_skill_stream",
        Some(request_id),
        content_sha256,
        content_len,
        target_language,
        force_refresh,
    ));
}

pub fn skill_stream_cache_hit(
    request_id: &str,
    content_sha256: &str,
    content_len: usize,
    target_language: &str,
    force_refresh: bool,
    wall_ms: u128,
    cache_stage: &'static str,
) {
    let mut v = base_skill(
        "stream_cache_hit",
        "ai_translate_skill_stream",
        Some(request_id),
        content_sha256,
        content_len,
        target_language,
        force_refresh,
    );
    if let Value::Object(ref mut m) = v {
        m.insert("wall_ms".into(), json!(wall_ms));
        m.insert("cache_stage".into(), json!(cache_stage));
    }
    append_json(&v);
}

pub fn skill_stream_complete(
    request_id: &str,
    content_sha256: &str,
    content_len: usize,
    target_language: &str,
    force_refresh: bool,
    wall_ms: u128,
) {
    let mut v = base_skill(
        "stream_complete",
        "ai_translate_skill_stream",
        Some(request_id),
        content_sha256,
        content_len,
        target_language,
        force_refresh,
    );
    if let Value::Object(ref mut m) = v {
        m.insert("wall_ms".into(), json!(wall_ms));
    }
    append_json(&v);
}

pub fn skill_stream_error(
    request_id: &str,
    content_sha256: &str,
    content_len: usize,
    target_language: &str,
    force_refresh: bool,
    wall_ms: u128,
    error: &str,
) {
    let mut v = base_skill(
        "stream_error",
        "ai_translate_skill_stream",
        Some(request_id),
        content_sha256,
        content_len,
        target_language,
        force_refresh,
    );
    if let Value::Object(ref mut m) = v {
        m.insert("wall_ms".into(), json!(wall_ms));
        m.insert("error".into(), json!(truncate(error, 512)));
    }
    append_json(&v);
}

/// Markdown-translator pipeline finished successfully.
pub fn skill_mdtx_complete(
    ctx: &TranslationMdtxLogCtx,
    content_sha256: &str,
    content_len: usize,
    target_language: &str,
    force_refresh: bool,
    mdtx_ms: u128,
    segments: usize,
    translations: usize,
    validation_passed: bool,
    api_calls: u64,
) {
    append_json(&json!({
        "ts": ts_iso(),
        "scope": "skill_md",
        "event": "mdtx_complete",
        "command": ctx.command,
        "request_id": ctx.request_id.as_deref(),
        "content_sha256": content_sha256,
        "content_len": content_len,
        "target_language": target_language,
        "force_refresh": force_refresh,
        "mdtx_ms": mdtx_ms,
        "segments": segments,
        "translations": translations,
        "validation_passed": validation_passed,
        "api_calls": api_calls,
    }));
}

pub fn skill_mdtx_error(
    ctx: &TranslationMdtxLogCtx,
    content_sha256: &str,
    content_len: usize,
    target_language: &str,
    force_refresh: bool,
    mdtx_ms: u128,
    error: &str,
) {
    append_json(&json!({
        "ts": ts_iso(),
        "scope": "skill_md",
        "event": "mdtx_error",
        "command": ctx.command,
        "request_id": ctx.request_id.as_deref(),
        "content_sha256": content_sha256,
        "content_len": content_len,
        "target_language": target_language,
        "force_refresh": force_refresh,
        "mdtx_ms": mdtx_ms,
        "error": truncate(error, 512),
    }));
}

fn base_short(
    event: &'static str,
    request_id: &str,
    content_sha256: &str,
    content_len: usize,
    target_language: &str,
    force_refresh: bool,
    requires_ai: bool,
) -> Value {
    json!({
        "ts": ts_iso(),
        "scope": "short_text",
        "event": event,
        "command": "ai_translate_short_text_stream_with_source",
        "request_id": request_id,
        "content_sha256": content_sha256,
        "content_len": content_len,
        "target_language": target_language,
        "force_refresh": force_refresh,
        "requires_ai": requires_ai,
    })
}

pub fn short_text_stream_start(
    request_id: &str,
    content_sha256: &str,
    content_len: usize,
    target_language: &str,
    force_refresh: bool,
    requires_ai: bool,
) {
    append_json(&base_short(
        "stream_start",
        request_id,
        content_sha256,
        content_len,
        target_language,
        force_refresh,
        requires_ai,
    ));
}

pub fn short_text_cache_hit(
    request_id: &str,
    content_sha256: &str,
    content_len: usize,
    target_language: &str,
    force_refresh: bool,
    requires_ai: bool,
    wall_ms: u128,
    source: &str,
) {
    let mut v = base_short(
        "stream_cache_hit",
        request_id,
        content_sha256,
        content_len,
        target_language,
        force_refresh,
        requires_ai,
    );
    if let Value::Object(ref mut m) = v {
        m.insert("wall_ms".into(), json!(wall_ms));
        m.insert("source".into(), json!(source));
    }
    append_json(&v);
}

pub fn short_text_stream_ok(
    request_id: &str,
    content_sha256: &str,
    content_len: usize,
    target_language: &str,
    force_refresh: bool,
    requires_ai: bool,
    wall_ms: u128,
    source: &str,
    result_len: usize,
) {
    let mut v = base_short(
        "stream_ok",
        request_id,
        content_sha256,
        content_len,
        target_language,
        force_refresh,
        requires_ai,
    );
    if let Value::Object(ref mut m) = v {
        m.insert("wall_ms".into(), json!(wall_ms));
        m.insert("source".into(), json!(source));
        m.insert("result_len".into(), json!(result_len));
    }
    append_json(&v);
}

pub fn short_text_stream_error(
    request_id: &str,
    content_sha256: &str,
    content_len: usize,
    target_language: &str,
    force_refresh: bool,
    requires_ai: bool,
    wall_ms: u128,
    error: &str,
) {
    let mut v = base_short(
        "stream_error",
        request_id,
        content_sha256,
        content_len,
        target_language,
        force_refresh,
        requires_ai,
    );
    if let Value::Object(ref mut m) = v {
        m.insert("wall_ms".into(), json!(wall_ms));
        m.insert("error".into(), json!(truncate(error, 512)));
    }
    append_json(&v);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn truncate_bounds() {
        assert_eq!(truncate("hi", 10), "hi");
        assert!(truncate("abcdefghij", 6).contains('…'));
    }
}
