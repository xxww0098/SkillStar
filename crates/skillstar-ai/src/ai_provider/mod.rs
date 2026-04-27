use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::{Arc, LazyLock, Mutex};
use tracing::{debug, error, warn};

use skillstar_model_config::providers;

pub mod config;
pub mod constants;
pub mod http_client;
pub mod scan_params;
pub mod skill_pick;

#[allow(unused_imports)]
pub use config::{AiConfig, AiProviderRef, ApiFormat, FormatPreset};
#[allow(unused_imports)]
pub use scan_params::{
    ResolvedScanParams, estimate_translation_max_tokens, resolve_scan_params,
    skill_translation_single_pass_char_budget,
};

use constants::{
    AI_CONFIG_CACHE_TTL, AI_MAX_TOKENS, MARKETPLACE_SEARCH_MAX_TOKENS, SHORT_TEXT_MAX_TOKENS,
    SKILL_PICK_MAX_RECOMMENDATIONS, SUMMARY_MAX_TOKENS, TRANSLATION_CHUNK_RETRY_MIN_CHARS,
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
    skillstar_infra::paths::ai_config_path()
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
                config.translation_api.decrypt_keys();
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
    config_to_save.translation_api.encrypt_keys();

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

/// Semantic section split: preserves markdown heading boundaries and code fences.
/// Always used for translation (never character-level hard chunking).
fn split_markdown_sections(content: &str) -> Vec<String> {
    if content.is_empty() {
        return vec![String::new()];
    }

    fn is_markdown_heading(trimmed_line: &str) -> bool {
        let hash_count = trimmed_line.chars().take_while(|c| *c == '#').count();
        if hash_count == 0 || hash_count > 6 {
            return false;
        }
        trimmed_line
            .chars()
            .nth(hash_count)
            .map(|ch| ch == ' ')
            .unwrap_or(false)
    }

    let mut sections = Vec::new();
    let mut current = String::new();
    let mut in_fenced_code_block = false;

    for line in content.split_inclusive('\n') {
        let trimmed_start = line.trim_start();
        let starts_fence = trimmed_start.starts_with("```") || trimmed_start.starts_with("~~~");
        let heading_boundary = !in_fenced_code_block && is_markdown_heading(trimmed_start);

        if heading_boundary && !current.is_empty() {
            sections.push(current);
            current = String::new();
        }

        current.push_str(line);

        if starts_fence {
            in_fenced_code_block = !in_fenced_code_block;
        }
    }

    if !current.is_empty() {
        sections.push(current);
    }

    if sections.is_empty() {
        vec![content.to_string()]
    } else {
        sections
    }
}

// ── Prompts ─────────────────────────────────────────────────────────

const TRANSLATE_DOCUMENT_PROMPT: &str =
    include_str!("../../../../src-tauri/prompts/ai/translate_document.md");
const TRANSLATE_DOCUMENT_PROMPT_HY_MT: &str =
    include_str!("../../../../src-tauri/prompts/ai/translate_document_hy_mt.md");
const TRANSLATE_SHORT_PROMPT: &str =
    include_str!("../../../../src-tauri/prompts/ai/translate_short.md");
const TRANSLATE_SHORT_PROMPT_HY_MT: &str =
    include_str!("../../../../src-tauri/prompts/ai/translate_short_hy_mt.md");
const SUMMARY_PROMPT: &str = include_str!("../../../../src-tauri/prompts/ai/summary.md");
const TRANSLATE_CHUNK_PROMPT: &str =
    include_str!("../../../../src-tauri/prompts/ai/translate_chunk.md");
const PICK_SKILLS_PROMPT: &str = include_str!("../../../../src-tauri/prompts/ai/pick_skills.md");
const MARKETPLACE_SEARCH_PROMPT: &str =
    include_str!("../../../../src-tauri/prompts/ai/marketplace_search.md");

fn is_empty_ai_response_error(err: &anyhow::Error) -> bool {
    err.to_string().contains("AI returned empty response")
}

fn is_hy_mt_model(config: &AiConfig) -> bool {
    is_local_format(config) && config.model.trim().to_ascii_lowercase().contains("hy-mt")
}

fn build_translation_system_prompt(
    config: &AiConfig,
    lang: &str,
    source_lang_hint: &str,
) -> String {
    let template = if is_hy_mt_model(config) {
        TRANSLATE_DOCUMENT_PROMPT_HY_MT
    } else {
        TRANSLATE_DOCUMENT_PROMPT
    };

    template
        .replace("{lang}", lang)
        .replace("{source_lang_hint}", source_lang_hint)
}

fn build_short_translation_system_prompt(
    config: &AiConfig,
    lang: &str,
    source_lang_hint: &str,
) -> String {
    let template = if is_hy_mt_model(config) {
        TRANSLATE_SHORT_PROMPT_HY_MT
    } else {
        TRANSLATE_SHORT_PROMPT
    };

    template
        .replace("{lang}", lang)
        .replace("{source_lang_hint}", source_lang_hint)
}

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

fn build_translation_chunk_prompt(
    base_system_prompt: &str,
    chunk_number: usize,
    total: usize,
) -> String {
    TRANSLATE_CHUNK_PROMPT
        .replace("{base_system_prompt}", base_system_prompt)
        .replace("{chunk_number}", &chunk_number.to_string())
        .replace("{total}", &total.to_string())
}

#[allow(dead_code)]
fn split_translation_chunks(text: &str, soft_limit_chars: usize) -> Vec<String> {
    if text.len() <= soft_limit_chars || soft_limit_chars == 0 {
        return vec![text.to_string()];
    }

    let mut chunks = Vec::new();
    let mut current = String::new();
    let mut in_fenced_code_block = false;

    for line in text.split_inclusive('\n') {
        let trimmed_start = line.trim_start();
        let starts_fence = trimmed_start.starts_with("```") || trimmed_start.starts_with("~~~");

        let should_split_before_line = !current.is_empty()
            && current.len() + line.len() > soft_limit_chars
            && !in_fenced_code_block;

        if should_split_before_line {
            chunks.push(current);
            current = String::new();
        }

        current.push_str(line);

        if starts_fence {
            in_fenced_code_block = !in_fenced_code_block;
        }
    }

    if !current.is_empty() {
        chunks.push(current);
    }

    if chunks.is_empty() {
        vec![text.to_string()]
    } else {
        chunks
    }
}

/// Translate text in semantic sections (by markdown heading), never by character count.
/// Each section gets its own timeout and retry, so one slow/failed section does not
/// kill the whole document translation. Falls back to the previous section's heading
/// as context to maintain terminology consistency.
async fn translate_text_in_chunks(
    config: &AiConfig,
    base_system_prompt: &str,
    text: &str,
) -> Result<String> {
    let sections = split_markdown_sections(text);

    // Single section — translate directly without chunking overhead.
    if sections.len() <= 1 {
        let result = chat_completion_capped(
            config,
            base_system_prompt,
            text,
            estimate_translation_max_tokens(text),
        )
        .await?;
        if result.trim().is_empty() {
            anyhow::bail!("AI returned empty response");
        }
        return Ok(result);
    }

    // Multi-section: translate each section with per-section timeout + retry.
    // Convert to owned strings so spawned tasks can hold them for as long as needed.
    let sections: Vec<String> = sections.into_iter().collect();
    let total = sections.len();
    let max_parallel = resolve_scan_params(config)
        .max_concurrent_requests
        .clamp(1, 4) as usize;
    let semaphore = std::sync::Arc::new(tokio::sync::Semaphore::new(max_parallel));

    // (ends_with_newline, translated_text_or_empty_on_error) per section index.
    let results: std::sync::Arc<
        tokio::sync::Mutex<std::collections::HashMap<usize, (bool, String)>>,
    > = std::sync::Arc::new(tokio::sync::Mutex::new(
        std::collections::HashMap::with_capacity(total),
    ));

    let mut tasks = tokio::task::JoinSet::new();

    for (idx, section_text) in sections.into_iter().enumerate() {
        let cfg = config.clone();
        let prompt = build_translation_chunk_prompt(base_system_prompt, idx + 1, total);
        let permit_pool = semaphore.clone();
        let results = results.clone();

        tasks.spawn(async move {
            let _permit = permit_pool.acquire_owned().await.ok();

            let ends_nl = section_text.ends_with('\n');

            let translated = match translate_section_with_retry(&cfg, &prompt, &section_text).await {
                Ok(t) => t,
                Err(e) => {
                    warn!(target: "translate", idx, error = %e, "section translate failed, using empty");
                    String::new()
                }
            };

            let mut guard = results.lock().await;
            guard.insert(idx, (ends_nl, translated));
        });
    }

    while let Some(joined) = tasks.join_next().await {
        if let Err(e) = joined {
            warn!(target: "translate", error = %e, "section task panicked");
        }
    }

    let guard = results.lock().await;
    let mut translated = String::new();
    for idx in 0..total {
        let (ends_nl, chunk_result) = guard
            .get(&idx)
            .map(|(a, b)| (*a, b.clone()))
            .unwrap_or((false, String::new()));
        translated.push_str(&chunk_result);
        if ends_nl && !chunk_result.ends_with('\n') {
            translated.push('\n');
        }
    }

    Ok(translated)
}

/// Extract the first markdown heading from a section for use as context, e.g. "## Installation".
#[allow(dead_code)]
fn extract_section_heading(section: &str) -> String {
    for line in section.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('#') {
            return trimmed.to_string();
        }
        if !trimmed.is_empty() {
            break;
        }
    }
    String::new()
}

