use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

pub mod config;
pub mod config_io;
pub mod constants;
pub mod http_client;
pub mod openai_client;
pub mod resolve;
pub mod scan_params;
pub mod skill_pick;
pub mod translate;

#[allow(unused_imports)]
pub use config::{AiConfig, AiProviderRef, ApiFormat, FormatPreset};
#[allow(unused_imports)]
pub use scan_params::{ResolvedScanParams, resolve_scan_params};

// Config load/save, concurrency limiting, crypto, and legacy TOML/meta parsing.
pub use config_io::*;
// Provider resolution + runtime-config helpers + language display names.
pub use resolve::*;

use constants::{
    AI_MAX_TOKENS, MARKETPLACE_SEARCH_MAX_TOKENS, SKILL_PICK_MAX_RECOMMENDATIONS,
    SUMMARY_MAX_TOKENS,
};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ChatCompletionUsage {
    pub prompt_tokens: Option<u32>,
    pub completion_tokens: Option<u32>,
    pub total_tokens: Option<u32>,
}

#[derive(Debug, Clone)]
pub struct ChatCompletionOutput {
    pub content: String,
    pub usage: Option<ChatCompletionUsage>,
}

// ── Prompts ─────────────────────────────────────────────────────────

const SUMMARY_PROMPT: &str = include_str!("../../../../src-tauri/prompts/ai/summary.md");
const PICK_SKILLS_PROMPT: &str = include_str!("../../../../src-tauri/prompts/ai/pick_skills.md");
const MARKETPLACE_SEARCH_PROMPT: &str =
    include_str!("../../../../src-tauri/prompts/ai/marketplace_search.md");

fn build_summary_system_prompt(lang: &str) -> String {
    SUMMARY_PROMPT.replace("{lang}", lang)
}

pub fn build_skill_pick_system_prompt(skill_catalog: &str) -> String {
    PICK_SKILLS_PROMPT
        .replace("{skill_catalog}", skill_catalog)
        .replace(
            "{max_recommendations}",
            &SKILL_PICK_MAX_RECOMMENDATIONS.to_string(),
        )
}

// ── OpenAI-Compatible Chat Completion (via async-openai) ─────────────

use http_client::{get_http_client, request_timeout_duration};

#[derive(Serialize)]
struct AnthropicMessage {
    role: String,
    content: String,
}

#[derive(Serialize)]
struct AnthropicRequest {
    model: String,
    max_tokens: u32,
    system: String,
    messages: Vec<AnthropicMessage>,
}

#[derive(Serialize)]
struct AnthropicStreamRequest {
    model: String,
    max_tokens: u32,
    system: String,
    messages: Vec<AnthropicMessage>,
    stream: bool,
}

#[derive(Deserialize)]
struct AnthropicTextBlock {
    #[serde(rename = "type")]
    kind: String,
    text: Option<String>,
}

#[derive(Deserialize)]
struct AnthropicResponse {
    content: Vec<AnthropicTextBlock>,
    usage: Option<AnthropicUsage>,
}

#[derive(Deserialize)]
struct AnthropicUsage {
    input_tokens: Option<u32>,
    output_tokens: Option<u32>,
}

fn is_anthropic_format(config: &AiConfig) -> bool {
    matches!(config.api_format, ApiFormat::Anthropic)
}

pub(crate) fn is_local_format(config: &AiConfig) -> bool {
    matches!(config.api_format, ApiFormat::Local)
}

/// For local API format, use a dummy token if api_key is empty.
pub(crate) fn effective_api_key(config: &AiConfig) -> String {
    if config.api_key.trim().is_empty() && is_local_format(config) {
        "ollama".to_string()
    } else {
        config.api_key.clone()
    }
}

#[cfg(test)]
pub(crate) fn build_openai_chat_url(base_url: &str) -> String {
    let mut base = base_url.trim_end_matches('/');
    if base.is_empty() {
        base = "https://api.openai.com/v1";
    }
    if base.ends_with("/chat/completions") {
        return base.to_string();
    }
    // Auto-insert /v1 for bare host:port URLs (e.g. http://host:1234)
    // that have no path segment — common with Ollama endpoints.
    if let Some(after_scheme) = base.split_once("://").map(|(_, rest)| rest)
        && !after_scheme.contains('/')
    {
        return format!("{}/v1/chat/completions", base);
    }
    format!("{}/chat/completions", base)
}

pub(crate) fn build_anthropic_messages_url(base_url: &str) -> String {
    let mut base = base_url.trim_end_matches('/');
    if base.is_empty() {
        base = "https://api.anthropic.com";
    }
    if base.ends_with("/v1/messages") || base.ends_with("/messages") {
        base.to_string()
    } else if base.ends_with("/v1") {
        format!("{}/messages", base)
    } else {
        format!("{}/v1/messages", base)
    }
}

