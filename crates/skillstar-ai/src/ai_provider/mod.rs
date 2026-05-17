use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::{Arc, LazyLock, Mutex};
use tracing::{error, warn};

use skillstar_models::providers;

pub mod config;
pub mod constants;
pub mod http_client;
pub mod openai_client;
pub mod scan_params;
pub mod skill_pick;

#[allow(unused_imports)]
pub use config::{AiConfig, AiProviderRef, ApiFormat, FormatPreset};
#[allow(unused_imports)]
pub use scan_params::{ResolvedScanParams, resolve_scan_params};

use constants::{
    AI_CONFIG_CACHE_TTL, AI_MAX_TOKENS, MARKETPLACE_SEARCH_MAX_TOKENS,
    SKILL_PICK_MAX_RECOMMENDATIONS, SUMMARY_MAX_TOKENS,
};

static AI_REQUEST_SEMAPHORE: LazyLock<Mutex<Option<(u32, Arc<tokio::sync::Semaphore>)>>> =
    LazyLock::new(|| Mutex::new(None));

// ── AiConfig In-Memory Cache ────────────────────────────────────────
//
// Avoids repeated disk reads + AES-256-GCM decryption on every AI command.
// TTL = 5 seconds; invalidated immediately on save_config.

static AI_CONFIG_CACHE: LazyLock<Mutex<Option<(std::time::Instant, AiConfig)>>> =
    LazyLock::new(|| Mutex::new(None));

/// Invalidate the in-memory AiConfig cache.
/// Called by `save_config` / `save_config_async` so that the next load
/// picks up fresh values from disk.
pub fn invalidate_config_cache() {
    if let Ok(mut guard) = AI_CONFIG_CACHE.lock() {
        *guard = None;
    }
}

fn ai_request_concurrency_budget(config: &AiConfig) -> u32 {
    resolve_scan_params(config).max_concurrent_requests.max(1)
}

fn get_ai_request_semaphore(config: &AiConfig) -> Result<Arc<tokio::sync::Semaphore>> {
    let budget = ai_request_concurrency_budget(config);
    let mut guard = AI_REQUEST_SEMAPHORE
        .lock()
        .map_err(|_| anyhow::anyhow!("AI request semaphore lock poisoned"))?;

    if let Some((cached_budget, semaphore)) = guard.as_ref() {
        if *cached_budget == budget {
            return Ok(semaphore.clone());
        }
    }

    let semaphore = Arc::new(tokio::sync::Semaphore::new(budget as usize));
    *guard = Some((budget, semaphore.clone()));
    Ok(semaphore)
}

async fn acquire_ai_request_permit(config: &AiConfig) -> Result<tokio::sync::OwnedSemaphorePermit> {
    let semaphore = get_ai_request_semaphore(config)?;
    semaphore
        .acquire_owned()
        .await
        .map_err(|_| anyhow::anyhow!("AI request semaphore closed"))
}

fn config_path() -> PathBuf {
    skillstar_core::infra::paths::ai_config_path()
}

fn get_encryption_key() -> aes_gcm::Key<aes_gcm::Aes256Gcm> {
    use sha2::{Digest, Sha256};
    let machine_id = machine_uid::get().unwrap_or_else(|_| "skillstar-fallback-id-123".to_string());
    let mut hasher = Sha256::new();
    hasher.update(b"skillstar-ai-api-key");
    hasher.update(machine_id.as_bytes());
    let result = hasher.finalize();
    *aes_gcm::Key::<aes_gcm::Aes256Gcm>::from_slice(result.as_slice())
}

fn encrypt_api_key(plain: &str) -> String {
    use aes_gcm::{
        Aes256Gcm, KeyInit,
        aead::{Aead, AeadCore, OsRng},
    };
    use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};

    if plain.is_empty() {
        return String::new();
    }
    let key = get_encryption_key();
    let cipher = Aes256Gcm::new(&key);
    let nonce = Aes256Gcm::generate_nonce(&mut OsRng); // 96-bits; unique per message
    match cipher.encrypt(&nonce, plain.as_bytes()) {
        Ok(ciphertext) => {
            let mut combined = nonce.to_vec();
            combined.extend_from_slice(&ciphertext);
            BASE64.encode(combined)
        }
        Err(_) => plain.to_string(), // fallback
    }
}