/// Retryable per-section translation: 30s timeout + up to 3 attempts with exponential backoff
/// for transient errors (rate-limit, network, timeout). Permanent errors (empty response, quota)
/// fail fast without retry.
async fn translate_section_with_retry(
    config: &AiConfig,
    prompt: &str,
    section: &str,
) -> Result<String> {
    const SECTION_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(30);
    const MAX_RETRIES: u8 = 3;

    for attempt in 0..=MAX_RETRIES {
        let start = std::time::Instant::now();

        let result = tokio::time::timeout(
            SECTION_TIMEOUT,
            chat_completion_capped(
                config,
                prompt,
                section,
                estimate_translation_max_tokens(section),
            ),
        )
        .await;

        match result {
            Ok(Ok(text)) if !text.trim().is_empty() => {
                // Validate translation quality before returning.
                if translation_looks_translated(&config.target_language, section, &text) {
                    return Ok(text);
                }
                // Output doesn't look translated — treat as empty.
                if attempt < MAX_RETRIES {
                    let backoff =
                        std::time::Duration::from_secs(2u64.saturating_pow(attempt as u32));
                    debug!(target: "translate", attempt, "section output not translated, retrying after {:?}", backoff);
                    tokio::time::sleep(backoff).await;
                    continue;
                }
                anyhow::bail!(
                    "Section returned untranslated content after {} attempts",
                    MAX_RETRIES
                );
            }
            Ok(Ok(_)) | Ok(Err(_)) => {
                // Empty or error — check if retryable.
                if attempt < MAX_RETRIES {
                    let backoff =
                        std::time::Duration::from_secs(2u64.saturating_pow(attempt as u32));
                    tokio::time::sleep(backoff).await;
                    continue;
                }
                anyhow::bail!("Section translation failed after {} attempts", MAX_RETRIES);
            }
            Err(_) => {
                // Timeout.
                warn!(target: "translate", elapsed_ms = start.elapsed().as_millis() as u64, attempt, "section translate timed out");
                if attempt < MAX_RETRIES {
                    let backoff =
                        std::time::Duration::from_secs(2u64.saturating_pow(attempt as u32));
                    tokio::time::sleep(backoff).await;
                    continue;
                }
                anyhow::bail!(
                    "Section translation timed out after {} attempts",
                    MAX_RETRIES
                );
            }
        }
    }
    anyhow::bail!("Section translation exhausted all retries")
}