async fn openai_chat_completion(
    config: &AiConfig,
    system_prompt: &str,
    user_content: &str,
) -> Result<String> {
    openai_chat_completion_with_opts(config, system_prompt, user_content, 0.3, None, None).await
}

async fn openai_chat_completion_with_opts(
    config: &AiConfig,
    system_prompt: &str,
    user_content: &str,
    temperature: f32,
    seed: Option<u64>,
    max_tokens_override: Option<u32>,
) -> Result<String> {
    openai_chat_completion_with_usage(
        config,
        system_prompt,
        user_content,
        temperature,
        seed,
        max_tokens_override,
    )
    .await
    .map(|output| output.content)
}

async fn openai_chat_completion_with_usage(
    config: &AiConfig,
    system_prompt: &str,
    user_content: &str,
    temperature: f32,
    seed: Option<u64>,
    max_tokens_override: Option<u32>,
) -> Result<ChatCompletionOutput> {
    let _permit = acquire_ai_request_permit(config).await?;
    openai_client::chat_completion_with_usage(
        config,
        system_prompt,
        user_content,
        temperature,
        seed,
        max_tokens_override,
    )
    .await
}

async fn openai_chat_completion_stream<F>(
    config: &AiConfig,
    system_prompt: &str,
    user_content: &str,
    max_tokens: u32,
    on_delta: &mut F,
) -> Result<String>
where
    F: FnMut(&str) -> Result<()>,
{
    let _permit = acquire_ai_request_permit(config).await?;
    openai_client::chat_completion_stream(config, system_prompt, user_content, max_tokens, on_delta)
        .await
}

async fn anthropic_messages_completion(
    config: &AiConfig,
    system_prompt: &str,
    user_content: &str,
    max_tokens: u32,
) -> Result<String> {
    anthropic_messages_completion_with_usage(config, system_prompt, user_content, max_tokens)
        .await
        .map(|output| output.content)
}

async fn anthropic_messages_completion_with_usage(
    config: &AiConfig,
    system_prompt: &str,
    user_content: &str,
    max_tokens: u32,
) -> Result<ChatCompletionOutput> {
    let _permit = acquire_ai_request_permit(config).await?;
    let client = get_http_client()?;
    let url = build_anthropic_messages_url(&config.base_url);

    let body = AnthropicRequest {
        model: config.model.clone(),
        max_tokens,
        system: system_prompt.to_string(),
        messages: vec![AnthropicMessage {
            role: "user".to_string(),
            content: user_content.to_string(),
        }],
    };

    let resp = client
        .post(&url)
        .timeout(request_timeout_duration(config))
        .header("Content-Type", "application/json")
        .header("x-api-key", effective_api_key(config))
        .header("anthropic-version", "2023-06-01")
        .json(&body)
        .send()
        .await
        .context("Failed to send request to AI provider")?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body_text = resp.text().await.unwrap_or_default();
        anyhow::bail!("AI API returned {} — {}", status, body_text);
    }

    let anthropic_resp: AnthropicResponse =
        resp.json().await.context("Failed to parse AI response")?;

    let text = anthropic_resp
        .content
        .iter()
        .filter(|b| b.kind == "text")
        .filter_map(|b| b.text.as_ref().map(|t| t.trim()))
        .filter(|t| !t.is_empty())
        .collect::<Vec<_>>()
        .join("\n\n");

    if text.is_empty() {
        anyhow::bail!("AI returned empty response");
    }

    let usage = anthropic_resp.usage.map(|usage| ChatCompletionUsage {
        prompt_tokens: usage.input_tokens,
        completion_tokens: usage.output_tokens,
        total_tokens: match (usage.input_tokens, usage.output_tokens) {
            (Some(input), Some(output)) => Some(input + output),
            _ => None,
        },
    });

    Ok(ChatCompletionOutput {
        content: text,
        usage,
    })
}