fn decrypt_api_key(encoded: &str) -> String {
    use aes_gcm::{Aes256Gcm, KeyInit, Nonce, aead::Aead};
    use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};

    if encoded.is_empty() {
        return String::new();
    }
    let Ok(decoded) = BASE64.decode(encoded) else {
        return encoded.to_string();
    };
    if decoded.len() < 12 {
        return encoded.to_string();
    }
    let (nonce_bytes, ciphertext) = decoded.split_at(12);
    let nonce = Nonce::from_slice(nonce_bytes);
    let key = get_encryption_key();
    let cipher = Aes256Gcm::new(&key);
    match cipher.decrypt(nonce, ciphertext) {
        Ok(plaintext) => String::from_utf8(plaintext).unwrap_or_else(|_| encoded.to_string()),
        Err(_) => encoded.to_string(),
    }
}

#[must_use]
pub fn load_config() -> AiConfig {
    // Try in-memory cache first (avoids disk read + AES decrypt).
    if let Ok(guard) = AI_CONFIG_CACHE.lock() {
        if let Some((ts, cached)) = guard.as_ref() {
            if ts.elapsed() < AI_CONFIG_CACHE_TTL {
                return cached.clone();
            }
        }
    }

    let fresh = load_config_from_disk();

    // Warm cache.
    if let Ok(mut guard) = AI_CONFIG_CACHE.lock() {
        *guard = Some((std::time::Instant::now(), fresh.clone()));
    }

    fresh
}

/// Read config directly from disk (no cache). Used by `load_config` after
/// a cache miss and by any code that explicitly needs the on-disk state.
fn load_config_from_disk() -> AiConfig {
    let path = config_path();
    if !path.exists() {
        return AiConfig::default();
    }
    match std::fs::read_to_string(&path) {
        Ok(content) => match serde_json::from_str::<AiConfig>(&content) {
            Ok(mut config) => {
                config.api_key = decrypt_api_key(&config.api_key);
                config
            }
            Err(err) => {
                warn!(
                    target: "ai_provider",
                    path = %path.display(),
                    error = %err,
                    "failed to parse AI config, using defaults"
                );
                AiConfig::default()
            }
        },
        Err(err) => {
            warn!(
                target: "ai_provider",
                path = %path.display(),
                error = %err,
                "failed to read AI config, using defaults"
            );
            AiConfig::default()
        }
    }
}

pub fn save_config(config: &AiConfig) -> Result<()> {
    let path = config_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).context("Failed to create config directory")?;
    }
    let mut config_to_save = config.clone();
    config_to_save.api_key = encrypt_api_key(&config_to_save.api_key);

    let content =
        serde_json::to_string_pretty(&config_to_save).context("Failed to serialize AI config")?;
    std::fs::write(&path, content).context("Failed to write AI config")?;

    // Invalidate in-memory cache so the next load picks up fresh values.
    invalidate_config_cache();

    Ok(())
}

#[must_use]
pub async fn load_config_async() -> AiConfig {
    tokio::task::spawn_blocking(load_config)
        .await
        .unwrap_or_else(|err| {
            error!(target: "ai_provider", error = %err, "load_config task failed");
            AiConfig::default()
        })
}

#[cfg_attr(not(test), allow(dead_code))]
pub async fn save_config_async(config: &AiConfig) -> Result<()> {
    let config = config.clone();
    tokio::task::spawn_blocking(move || save_config(&config))
        .await
        .map_err(|err| anyhow::anyhow!("save_config task failed: {}", err))?
}

fn parse_toml_string_field(config_text: &str, field: &str) -> Option<String> {
    for line in config_text.lines() {
        let trimmed = line.trim();
        let prefix = format!("{field} =");
        if !trimmed.starts_with(&prefix) {
            continue;
        }
        let rhs = trimmed.split_once('=')?.1.trim();
        if rhs.starts_with('"') {
            let mut chars = rhs.chars();
            let _ = chars.next();
            let mut collected = String::new();
            for ch in chars {
                if ch == '"' {
                    break;
                }
                collected.push(ch);
            }
            return Some(collected).filter(|value| !value.trim().is_empty());
        }
    }
    None
}

fn select_provider_app<'a>(
    store: &'a providers::ProvidersStore,
    app_id: &str,
) -> Option<&'a providers::AppProviders> {
    match app_id {
        "claude" => Some(&store.claude),
        "codex" => Some(&store.codex),
        _ => None,
    }
}