// ── HTTP Client (delegated to http_client.rs) ───────────────────────

use http_client::get_http_client;

// ── OpenAI-Compatible Chat Completion ────────────────────────────────

#[derive(Serialize)]
struct ChatMessage {
    role: String,
    content: String,
}

#[derive(Serialize)]
struct ChatRequest {
    model: String,
    messages: Vec<ChatMessage>,
    temperature: f32,
    max_tokens: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    seed: Option<u64>,
}

#[derive(Serialize)]
struct ChatStreamRequest {
    model: String,
    messages: Vec<ChatMessage>,
    temperature: f32,
    max_tokens: u32,
    stream: bool,
}

#[derive(Deserialize)]
struct ChatChoice {
    message: ChatMessageResponse,
}

#[derive(Deserialize)]
struct ChatMessageResponse {
    content: Option<String>,
}

#[derive(Deserialize)]
struct ChatResponse {
    choices: Vec<ChatChoice>,
}

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
    let client = get_http_client()?;
    let url = build_openai_chat_url(&config.base_url);

    let body = ChatRequest {
        model: config.model.clone(),
        messages: vec![
            ChatMessage {
                role: "system".to_string(),
                content: system_prompt.to_string(),
            },
            ChatMessage {
                role: "user".to_string(),
                content: user_content.to_string(),
            },
        ],
        temperature,
        max_tokens: max_tokens_override.unwrap_or(AI_MAX_TOKENS),
        seed,
    };

    let resp = client
        .post(&url)
        .header("Content-Type", "application/json")
        .header(
            "Authorization",
            format!("Bearer {}", effective_api_key(config)),
        )
        .json(&body)
        .send()
        .await
        .context("Failed to send request to AI provider")?;

    let status = resp.status();
    let body_text = resp
        .text()
        .await
        .context("Failed to read AI response body")?;

    if !status.is_success() {
        error!(
            target: "ai_provider",
            status = %status,
            "AI API error"
        );
        anyhow::bail!("AI API returned {} — {}", status, body_text);
    }

    let chat_resp: ChatResponse = match serde_json::from_str(&body_text) {
        Ok(r) => r,
        Err(e) => {
            error!(
                target: "ai_provider",
                error = %e,
                "failed to parse AI response"
            );
            anyhow::bail!("Failed to parse AI response: {}", e);
        }
    };

    let content = chat_resp
        .choices
        .into_iter()
        .next()
        .and_then(|c| c.message.content);

    match content {
        Some(c) if !c.trim().is_empty() => Ok(c),
        _ => {
            warn!(
                target: "ai_provider",
                "AI returned empty/null content"
            );
            anyhow::bail!("AI returned empty response");
        }
    }
}

fn process_openai_stream_data_event<F>(
    data_lines: &[String],
    translated: &mut String,
    on_delta: &mut F,
) -> Result<()>
where
    F: FnMut(&str) -> Result<()>,
{
    if data_lines.is_empty() {
        return Ok(());
    }

    let joined_data = data_lines.join("\n");
    let trimmed = joined_data.trim();
    if trimmed.is_empty() || trimmed == "[DONE]" {
        return Ok(());
    }

    let value: serde_json::Value = serde_json::from_str(trimmed)
        .with_context(|| "Failed to parse AI streaming payload".to_string())?;

    if let Some(message) = value
        .get("error")
        .and_then(|error| error.get("message"))
        .and_then(|msg| msg.as_str())
    {
        anyhow::bail!("AI API stream error — {}", message);
    }

    let delta = value
        .pointer("/choices/0/delta/content")
        .and_then(|v| v.as_str())
        .or_else(|| {
            value
                .pointer("/choices/0/message/content")
                .and_then(|v| v.as_str())
        });

    if let Some(delta_text) = delta.filter(|s| !s.is_empty()) {
        translated.push_str(delta_text);
        on_delta(delta_text)?;
    }

    Ok(())
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
    let client = get_http_client()?;
    let url = build_openai_chat_url(&config.base_url);

    let body = ChatStreamRequest {
        model: config.model.clone(),
        messages: vec![
            ChatMessage {
                role: "system".to_string(),
                content: system_prompt.to_string(),
            },
            ChatMessage {
                role: "user".to_string(),
                content: user_content.to_string(),
            },
        ],
        temperature: 0.3,
        max_tokens,
        stream: true,
    };

    let mut resp = client
        .post(&url)
        .header("Content-Type", "application/json")
        .header("Accept", "text/event-stream")
        .header(
            "Authorization",
            format!("Bearer {}", effective_api_key(config)),
        )
        .json(&body)
        .send()
        .await
        .context("Failed to send streaming request to AI provider")?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body_text = resp.text().await.unwrap_or_default();
        anyhow::bail!("AI API returned {} — {}", status, body_text);
    }

    let mut translated = String::new();
    let mut buffer = String::new();
    let mut event_data_lines: Vec<String> = Vec::new();

    while let Some(chunk) = resp
        .chunk()
        .await
        .context("Failed to read streaming response from AI provider")?
    {
        let chunk_text = String::from_utf8_lossy(&chunk);
        buffer.push_str(&chunk_text);

        while let Some(newline_idx) = buffer.find('\n') {
            let mut line = buffer[..newline_idx].to_string();
            buffer.drain(..=newline_idx);

            if line.ends_with('\r') {
                line.pop();
            }

            if line.is_empty() {
                process_openai_stream_data_event(&event_data_lines, &mut translated, on_delta)?;
                event_data_lines.clear();
                continue;
            }

            if let Some(data_part) = line.strip_prefix("data:") {
                event_data_lines.push(data_part.trim_start().to_string());
            }
        }
    }

    if !buffer.trim().is_empty() {
        let mut tail = buffer;
        if tail.ends_with('\r') {
            tail.pop();
        }
        if let Some(data_part) = tail.strip_prefix("data:") {
            event_data_lines.push(data_part.trim_start().to_string());
        }
    }

    if !event_data_lines.is_empty() {
        process_openai_stream_data_event(&event_data_lines, &mut translated, on_delta)?;
    }

    if translated.trim().is_empty() {
        anyhow::bail!("AI returned empty response");
    }

    Ok(translated)
}