/// Anthropic Messages API with real SSE streaming.
/// Anthropic's SSE format uses event types like `content_block_delta` with
/// `delta.type = "text_delta"` and `delta.text` for incremental content.
async fn anthropic_messages_completion_stream<F>(
    config: &AiConfig,
    system_prompt: &str,
    user_content: &str,
    max_tokens: u32,
    on_delta: &mut F,
) -> Result<String>
where
    F: FnMut(&str) -> Result<()>,
{
    let _permit = acquire_ai_request_permit(config).await?;
    let client = get_http_client()?;
    let url = build_anthropic_messages_url(&config.base_url);

    let body = AnthropicStreamRequest {
        model: config.model.clone(),
        max_tokens,
        system: system_prompt.to_string(),
        messages: vec![AnthropicMessage {
            role: "user".to_string(),
            content: user_content.to_string(),
        }],
        stream: true,
    };

    let mut resp = client
        .post(&url)
        .timeout(request_timeout_duration(config))
        .header("Content-Type", "application/json")
        .header("Accept", "text/event-stream")
        .header("x-api-key", effective_api_key(config))
        .header("anthropic-version", "2023-06-01")
        .json(&body)
        .send()
        .await
        .context("Failed to send streaming request to Anthropic API")?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body_text = resp.text().await.unwrap_or_default();
        anyhow::bail!("Anthropic API returned {} — {}", status, body_text);
    }

    let mut translated = String::new();
    let mut buffer = String::new();
    let mut current_event_type = String::new();
    let mut event_data_lines: Vec<String> = Vec::new();

    while let Some(chunk) = resp
        .chunk()
        .await
        .context("Failed to read streaming response from Anthropic API")?
    {
        let chunk_text = String::from_utf8_lossy(&chunk);
        buffer.push_str(&chunk_text);

        while let Some(newline_idx) = buffer.find('\n') {
            let mut line = buffer[..newline_idx].to_string();
            buffer.drain(..=newline_idx);

            if line.ends_with('\r') {
                line.pop();
            }

            // Empty line = end of SSE event block
            if line.is_empty() {
                if !event_data_lines.is_empty() {
                    let event_payload = event_data_lines.join("\n");
                    process_anthropic_sse_event(
                        &current_event_type,
                        &event_payload,
                        &mut translated,
                        on_delta,
                    )?;
                    event_data_lines.clear();
                }
                current_event_type.clear();
                continue;
            }

            if let Some(event_type) = line.strip_prefix("event:") {
                current_event_type = event_type.trim().to_string();
            } else if let Some(data_part) = line.strip_prefix("data:") {
                event_data_lines.push(data_part.trim_start().to_string());
            }
        }
    }

    // Process any remaining buffered event
    if !event_data_lines.is_empty() {
        let event_payload = event_data_lines.join("\n");
        process_anthropic_sse_event(
            &current_event_type,
            &event_payload,
            &mut translated,
            on_delta,
        )?;
    }

    if translated.trim().is_empty() {
        anyhow::bail!("AI returned empty response");
    }

    Ok(translated)
}

/// Parse a single Anthropic SSE event and extract delta text.
fn process_anthropic_sse_event<F>(
    event_type: &str,
    data: &str,
    translated: &mut String,
    on_delta: &mut F,
) -> Result<()>
where
    F: FnMut(&str) -> Result<()>,
{
    let trimmed = data.trim();
    if trimmed.is_empty() {
        return Ok(());
    }

    let value: serde_json::Value = match serde_json::from_str(trimmed) {
        Ok(v) => v,
        Err(_) => return Ok(()), // skip unparseable events
    };

    // Check for error events
    if event_type == "error" {
        let message = value
            .get("error")
            .and_then(|e| e.get("message"))
            .and_then(|m| m.as_str())
            .or_else(|| value.get("message").and_then(|m| m.as_str()))
            .unwrap_or("Unknown Anthropic API error");
        anyhow::bail!("Anthropic API stream error — {}", message);
    }

    // content_block_delta: extract delta.text
    if event_type == "content_block_delta"
        && let Some(delta_text) = value
            .get("delta")
            .and_then(|d| d.get("text"))
            .and_then(|t| t.as_str())
        && !delta_text.is_empty()
    {
        translated.push_str(delta_text);
        on_delta(delta_text)?;
    }

    Ok(())
}

pub async fn chat_completion(
    config: &AiConfig,
    system_prompt: &str,
    user_content: &str,
) -> Result<String> {
    if is_anthropic_format(config) {
        anthropic_messages_completion(config, system_prompt, user_content, AI_MAX_TOKENS).await
    } else {
        openai_chat_completion(config, system_prompt, user_content).await
    }
}

/// Chat completion with a capped max_tokens.  Reduces inference latency on
/// providers that pre-allocate KV cache proportional to max_tokens.
pub async fn chat_completion_capped(
    config: &AiConfig,
    system_prompt: &str,
    user_content: &str,
    max_response_tokens: u32,
) -> Result<String> {
    chat_completion_capped_with_usage(config, system_prompt, user_content, max_response_tokens)
        .await
        .map(|output| output.content)
}

/// Chat completion with a capped max_tokens plus provider token usage when
/// the API returns it.
pub async fn chat_completion_capped_with_usage(
    config: &AiConfig,
    system_prompt: &str,
    user_content: &str,
    max_response_tokens: u32,
) -> Result<ChatCompletionOutput> {
    if is_anthropic_format(config) {
        anthropic_messages_completion_with_usage(
            config,
            system_prompt,
            user_content,
            max_response_tokens,
        )
        .await
    } else {
        openai_chat_completion_with_usage(
            config,
            system_prompt,
            user_content,
            0.3,
            None,
            Some(max_response_tokens),
        )
        .await
    }
}