pub fn resolve_provider_ref_parts(
    config: &mut AiConfig,
    app_id: &str,
    provider_id: &str,
) -> Result<String> {
    let app_id = app_id.trim();
    let provider_id = provider_id.trim();

    if provider_id.is_empty() || !matches!(app_id, "claude" | "codex") {
        anyhow::bail!("Unsupported AI provider reference: {app_id}:{provider_id}");
    }

    let store = providers::read_store().context("Failed to read model providers")?;
    let app = select_provider_app(&store, app_id)
        .ok_or_else(|| anyhow::anyhow!("Unsupported AI provider app: {app_id}"))?;
    let entry = app
        .providers
        .get(provider_id)
        .ok_or_else(|| anyhow::anyhow!("Unknown AI provider: {app_id}:{provider_id}"))?;

    let label = entry.name.clone();

    match app_id {
        "claude" => {
            let env = entry
                .settings_config
                .get("env")
                .and_then(|value| value.as_object())
                .ok_or_else(|| anyhow::anyhow!("Claude provider env is missing"))?;

            let api_key = env
                .get("ANTHROPIC_AUTH_TOKEN")
                .or_else(|| env.get("ANTHROPIC_API_KEY"))
                .and_then(|value| value.as_str())
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .ok_or_else(|| anyhow::anyhow!("{label} is missing an API key"))?;

            let base_url = env
                .get("ANTHROPIC_BASE_URL")
                .and_then(|value| value.as_str())
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .unwrap_or("https://api.anthropic.com");

            let model = env
                .get("ANTHROPIC_MODEL")
                .or_else(|| env.get("CLAUDE_CODE_MODEL"))
                .and_then(|value| value.as_str())
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .unwrap_or("claude-sonnet-4-20250514");

            config.api_format = ApiFormat::Anthropic;
            config.api_key = api_key.to_string();
            config.base_url = base_url.to_string();
            config.model = model.to_string();
        }
        "codex" => {
            let auth = entry
                .settings_config
                .get("auth")
                .and_then(|value| value.as_object())
                .ok_or_else(|| anyhow::anyhow!("Codex provider auth is missing"))?;
            let config_text = entry
                .settings_config
                .get("config")
                .and_then(|value| value.as_str())
                .unwrap_or_default();

            let api_key = auth
                .get("OPENAI_API_KEY")
                .and_then(|value| value.as_str())
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .ok_or_else(|| anyhow::anyhow!("{label} is missing an API key"))?;

            let base_url = parse_toml_string_field(config_text, "openai_base_url")
                .or_else(|| parse_toml_string_field(config_text, "base_url"))
                .or_else(|| {
                    entry
                        .meta
                        .as_ref()
                        .and_then(|meta| meta.get("baseURL"))
                        .and_then(|value| value.as_str())
                        .map(str::trim)
                        .filter(|value| !value.is_empty())
                        .map(str::to_string)
                })
                .unwrap_or_else(|| "https://api.openai.com/v1".to_string());
            let model = parse_toml_string_field(config_text, "model").unwrap_or_else(|| {
                if config.model.trim().is_empty() {
                    "gpt-5.4".to_string()
                } else {
                    config.model.clone()
                }
            });

            config.api_format = ApiFormat::Openai;
            config.api_key = api_key.to_string();
            config.base_url = base_url;
            config.model = model;
        }
        _ => anyhow::bail!("Unsupported AI provider app: {app_id}"),
    }

    Ok(label)
}

pub fn resolve_provider_ref(config: &mut AiConfig) -> Result<()> {
    let Some(provider_ref) = config.provider_ref.clone() else {
        return Ok(());
    };

    resolve_provider_ref_parts(config, &provider_ref.app_id, &provider_ref.provider_id).map(|_| ())
}

pub fn resolve_runtime_config(config: &AiConfig) -> Result<AiConfig> {
    let mut resolved = config.clone();
    resolve_provider_ref(&mut resolved)?;
    Ok(resolved)
}

pub fn ai_runtime_ready(config: &AiConfig) -> bool {
    if !config.enabled {
        return false;
    }

    match resolve_runtime_config(config) {
        Ok(resolved) => !resolved.api_key.trim().is_empty() || is_local_format(&resolved),
        Err(_) => false,
    }
}