/// Streaming version of section-based translation. Uses semantic split (markdown headings)
/// instead of character-level chunking. Each section is streamed sequentially; on section
/// failure after retries, emits an error event and aborts the whole stream (streaming
/// cannot recover gracefully from mid-stream gaps the way non-streaming can).
async fn translate_text_in_chunks_streaming<F>(
    config: &AiConfig,
    base_system_prompt: &str,
    text: &str,
    on_delta: &mut F,
) -> Result<String>
where
    F: FnMut(&str) -> Result<()>,
{
    let sections = split_markdown_sections(text);

    // Single section — stream directly without chunking overhead.
    if sections.len() <= 1 {
        let result = chat_completion_stream(
            config,
            base_system_prompt,
            text,
            estimate_translation_max_tokens(text),
            on_delta,
        )
        .await?;
        if result.trim().is_empty() {
            anyhow::bail!("AI returned empty response");
        }
        return Ok(result);
    }

    let total = sections.len();
    let mut translated = String::new();

    for (idx, section) in sections.iter().enumerate() {
        let chunk_number = idx + 1;
        let chunk_prompt = build_translation_chunk_prompt(base_system_prompt, chunk_number, total);

        let section_result =
            translate_streaming_section(config, &chunk_prompt, section, on_delta).await;

        match section_result {
            Ok(section_translated) => {
                translated.push_str(&section_translated);
                if section.ends_with('\n') && !section_translated.ends_with('\n') {
                    translated.push('\n');
                    on_delta("\n")?;
                }
            }
            Err(e) => {
                // In streaming mode, we cannot gracefully skip a failed section because
                // that would produce garbled output. Fail the whole stream.
                anyhow::bail!(
                    "Section {}/{} translation failed after retries: {}",
                    chunk_number,
                    total,
                    e
                );
            }
        }
    }

    Ok(translated)
}