/// Chat completion with temperature and seed overrides for deterministic output.
pub(super) async fn chat_completion_deterministic(
    config: &AiConfig,
    system_prompt: &str,
    user_content: &str,
    seed: Option<u64>,
    max_tokens: u32,
) -> Result<String> {
    if is_anthropic_format(config) {
        // Anthropic API does not support temperature/seed overrides in this wrapper,
        // but temperature 0 is roughly achieved by using the same prompt.
        anthropic_messages_completion(config, system_prompt, user_content, max_tokens).await
    } else {
        openai_chat_completion_with_opts(
            config,
            system_prompt,
            user_content,
            0.0,
            seed,
            Some(max_tokens),
        )
        .await
    }
}

/// Generic streaming chat completion dispatcher — routes to OpenAI or Anthropic.
async fn chat_completion_stream<F>(
    config: &AiConfig,
    system_prompt: &str,
    user_content: &str,
    max_tokens: u32,
    on_delta: &mut F,
) -> Result<String>
where
    F: FnMut(&str) -> Result<()>,
{
    if is_anthropic_format(config) {
        anthropic_messages_completion_stream(
            config,
            system_prompt,
            user_content,
            max_tokens,
            on_delta,
        )
        .await
    } else {
        openai_chat_completion_stream(config, system_prompt, user_content, max_tokens, on_delta)
            .await
    }
}

// ── Public API ───────────────────────────────────────────────────────

/// Generate a structured summary of a SKILL.md content.
pub async fn summarize_text(config: &AiConfig, text: &str) -> Result<String> {
    let lang = language_display_name(&config.target_language);
    let system_prompt = build_summary_system_prompt(lang);

    chat_completion_capped(config, &system_prompt, text, SUMMARY_MAX_TOKENS).await
}

/// Generate a structured summary with streaming delta callbacks.
/// Supports both OpenAI and Anthropic formats with real SSE streaming.
pub async fn summarize_text_streaming<F>(
    config: &AiConfig,
    text: &str,
    on_delta: &mut F,
) -> Result<String>
where
    F: FnMut(&str) -> Result<()>,
{
    let lang = language_display_name(&config.target_language);
    let system_prompt = build_summary_system_prompt(lang);

    match chat_completion_stream(config, &system_prompt, text, SUMMARY_MAX_TOKENS, on_delta).await {
        Ok(result) if !result.trim().is_empty() => Ok(result),
        Ok(_) => summarize_text(config, text)
            .await
            .context("Streaming summary returned empty; non-stream fallback failed"),
        Err(err) => summarize_text(config, text).await.with_context(|| {
            format!(
                "Streaming summary failed ({}); non-stream fallback failed",
                err
            )
        }),
    }
}

// ── AI Marketplace Search ───────────────────────────────────────────

/// Extract English search keywords from a natural-language user query.
///
/// The AI decomposes the query into 3-8 single-word / compound-term
/// English keywords suitable for the skills.sh search API.
pub async fn extract_search_keywords(config: &AiConfig, user_query: &str) -> Result<Vec<String>> {
    let raw = chat_completion_capped(
        config,
        MARKETPLACE_SEARCH_PROMPT,
        user_query,
        MARKETPLACE_SEARCH_MAX_TOKENS,
    )
    .await?;

    // The model should return a JSON array like ["react", "typescript", ...]
    // Be lenient: strip markdown fences and leading/trailing noise.
    let trimmed = raw
        .trim()
        .trim_start_matches("```json")
        .trim_start_matches("```")
        .trim_end_matches("```")
        .trim();

    let keywords: Vec<String> = serde_json::from_str(trimmed)
        .with_context(|| format!("AI returned unparseable keyword list: {trimmed}"))?;

    // Filter out empty strings, dedup, and cap at 8.
    let mut seen = std::collections::HashSet::new();
    let deduped: Vec<String> = keywords
        .into_iter()
        .map(|k| k.trim().to_lowercase())
        .filter(|k| !k.is_empty() && seen.insert(k.clone()))
        .take(8)
        .collect();

    if deduped.is_empty() {
        anyhow::bail!("AI returned no usable search keywords");
    }

    Ok(deduped)
}

// ── Skill Pick (delegated to skill_pick.rs) ─────────────────────────

#[allow(unused_imports)]
pub use skill_pick::{SkillPickCandidate, SkillPickRecommendation, SkillPickResponse, pick_skills};

/// Test API connectivity with a minimal request.
pub async fn test_connection(config: &AiConfig) -> Result<u64> {
    let system_prompt = "Reply with exactly: connection_ok";
    let start = std::time::Instant::now();
    let _ = chat_completion(config, system_prompt, "ping").await?;
    Ok(start.elapsed().as_millis() as u64)
}

#[cfg(test)]
mod tests;