// ── Language Mapping ────────────────────────────────────────────────

pub fn language_display_name(code: &str) -> &str {
    match code {
        "zh-CN" => "Simplified Chinese",
        "zh-TW" => "Traditional Chinese",
        "en" => "English",
        "ja" => "Japanese",
        "ko" => "Korean",
        "es" => "Spanish",
        "fr" => "French",
        "de" => "German",
        "ru" => "Russian",
        "pt-BR" => "Brazilian Portuguese",
        "ar" => "Arabic",
        "hi" => "Hindi",
        _ => code,
    }
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

use http_client::get_http_client;

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
}

fn is_anthropic_format(config: &AiConfig) -> bool {
    matches!(config.api_format, ApiFormat::Anthropic)
}

fn is_local_format(config: &AiConfig) -> bool {
    matches!(config.api_format, ApiFormat::Local)
}

/// For local API format, use a dummy token if api_key is empty.
fn effective_api_key(config: &AiConfig) -> String {
    if config.api_key.trim().is_empty() && is_local_format(config) {
        "ollama".to_string()
    } else {
        config.api_key.clone()
    }
}

#[cfg(test)]
fn build_openai_chat_url(base_url: &str) -> String {
    let mut base = base_url.trim_end_matches('/');
    if base.is_empty() {
        base = "https://api.openai.com/v1";
    }
    if base.ends_with("/chat/completions") {
        return base.to_string();
    }
    // Auto-insert /v1 for bare host:port URLs (e.g. http://host:1234)
    // that have no path segment — common with Ollama endpoints.
    if let Some(after_scheme) = base.split_once("://").map(|(_, rest)| rest) {
        if !after_scheme.contains('/') {
            return format!("{}/v1/chat/completions", base);
        }
    }
    format!("{}/chat/completions", base)
}