/// Translate a single section with streaming, with per-section timeout and retry.
/// Returns the full section text on success; returns an error on persistent failure.
async fn translate_streaming_section<F>(
    config: &AiConfig,
    prompt: &str,
    section: &str,
    on_delta: &mut F,
) -> Result<String>
where
    F: FnMut(&str) -> Result<()>,
{
    const SECTION_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(30);
    const MAX_RETRIES: u8 = 3;

    for attempt in 0..=MAX_RETRIES {
        let result = tokio::time::timeout(
            SECTION_TIMEOUT,
            chat_completion_stream(
                config,
                prompt,
                section,
                estimate_translation_max_tokens(section),
                on_delta,
            ),
        )
        .await;

        match result {
            Ok(Ok(text)) if !text.trim().is_empty() => {
                if translation_looks_translated(&config.target_language, section, &text) {
                    return Ok(text);
                }
                // Output doesn't look translated.
                if attempt < MAX_RETRIES {
                    let backoff =
                        std::time::Duration::from_secs(2u64.saturating_pow(attempt as u32));
                    debug!(target: "translate", attempt, "streaming section not translated, retrying");
                    tokio::time::sleep(backoff).await;
                    continue;
                }
                anyhow::bail!("Streaming section returned untranslated content");
            }
            Ok(Ok(_)) | Ok(Err(_)) => {
                if attempt < MAX_RETRIES {
                    let backoff =
                        std::time::Duration::from_secs(2u64.saturating_pow(attempt as u32));
                    tokio::time::sleep(backoff).await;
                    continue;
                }
                anyhow::bail!(
                    "Streaming section translation failed after {} attempts",
                    MAX_RETRIES
                );
            }
            Err(_) => {
                warn!(target: "translate", attempt, "streaming section timed out");
                if attempt < MAX_RETRIES {
                    let backoff =
                        std::time::Duration::from_secs(2u64.saturating_pow(attempt as u32));
                    tokio::time::sleep(backoff).await;
                    continue;
                }
                anyhow::bail!("Streaming section timed out after {} attempts", MAX_RETRIES);
            }
        }
    }
    anyhow::bail!("Streaming section exhausted all retries")
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

fn ai_short_text_available(config: &AiConfig) -> bool {
    ai_runtime_ready(config)
}

/// Check if a translation result actually contains characters from the target
/// language's script.  Returns `true` when the result looks genuinely
/// translated (or the target is Latin-script and thus indistinguishable).
///
/// This prevents caching English text as a "translation" when the AI model
/// returns the input verbatim.
pub fn translation_looks_translated(target_language: &str, source: &str, result: &str) -> bool {
    let target_needs_cjk = matches!(
        target_language,
        "zh-CN" | "zh-TW" | "zh-cn" | "zh-tw" | "ja" | "ko"
    );
    if !target_needs_cjk {
        // For Latin-script targets we can't easily validate; assume OK.
        return true;
    }

    // If source is already in the target script, any result is fine.
    let source_lang = detect_short_text_source_lang(source);
    let target_normalized = normalize_lang_code(target_language);
    let source_normalized = normalize_lang_code(source_lang);
    if source_normalized == target_normalized {
        return true;
    }

    // Count CJK / kana / hangul characters in the result.
    let target_script_chars: usize = result
        .chars()
        .filter(|c| is_cjk_ideograph(*c) || is_japanese_kana(*c) || is_hangul(*c))
        .count();

    // Require at least 2 target-script characters, or 10% of the result.
    let alpha_chars: usize = result.chars().filter(|c| c.is_alphabetic()).count();
    if alpha_chars == 0 {
        return target_script_chars > 0;
    }
    let ratio = target_script_chars as f32 / alpha_chars as f32;
    target_script_chars >= 2 && ratio >= 0.08
}

fn normalize_lang_code(code: &str) -> String {
    let trimmed = code.trim();
    if trimmed.is_empty() {
        return "zh-CN".to_string();
    }
    match trimmed {
        "zh-CN" => "zh-CN".to_string(),
        "zh-TW" => "zh-TW".to_string(),
        "pt-BR" => "pt-BR".to_string(),
        _ => trimmed
            .split('-')
            .next()
            .filter(|v| !v.is_empty())
            .unwrap_or(trimmed)
            .to_string(),
    }
}

fn is_cjk_ideograph(c: char) -> bool {
    ('\u{4E00}'..='\u{9FFF}').contains(&c)
        || ('\u{3400}'..='\u{4DBF}').contains(&c)
        || ('\u{F900}'..='\u{FAFF}').contains(&c)
}

fn is_japanese_kana(c: char) -> bool {
    ('\u{3040}'..='\u{30FF}').contains(&c)
}

fn is_hangul(c: char) -> bool {
    ('\u{AC00}'..='\u{D7AF}').contains(&c)
}

fn detect_short_text_source_lang(text: &str) -> &'static str {
    let mut han_count = 0usize;
    let mut kana_count = 0usize;
    let mut hangul_count = 0usize;
    let mut latin_count = 0usize;

    for c in text.chars() {
        if is_japanese_kana(c) {
            kana_count += 1;
        } else if is_hangul(c) {
            hangul_count += 1;
        } else if is_cjk_ideograph(c) {
            han_count += 1;
        } else if c.is_ascii_alphabetic() {
            latin_count += 1;
        }
    }

    let script_total = han_count + kana_count + hangul_count + latin_count;
    if script_total == 0 {
        return "en";
    }

    let hangul_ratio = hangul_count as f32 / script_total as f32;
    if hangul_ratio >= 0.30 || (hangul_count >= 4 && hangul_count >= han_count + kana_count) {
        return "ko";
    }

    let japanese_score = kana_count + (han_count / 2);
    let japanese_ratio = japanese_score as f32 / script_total as f32;
    if japanese_ratio >= 0.30 || kana_count >= 2 {
        return "ja";
    }

    let han_ratio = han_count as f32 / script_total as f32;
    if han_ratio >= 0.30 || (han_count >= 6 && han_count > latin_count) {
        return "zh-CN";
    }

    "en"
}

fn source_lang_hint_from_code(code: &str) -> String {
    let normalized = code.trim();
    if normalized.is_empty() {
        return "Unknown; auto-detect from input.".to_string();
    }
    format!(
        "{} ({normalized}); auto-detect if mixed-language content is present.",
        language_display_name(normalized)
    )
}

// ── Public API ───────────────────────────────────────────────────────

/// Translate a SKILL.md content to the target language.
/// Preserves markdown formatting; only translates natural language text.
pub async fn translate_text(config: &AiConfig, text: &str) -> Result<String> {
    let lang = language_display_name(&config.target_language);
    let source_lang_hint =
        "English (en); technical markdown with possible mixed-language snippets.";
    let system_prompt = build_translation_system_prompt(config, lang, source_lang_hint);
    debug!(
        target: "translate",
        lang = %lang,
        text_len = text.len(),
        "translate_text ENTER"
    );

    match chat_completion_capped(
        config,
        &system_prompt,
        text,
        estimate_translation_max_tokens(text),
    )
    .await
    {
        Ok(result) if !result.trim().is_empty() => {
            debug!(target: "translate", result_len = result.len(), "translate_text → single-pass OK");
            Ok(result)
        }
        Ok(_) => {
            debug!(target: "translate", "translate_text → single-pass returned empty, trying chunks");
            if text.len() < TRANSLATION_CHUNK_RETRY_MIN_CHARS {
                anyhow::bail!("AI returned empty response");
            }
            translate_text_in_chunks(config, &system_prompt, text)
                .await
                .context("Single-pass translation returned empty response; chunked fallback failed")
        }
        Err(err) => {
            warn!(target: "translate", error = %err, "translate_text → single-pass FAILED");
            if text.len() < TRANSLATION_CHUNK_RETRY_MIN_CHARS || !is_empty_ai_response_error(&err) {
                return Err(err);
            }

            translate_text_in_chunks(config, &system_prompt, text).await.with_context(|| {
                format!(
                    "Single-pass translation failed with empty response for {} chars; chunked fallback failed",
                    text.len()
                )
            })
        }
    }
}