fn build_anthropic_messages_url(base_url: &str) -> String {
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
    let _permit = acquire_ai_request_permit(config).await?;
    openai_client::chat_completion(
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

    Ok(text)
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
    if event_type == "content_block_delta" {
        if let Some(delta_text) = value
            .get("delta")
            .and_then(|d| d.get("text"))
            .and_then(|t| t.as_str())
        {
            if !delta_text.is_empty() {
                translated.push_str(delta_text);
                on_delta(delta_text)?;
            }
        }
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

/// Chat completion with a capped max_tokens for responses that are known to
/// be small (e.g. security scan JSON).  Reduces inference latency on providers
/// that pre-allocate KV cache proportional to max_tokens.
pub async fn chat_completion_capped(
    config: &AiConfig,
    system_prompt: &str,
    user_content: &str,
    max_response_tokens: u32,
) -> Result<String> {
    if is_anthropic_format(config) {
        anthropic_messages_completion(config, system_prompt, user_content, max_response_tokens)
            .await
    } else {
        openai_chat_completion_with_opts(
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
// Internal types needed by tests:
#[cfg(test)]
use skill_pick::{
    RankedSkillPickCandidate, fallback_skill_pick, parse_skill_pick_response,
    shortlist_skill_pick_candidates,
};

/// Test API connectivity with a minimal request.
pub async fn test_connection(config: &AiConfig) -> Result<u64> {
    let system_prompt = "Reply with exactly: connection_ok";
    let start = std::time::Instant::now();
    let _ = chat_completion(config, system_prompt, "ping").await?;
    Ok(start.elapsed().as_millis() as u64)
}

#[cfg(test)]
mod tests {
    use super::{
        AiConfig, ApiFormat, RankedSkillPickCandidate, SkillPickCandidate, fallback_skill_pick,
        parse_skill_pick_response, shortlist_skill_pick_candidates,
    };

    #[test]
    fn parse_skill_pick_response_accepts_structured_items_and_filters_invalid_names() {
        let valid_names = std::collections::HashSet::from([
            "premium-frontend-ui".to_string(),
            "web-coder".to_string(),
        ]);
        let raw = r#"
        [
          {"name":"premium-frontend-ui","score":91,"reason":"  直接覆盖 响应式 设计 与 动效  "},
          {"name":"unknown-skill","score":100,"reason":"ignore me"},
          {"name":"premium-frontend-ui","score":88,"reason":"duplicate"},
          "web-coder"
        ]
        "#;

        let parsed = parse_skill_pick_response(raw, &valid_names).expect("should parse");

        assert_eq!(parsed.len(), 2);
        assert_eq!(parsed[0].name, "premium-frontend-ui");
        assert_eq!(parsed[0].score, 91);
        assert_eq!(parsed[0].reason, "直接覆盖 响应式 设计 与 动效");
        assert_eq!(parsed[1].name, "web-coder");
        assert!(parsed[1].score >= 55);
    }

    #[test]
    fn fallback_skill_pick_uses_rank_gradient_for_low_signal_scores() {
        let ranked = vec![
            RankedSkillPickCandidate {
                name: "arrange".to_string(),
                description: "layout".to_string(),
                local_score: 7,
            },
            RankedSkillPickCandidate {
                name: "extract".to_string(),
                description: "reuse".to_string(),
                local_score: 5,
            },
            RankedSkillPickCandidate {
                name: "refine".to_string(),
                description: "polish".to_string(),
                local_score: 3,
            },
        ];

        let recommendations = fallback_skill_pick(&ranked);
        assert_eq!(recommendations.len(), 3);
        assert!(recommendations[0].score > recommendations[1].score);
        assert!(recommendations[1].score > recommendations[2].score);
        assert_eq!(recommendations[0].score, 82);
        assert_eq!(recommendations[1].score, 78);
        assert_eq!(recommendations[2].score, 74);
    }

    #[test]
    fn shortlist_skill_pick_candidates_prioritizes_direct_keyword_matches() {
        let prompt = "Build a Next.js ecommerce app with TypeScript, responsive UI, and motion-heavy animations.";
        let ranked = shortlist_skill_pick_candidates(
            prompt,
            vec![
                SkillPickCandidate {
                    name: "security-review".to_string(),
                    description: "Audit code for security vulnerabilities.".to_string(),
                },
                SkillPickCandidate {
                    name: "premium-frontend-ui".to_string(),
                    description:
                        "Craft immersive web experiences with advanced motion, typography, and responsive layouts."
                            .to_string(),
                },
                SkillPickCandidate {
                    name: "web-coder".to_string(),
                    description:
                        "Expert web development guidance for HTML, CSS, JavaScript, performance, and accessibility."
                            .to_string(),
                },
            ],
        );

        let premium_index = ranked
            .iter()
            .position(|candidate| candidate.name == "premium-frontend-ui")
            .expect("premium-frontend-ui should be present");
        let security_index = ranked
            .iter()
            .position(|candidate| candidate.name == "security-review")
            .expect("security-review should be present");

        assert!(
            premium_index < security_index,
            "frontend-focused skill should rank ahead of unrelated security review"
        );
        assert!(
            ranked[premium_index].local_score >= ranked[security_index].local_score,
            "direct keyword overlap should produce an equal or higher deterministic score"
        );
    }

    /// Helper: generate a unique temp dir, set env, run, restore.
    /// Uses a global mutex to serialize env-var mutation across parallel tests.
    fn with_temp_data_root<F: FnOnce(&std::path::Path)>(f: F) {
        use std::sync::{LazyLock, Mutex};
        static LOCK: LazyLock<Mutex<()>> = LazyLock::new(|| Mutex::new(()));
        let _guard = LOCK.lock().unwrap();

        let dir = tempfile::tempdir().expect("create temp dir");
        let key = "SKILLSTAR_DATA_DIR";
        let prev = std::env::var(key).ok();
        // SAFETY: test-only, mutex-protected so no concurrent mutation.
        unsafe {
            std::env::set_var(key, dir.path());
        }
        f(dir.path());
        unsafe {
            match prev {
                Some(v) => std::env::set_var(key, v),
                None => std::env::remove_var(key),
            }
        }
    }

    #[test]
    fn load_config_returns_default_when_json_is_corrupted() {
        with_temp_data_root(|_dir| {
            let config_path = super::config_path();
            std::fs::create_dir_all(config_path.parent().expect("config dir"))
                .expect("create config dir");
            std::fs::write(&config_path, "{not-valid-json").expect("write corrupt json");
            super::invalidate_config_cache();
            let loaded = super::load_config();
            let defaults = super::AiConfig::default();
            assert_eq!(loaded.enabled, defaults.enabled);
            assert_eq!(loaded.model, defaults.model);
            assert_eq!(loaded.api_key, defaults.api_key);
        });
    }

    #[test]
    fn save_and_load_config_async_roundtrip_keeps_plain_api_key() {
        with_temp_data_root(|_dir| {
            let rt = tokio::runtime::Runtime::new().expect("create runtime");
            rt.block_on(async {
                let mut cfg = super::AiConfig::default();
                cfg.enabled = true;
                cfg.base_url = "https://api.openai.com/v1".to_string();
                cfg.api_key = "test-secret-key".to_string();
                cfg.model = "gpt-5.4".to_string();
                cfg.target_language = "en".to_string();

                super::save_config_async(&cfg)
                    .await
                    .expect("save config async should succeed");
                let loaded = super::load_config_async().await;

                assert!(loaded.enabled);
                assert_eq!(loaded.base_url, cfg.base_url);
                assert_eq!(loaded.api_key, cfg.api_key);
                assert_eq!(loaded.model, cfg.model);
                assert_eq!(loaded.target_language, cfg.target_language);
            });
        });
    }

    #[test]
    fn ai_runtime_ready_false_when_disabled() {
        let mut cfg = AiConfig::default();
        cfg.enabled = false;
        assert!(!super::ai_runtime_ready(&cfg));
    }

    #[test]
    fn ai_runtime_ready_true_with_api_key() {
        let mut cfg = AiConfig::default();
        cfg.enabled = true;
        cfg.api_key = "sk-test".to_string();
        assert!(super::ai_runtime_ready(&cfg));
    }

    #[test]
    fn ai_runtime_ready_true_for_local_format_without_key() {
        let mut cfg = AiConfig::default();
        cfg.enabled = true;
        cfg.api_format = ApiFormat::Local;
        cfg.api_key = "".to_string();
        cfg.base_url = "http://127.0.0.1:11434".to_string();
        assert!(super::ai_runtime_ready(&cfg));
    }

    #[test]
    fn build_openai_chat_url_normalizes_various_bases() {
        assert_eq!(
            super::build_openai_chat_url("https://api.openai.com/v1"),
            "https://api.openai.com/v1/chat/completions"
        );
        assert_eq!(
            super::build_openai_chat_url("http://localhost:11434"),
            "http://localhost:11434/v1/chat/completions"
        );
        assert_eq!(
            super::build_openai_chat_url("http://host:1234/v1/chat/completions"),
            "http://host:1234/v1/chat/completions"
        );
        assert_eq!(
            super::build_openai_chat_url(""),
            "https://api.openai.com/v1/chat/completions"
        );
    }

    #[test]
    fn build_anthropic_messages_url_normalizes_various_bases() {
        assert_eq!(
            super::build_anthropic_messages_url("https://api.anthropic.com"),
            "https://api.anthropic.com/v1/messages"
        );
        assert_eq!(
            super::build_anthropic_messages_url("https://proxy.example.com/v1"),
            "https://proxy.example.com/v1/messages"
        );
        assert_eq!(
            super::build_anthropic_messages_url("https://proxy.example.com/messages"),
            "https://proxy.example.com/messages"
        );
        assert_eq!(
            super::build_anthropic_messages_url(""),
            "https://api.anthropic.com/v1/messages"
        );
    }

    #[test]
    fn resolve_provider_ref_parts_rejects_empty_provider_id() {
        let mut cfg = AiConfig::default();
        let result = super::resolve_provider_ref_parts(&mut cfg, "claude", "");
        assert!(result.is_err());
    }

    #[test]
    fn resolve_provider_ref_parts_rejects_unsupported_app() {
        let mut cfg = AiConfig::default();
        let result = super::resolve_provider_ref_parts(&mut cfg, "gemini", "some-id");
        assert!(result.is_err());
    }

    #[test]
    fn effective_api_key_returns_ollama_for_local_format() {
        let mut cfg = AiConfig::default();
        cfg.api_format = ApiFormat::Local;
        cfg.api_key = "".to_string();
        assert_eq!(super::effective_api_key(&cfg), "ollama");
    }

    #[test]
    fn effective_api_key_returns_actual_key_for_non_local() {
        let mut cfg = AiConfig::default();
        cfg.api_format = ApiFormat::Openai;
        cfg.api_key = "sk-test".to_string();
        assert_eq!(super::effective_api_key(&cfg), "sk-test");
    }
}