/// Translate SKILL.md content with streaming delta callbacks.
/// Supports both OpenAI and Anthropic formats with real SSE streaming.
pub async fn translate_text_streaming<F>(
    config: &AiConfig,
    text: &str,
    on_delta: &mut F,
) -> Result<String>
where
    F: FnMut(&str) -> Result<()>,
{
    let lang = language_display_name(&config.target_language);
    let source_lang_hint =
        "English (en); technical markdown with possible mixed-language snippets.";
    let system_prompt = build_translation_system_prompt(config, lang, source_lang_hint);
    debug!(
        target: "translate",
        lang = %lang,
        text_len = text.len(),
        "translate_text_streaming ENTER"
    );

    match chat_completion_stream(
        config,
        &system_prompt,
        text,
        estimate_translation_max_tokens(text),
        on_delta,
    )
    .await
    {
        Ok(result) if !result.trim().is_empty() => {
            debug!(target: "translate", result_len = result.len(), "translate_text_streaming → stream OK");
            Ok(result)
        }
        Ok(_) => {
            debug!(target: "translate", "translate_text_streaming → stream returned empty, trying chunked");
            if text.len() < TRANSLATION_CHUNK_RETRY_MIN_CHARS {
                anyhow::bail!("AI returned empty response");
            }
            translate_text_in_chunks_streaming(config, &system_prompt, text, on_delta)
                .await
                .context("Streaming translation returned empty response; chunked fallback failed")
        }
        Err(err) => {
            warn!(target: "translate", error = %err, "translate_text_streaming → stream FAILED");
            if text.len() >= TRANSLATION_CHUNK_RETRY_MIN_CHARS && is_empty_ai_response_error(&err) {
                return translate_text_in_chunks_streaming(config, &system_prompt, text, on_delta)
                    .await
                    .with_context(|| {
                        format!(
                            "Single-pass streaming translation failed with empty response for {} chars; chunked fallback failed",
                            text.len()
                        )
                    });
            }

            translate_text(config, text).await.with_context(|| {
                format!(
                    "Streaming translation failed ({}); non-stream fallback failed",
                    err
                )
            })
        }
    }
}

/// Translate a short description / text snippet (not a full Markdown document).
/// Uses a simpler, more direct prompt to avoid the AI treating short text as conversation.
pub async fn translate_short_text(config: &AiConfig, text: &str) -> Result<String> {
    let lang = language_display_name(&config.target_language);
    let source_lang = detect_short_text_source_lang(text);
    let source_lang_hint = source_lang_hint_from_code(source_lang);
    let system_prompt = build_short_translation_system_prompt(config, lang, &source_lang_hint);
    chat_completion_capped(config, &system_prompt, text, SHORT_TEXT_MAX_TOKENS).await
}

/// Batch-translate multiple short texts in a single API call.
///
/// Packs texts with numbered delimiters (`[1] text`, `[2] text`, …) and
/// parses the response back into individual translations.  Falls back to
/// per-item `translate_short_text` when parsing fails.
///
/// Returns a Vec with the same length and order as `texts`.
pub async fn translate_short_texts_batch(config: &AiConfig, texts: &[&str]) -> Result<Vec<String>> {
    if texts.is_empty() {
        return Ok(vec![]);
    }
    if texts.len() == 1 {
        return Ok(vec![translate_short_text(config, texts[0]).await?]);
    }

    let lang = language_display_name(&config.target_language);
    let system_prompt = format!(
        "You are a translation assistant. Translate each numbered item below into {lang}.\n\
         Rules:\n\
         - Preserve the [N] numbering in your output exactly.\n\
         - Translate ONLY the text after each [N] marker.\n\
         - Do NOT add, remove, or reorder items.\n\
         - Keep technical terms, code identifiers, and proper nouns unchanged.\n\
         - Output nothing else — no preamble, no explanation."
    );

    let mut user_message = String::new();
    for (i, text) in texts.iter().enumerate() {
        user_message.push_str(&format!("[{}] {}\n", i + 1, text.trim()));
    }

    // Estimate tokens: each short text ~50 tokens, so batch of 10 ≈ 500 output tokens
    let max_tokens = (texts.len() as u32) * 120 + 200;

    match chat_completion_capped(config, &system_prompt, &user_message, max_tokens).await {
        Ok(response) => {
            match parse_batch_translation_response(&response, texts.len()) {
                Ok(translations) => Ok(translations),
                Err(parse_err) => {
                    warn!(
                        target: "batch_translate",
                        error = %parse_err,
                        "parse failed, falling back to per-item"
                    );
                    // Fallback: translate individually
                    let mut results = Vec::with_capacity(texts.len());
                    for text in texts {
                        match translate_short_text(config, text).await {
                            Ok(t) => results.push(t),
                            Err(_) => results.push(String::new()),
                        }
                    }
                    Ok(results)
                }
            }
        }
        Err(err) => {
            warn!(
                target: "batch_translate",
                error = %err,
                "API failed, falling back to per-item"
            );
            let mut results = Vec::with_capacity(texts.len());
            for text in texts {
                match translate_short_text(config, text).await {
                    Ok(t) => results.push(t),
                    Err(_) => results.push(String::new()),
                }
            }
            Ok(results)
        }
    }
}

/// Parse a batch translation response with [N] markers back into individual items.
fn parse_batch_translation_response(response: &str, expected_count: usize) -> Result<Vec<String>> {
    // Build regex to split on [1], [2], etc.
    let re = regex::Regex::new(r"\[(\d+)\]\s*").context("Failed to compile batch parse regex")?;

    let mut items: Vec<(usize, String)> = Vec::new();
    let mut last_idx: Option<usize> = None;
    let mut last_start: usize = 0;

    for caps in re.captures_iter(response) {
        let Some(full_match) = caps.get(0) else {
            continue;
        };
        if let Some(prev_idx) = last_idx {
            let text = response[last_start..full_match.start()].trim().to_string();
            items.push((prev_idx, text));
        }
        // Extract the index from capture group 1 (digits only).
        let Some(num_match) = caps.get(1) else {
            continue;
        };
        if let Ok(num) = num_match.as_str().parse::<usize>() {
            last_idx = Some(num);
            last_start = full_match.end();
        }
    }
    // Capture the last item
    if let Some(prev_idx) = last_idx {
        let text = response[last_start..].trim().to_string();
        items.push((prev_idx, text));
    }

    if items.len() < expected_count / 2 {
        anyhow::bail!(
            "Parsed only {}/{} items from batch response",
            items.len(),
            expected_count
        );
    }

    // Build ordered result
    let mut result = vec![String::new(); expected_count];
    for (idx, text) in items {
        if idx >= 1 && idx <= expected_count {
            result[idx - 1] = text;
        }
    }

    Ok(result)
}

/// Translate short text via AI streaming with delta callbacks.
pub async fn translate_short_text_streaming_with_source<F>(
    config: &AiConfig,
    text: &str,
    on_delta: &mut F,
) -> Result<String>
where
    F: FnMut(&str) -> Result<()>,
{
    if !ai_short_text_available(config) {
        anyhow::bail!(
            "Short-text translation is not configured. Choose a Models provider or local model in Settings."
        );
    }

    let result = translate_short_text_streaming(config, text, on_delta).await?;
    if result.trim().is_empty() {
        anyhow::bail!("AI returned empty response");
    }
    if !translation_looks_translated(&config.target_language, text, &result) {
        anyhow::bail!("AI returned untranslated result");
    }
    Ok(result)
}

/// Translate a short description with streaming delta callbacks.
pub async fn translate_short_text_streaming<F>(
    config: &AiConfig,
    text: &str,
    on_delta: &mut F,
) -> Result<String>
where
    F: FnMut(&str) -> Result<()>,
{
    let lang = language_display_name(&config.target_language);
    let source_lang = detect_short_text_source_lang(text);
    let source_lang_hint = source_lang_hint_from_code(source_lang);
    let system_prompt = build_short_translation_system_prompt(config, lang, &source_lang_hint);

    match chat_completion_stream(
        config,
        &system_prompt,
        text,
        SHORT_TEXT_MAX_TOKENS,
        on_delta,
    )
    .await
    {
        Ok(result) if !result.trim().is_empty() => Ok(result),
        Ok(_) => {
            warn!(target: "short_text", "AI streaming returned empty, retrying non-streaming");
            tokio::time::timeout(
                std::time::Duration::from_secs(20),
                translate_short_text(config, text),
            )
            .await
            .map_err(|_| anyhow::anyhow!("Short text non-streaming fallback timed out after 20s"))?
        }
        Err(stream_err) => {
            warn!(target: "short_text", error = %stream_err, "AI streaming failed, retrying non-streaming");
            tokio::time::timeout(
                std::time::Duration::from_secs(20),
                translate_short_text(config, text),
            )
            .await
            .map_err(|_| anyhow::anyhow!("Short text non-streaming fallback timed out after 20s"))?
        }
    }
}

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
            .context("Streaming summary returned empty response; non-stream fallback failed"),
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
        parse_batch_translation_response, parse_skill_pick_response,
        shortlist_skill_pick_candidates, split_translation_chunks,
    };
    #[test]
    fn split_translation_chunks_preserves_full_content() {
        let text = "## Intro\nline 1\nline 2\n\n## Next\nline 3\nline 4\n";
        let chunks = split_translation_chunks(text, 18);
        assert!(chunks.len() > 1);
        assert_eq!(chunks.concat(), text);
    }

    #[test]
    fn split_translation_chunks_no_split_for_short_input() {
        let text = "short markdown";
        let chunks = split_translation_chunks(text, 10_000);
        assert_eq!(chunks, vec![text.to_string()]);
    }

    #[test]
    fn split_translation_chunks_avoids_mid_fence_split() {
        let text = "Header\n```bash\nline-a\nline-b\nline-c\n```\nTail\n";
        let chunks = split_translation_chunks(text, 20);
        assert_eq!(chunks.concat(), text);
        assert!(chunks.iter().any(|chunk| chunk.contains("```bash\nline-a")));
        assert!(chunks.iter().any(|chunk| chunk.contains("line-c\n```")));
    }

    #[test]
    fn parse_batch_translation_response_accepts_markers_with_trailing_spaces() {
        let raw = "[1] first item\n[2] second item";
        let parsed = parse_batch_translation_response(raw, 2).expect("should parse");
        assert_eq!(
            parsed,
            vec!["first item".to_string(), "second item".to_string()]
        );
    }

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

    #[test]
    fn hy_mt_prompt_selection_only_for_local_hy_mt_models() {
        let mut cfg = AiConfig::default();
        cfg.api_format = ApiFormat::Local;
        cfg.model = "huihui_ai/hy-mt1.5-abliterated:1.8b".to_string();

        let hy_mt_prompt =
            super::build_translation_system_prompt(&cfg, "Simplified Chinese", "English (en)");
        assert!(
            hy_mt_prompt.contains("Treat the USER message as source text"),
            "HY-MT local models should use the dedicated HY-MT translation prompt"
        );

        cfg.model = "llama3.1:8b".to_string();
        let generic_prompt =
            super::build_translation_system_prompt(&cfg, "Simplified Chinese", "English (en)");
        assert!(
            !generic_prompt.contains("Treat the USER message as source text"),
            "non HY-MT models should keep the generic translation prompt"
        );
    }

    #[test]
    fn hy_mt_prompt_selection_not_enabled_for_non_local_formats() {
        let mut cfg = AiConfig::default();
        cfg.api_format = ApiFormat::Openai;
        cfg.model = "tencent/HY-MT1.5-1.8B".to_string();

        let prompt = super::build_short_translation_system_prompt(
            &cfg,
            "Simplified Chinese",
            "English (en)",
        );
        assert!(
            !prompt.contains("Treat the USER message as source text"),
            "HY-MT prompt adaptation is scoped to local format only"
        );
    }

    // (MyMemory tests removed — MyMemory support was dropped)

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
    fn load_config_does_not_derive_translation_settings_from_legacy_fields() {
        with_temp_data_root(|_dir| {
            let config_path = super::config_path();
            std::fs::create_dir_all(config_path.parent().expect("config dir"))
                .expect("create config dir");
            std::fs::write(
                &config_path,
                serde_json::json!({
                    "enabled": true,
                    "api_format": "openai",
                    "base_url": "https://api.openai.com/v1",
                    "api_key": "legacy-key",
                    "model": "gpt-5.4",
                    "target_language": "ja",
                    "short_text_priority": "ai_first",
                    "translation_api": {
                        "deeplx_url": "http://127.0.0.1:1188"
                    }
                })
                .to_string(),
            )
            .expect("write legacy config");

            super::invalidate_config_cache();
            let loaded = super::load_config();

            assert_eq!(
                loaded.translation_settings,
                crate::translation_config::TranslationSettings::default()
            );
            assert_eq!(loaded.target_language, "ja");
            assert_eq!(loaded.translation_api.deeplx_url, "http://127.0.0.1:1188");
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
                cfg.translation_api.deeplx_key = "deeplx-secret-key".to_string();
                cfg.translation_api.deeplx_url = "https://example.com/deeplx/translate".to_string();

                super::save_config_async(&cfg)
                    .await
                    .expect("save config async should succeed");
                let loaded = super::load_config_async().await;

                assert!(loaded.enabled);
                assert_eq!(loaded.base_url, cfg.base_url);
                assert_eq!(loaded.api_key, cfg.api_key);
                assert_eq!(loaded.model, cfg.model);
                assert_eq!(loaded.target_language, cfg.target_language);
                assert_eq!(
                    loaded.translation_api.deeplx_key,
                    cfg.translation_api.deeplx_key
                );
                assert_eq!(
                    loaded.translation_api.deeplx_url,
                    cfg.translation_api.deeplx_url
                );
            });
        });
    }

    #[test]
    fn translation_looks_translated_rejects_english_for_zh_cn() {
        let source = "Diagnose and fix broken adapters when websites change.";
        let result = "Diagnose and fix broken adapters when websites change.";
        assert!(
            !super::translation_looks_translated("zh-CN", source, result),
            "Should reject English result for zh-CN target"
        );
    }

    #[test]
    fn translation_looks_translated_accepts_chinese() {
        let source = "Diagnose and fix broken adapters when websites change.";
        let result = "当网站发生变化时，诊断并修复损坏的适配器。";
        assert!(
            super::translation_looks_translated("zh-CN", source, result),
            "Should accept Chinese result for zh-CN target"
        );
    }

    #[test]
    fn translation_looks_translated_accepts_mixed() {
        let source = "Use OpenCLI to automate tasks.";
        let result = "使用 OpenCLI 自动化任务。";
        assert!(
            super::translation_looks_translated("zh-CN", source, result),
            "Should accept mixed CJK+Latin result"
        );
    }

    #[test]
    fn translation_looks_translated_accepts_latin_target() {
        let source = "诊断并修复适配器。";
        let result = "Diagnose and fix adapters.";
        assert!(
            super::translation_looks_translated("en", source, result),
            "Latin-script targets always pass"
        );
    }

    #[test]
    fn translation_looks_translated_accepts_same_script() {
        let source = "这是中文描述。";
        let result = "这是中文描述。";
        assert!(
            super::translation_looks_translated("zh-CN", source, result),
            "Same-script source and target should pass"
        );
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
    fn detect_short_text_source_lang_identifies_scripts() {
        assert_eq!(super::detect_short_text_source_lang("Hello world"), "en");
        assert_eq!(super::detect_short_text_source_lang("中"), "zh-CN");
        assert_eq!(
            super::detect_short_text_source_lang("これは日本語です"),
            "ja"
        );
        assert_eq!(super::detect_short_text_source_lang("한국어 텍스트"), "ko");
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
