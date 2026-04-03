use anyhow::{Context, Result};
use serde::{Deserialize, Deserializer, Serialize};
use std::path::PathBuf;
use std::sync::{Arc, LazyLock, Mutex, MutexGuard};
use tracing::{debug, error, warn};

const AI_MAX_TOKENS: u32 = 196_608;
const SHORT_TEXT_MAX_TOKENS: u32 = 1024;
const SUMMARY_MAX_TOKENS: u32 = 4_096;
const SKILL_PICK_MAX_CANDIDATES: usize = 64;
const SKILL_PICK_LOW_SIGNAL_MAX_CANDIDATES: usize = 96;
const SKILL_PICK_MAX_RECOMMENDATIONS: usize = 12;
const SKILL_PICK_ROUND_MAX_TOKENS: u32 = 2_048;

/// Estimate a reasonable max_tokens for translation output.
/// Translation output is roughly proportional to input length.
/// Uses chars/3 as a rough token estimate, adds 2x headroom, min 1024, max 32K.
fn estimate_translation_max_tokens(input: &str) -> u32 {
    let estimated_input_tokens = (input.len() as u32) / 3;
    let estimate = (estimated_input_tokens * 2).max(1024);
    estimate.min(32_768)
}

// ── Configuration ───────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ApiFormat {
    Openai,
    Anthropic,
    Local,
}

impl ApiFormat {
    fn parse_loose(raw: &str) -> Self {
        match raw.trim().to_ascii_lowercase().as_str() {
            "openai" => Self::Openai,
            "anthropic" => Self::Anthropic,
            "local" => Self::Local,
            _ => Self::Openai,
        }
    }
}

impl Default for ApiFormat {
    fn default() -> Self {
        Self::Openai
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ShortTextPriority {
    AiFirst,
    MymemoryFirst,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ShortTextSource {
    Ai,
    Mymemory,
}

impl ShortTextSource {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Ai => "ai",
            Self::Mymemory => "mymemory",
        }
    }
}

impl ShortTextPriority {
    fn parse_loose(raw: &str) -> Self {
        match raw.trim().to_ascii_lowercase().as_str() {
            "mymemory_first" | "mymemoryfirst" | "my_memory_first" => Self::MymemoryFirst,
            _ => Self::AiFirst,
        }
    }
}

impl Default for ShortTextPriority {
    fn default() -> Self {
        Self::AiFirst
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct MymemoryUsageStats {
    #[serde(default)]
    pub total_chars_sent: u64,
    #[serde(default)]
    pub daily_chars_sent: u64,
    #[serde(default)]
    pub daily_reset_date: String,
    #[serde(default)]
    pub updated_at: String,
}

static MYMEMORY_USAGE_LOCK: LazyLock<Mutex<()>> = LazyLock::new(|| Mutex::new(()));
static AI_REQUEST_SEMAPHORE: LazyLock<Mutex<Option<(u32, Arc<tokio::sync::Semaphore>)>>> =
    LazyLock::new(|| Mutex::new(None));

// ── AiConfig In-Memory Cache ────────────────────────────────────────
//
// Avoids repeated disk reads + AES-256-GCM decryption on every AI command.
// TTL = 5 seconds; invalidated immediately on save_config.

const AI_CONFIG_CACHE_TTL: std::time::Duration = std::time::Duration::from_secs(5);

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

fn deserialize_api_format<'de, D>(deserializer: D) -> Result<ApiFormat, D::Error>
where
    D: Deserializer<'de>,
{
    let raw = String::deserialize(deserializer)?;
    Ok(ApiFormat::parse_loose(&raw))
}

fn deserialize_short_text_priority<'de, D>(deserializer: D) -> Result<ShortTextPriority, D::Error>
where
    D: Deserializer<'de>,
{
    let raw = String::deserialize(deserializer)?;
    Ok(ShortTextPriority::parse_loose(&raw))
}

/// Per-format saved preset (base_url, api_key, model).
/// When the user switches api_format, the active fields are swapped from/to
/// the corresponding preset so each format remembers its own values.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct FormatPreset {
    #[serde(default)]
    pub base_url: String,
    #[serde(default)]
    pub api_key: String,
    #[serde(default)]
    pub model: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AiConfig {
    pub enabled: bool,
    #[serde(default, deserialize_with = "deserialize_api_format")]
    pub api_format: ApiFormat,
    pub base_url: String,
    pub api_key: String,
    pub model: String,
    pub target_language: String,
    #[serde(default, deserialize_with = "deserialize_short_text_priority")]
    pub short_text_priority: ShortTextPriority,
    /// Model context window in K tokens (e.g. 128 = 128K tokens).
    /// All scan parameters are auto-derived from this value.
    #[serde(default = "default_context_window_k")]
    pub context_window_k: u32,
    /// Override: 0 = auto-derive from context_window_k
    #[serde(default)]
    pub max_concurrent_requests: u32,
    /// Override: 0 = auto-derive from context_window_k
    #[serde(default)]
    pub chunk_char_limit: usize,
    /// Override: 0 = auto-derive from context_window_k
    #[serde(default)]
    pub scan_max_response_tokens: u32,
    /// Optional anonymous telemetry for security scan quality metrics.
    /// When enabled, SkillStar only records aggregate run stats (no skill names/content).
    #[serde(default = "default_security_scan_telemetry_enabled")]
    pub security_scan_telemetry_enabled: bool,
    /// Saved preset for OpenAI-compatible format.
    #[serde(default)]
    pub openai_preset: FormatPreset,
    /// Saved preset for Anthropic Messages format.
    #[serde(default)]
    pub anthropic_preset: FormatPreset,
    /// Saved preset for Local (Ollama) format.
    #[serde(default)]
    pub local_preset: FormatPreset,
}

fn default_context_window_k() -> u32 {
    128
}

fn default_security_scan_telemetry_enabled() -> bool {
    false
}

impl Default for AiConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            api_format: ApiFormat::default(),
            base_url: String::new(),
            api_key: String::new(),
            model: "gpt-5.4".to_string(),
            target_language: "zh-CN".to_string(),
            short_text_priority: ShortTextPriority::default(),
            context_window_k: default_context_window_k(),
            max_concurrent_requests: 4,
            chunk_char_limit: 0,
            scan_max_response_tokens: 0,
            security_scan_telemetry_enabled: default_security_scan_telemetry_enabled(),
            openai_preset: FormatPreset::default(),
            anthropic_preset: FormatPreset::default(),
            local_preset: FormatPreset {
                base_url: "http://127.0.0.1:11434/v1".to_string(),
                model: "llama3.1:8b".to_string(),
                ..Default::default()
            },
        }
    }
}

// ── Auto-derived scan parameters ────────────────────────────────────

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



fn config_path() -> PathBuf {
    super::paths::ai_config_path()
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

pub async fn save_config_async(config: &AiConfig) -> Result<()> {
    let config = config.clone();
    tokio::task::spawn_blocking(move || save_config(&config))
        .await
        .map_err(|err| anyhow::anyhow!("save_config task failed: {}", err))?
}

// ── Language Mapping ────────────────────────────────────────────────

fn language_display_name(code: &str) -> &str {
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

const TRANSLATION_CHUNK_SOFT_LIMIT_CHARS: usize = 10_000;
const TRANSLATION_CHUNK_RETRY_MIN_CHARS: usize = 4_000;

// ── Prompts ─────────────────────────────────────────────────────────

const TRANSLATE_DOCUMENT_PROMPT: &str = include_str!("../../../prompts/ai/translate_document.md");
const TRANSLATE_DOCUMENT_PROMPT_HY_MT: &str =
    include_str!("../../../prompts/ai/translate_document_hy_mt.md");
const TRANSLATE_SHORT_PROMPT: &str = include_str!("../../../prompts/ai/translate_short.md");
const TRANSLATE_SHORT_PROMPT_HY_MT: &str =
    include_str!("../../../prompts/ai/translate_short_hy_mt.md");
const SUMMARY_PROMPT: &str = include_str!("../../../prompts/ai/summary.md");
const TRANSLATE_CHUNK_PROMPT: &str = include_str!("../../../prompts/ai/translate_chunk.md");
const PICK_SKILLS_PROMPT: &str = include_str!("../../../prompts/ai/pick_skills.md");
const MARKETPLACE_SEARCH_PROMPT: &str = include_str!("../../../prompts/ai/marketplace_search.md");

const MARKETPLACE_SEARCH_MAX_TOKENS: u32 = 256;

fn is_empty_ai_response_error(err: &anyhow::Error) -> bool {
    err.to_string().contains("AI returned empty response")
}

fn is_hy_mt_model(config: &AiConfig) -> bool {
    is_local_format(config)
        && config
            .model
            .trim()
            .to_ascii_lowercase()
            .contains("hy-mt")
}

fn build_translation_system_prompt(config: &AiConfig, lang: &str, source_lang_hint: &str) -> String {
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

fn build_skill_pick_system_prompt(skill_catalog: &str) -> String {
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

async fn translate_text_in_chunks(
    config: &AiConfig,
    base_system_prompt: &str,
    text: &str,
) -> Result<String> {
    let chunks = split_translation_chunks(text, TRANSLATION_CHUNK_SOFT_LIMIT_CHARS);
    if chunks.len() <= 1 {
        return chat_completion_capped(
            config,
            base_system_prompt,
            text,
            estimate_translation_max_tokens(text),
        )
        .await;
    }

    let total = chunks.len();
    let max_parallel = resolve_scan_params(config)
        .max_concurrent_requests
        .clamp(1, 8) as usize;
    let semaphore = std::sync::Arc::new(tokio::sync::Semaphore::new(max_parallel));
    let mut tasks = tokio::task::JoinSet::new();

    for (index, chunk) in chunks.iter().cloned().enumerate() {
        let cfg = config.clone();
        let prompt = build_translation_chunk_prompt(base_system_prompt, index + 1, total);
        let permit_pool = semaphore.clone();
        tasks.spawn(async move {
            let _permit = permit_pool
                .acquire_owned()
                .await
                .map_err(|_| anyhow::anyhow!("Chunk translation semaphore closed"))?;

            let chunk_number = index + 1;
            let chunk_result = chat_completion_capped(
                &cfg,
                &prompt,
                &chunk,
                estimate_translation_max_tokens(&chunk),
            )
            .await
            .with_context(|| format!("Failed to translate chunk {chunk_number}/{total}"))?;

            if chunk_result.trim().is_empty() {
                anyhow::bail!("AI returned empty response for chunk {chunk_number}/{total}");
            }

            Ok::<(usize, bool, String), anyhow::Error>((index, chunk.ends_with('\n'), chunk_result))
        });
    }

    let mut ordered_results: Vec<Option<(bool, String)>> = vec![None; total];
    while let Some(joined) = tasks.join_next().await {
        let (index, ends_with_newline, chunk_result) =
            joined.map_err(|e| anyhow::anyhow!("Chunk translation task panicked: {}", e))??;
        ordered_results[index] = Some((ends_with_newline, chunk_result));
    }

    let mut translated = String::new();
    for (index, item) in ordered_results.into_iter().enumerate() {
        let (ends_with_newline, chunk_result) = item.ok_or_else(|| {
            anyhow::anyhow!(
                "Missing translation result for chunk {}/{}",
                index + 1,
                total
            )
        })?;
        translated.push_str(&chunk_result);
        if ends_with_newline && !chunk_result.ends_with('\n') {
            translated.push('\n');
        }
    }

    Ok(translated)
}

// ── HTTP Client Builder ─────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq)]
struct ProxyFingerprint {
    enabled: bool,
    scheme: String,
    host: String,
    port: u16,
    username: String,
    password: String,
}

impl ProxyFingerprint {
    fn from_config(config: &super::proxy::ProxyConfig) -> Self {
        Self {
            enabled: config.enabled && !config.host.trim().is_empty(),
            scheme: config.proxy_type.as_scheme().to_string(),
            host: config.host.trim().to_string(),
            port: config.port,
            username: config
                .username
                .as_deref()
                .unwrap_or_default()
                .trim()
                .to_string(),
            password: config.password.clone().unwrap_or_default(),
        }
    }
}

static SHARED_HTTP_CLIENT: LazyLock<Mutex<Option<(ProxyFingerprint, reqwest::Client)>>> =
    LazyLock::new(|| Mutex::new(None));

fn current_proxy_fingerprint() -> ProxyFingerprint {
    match super::proxy::load_config() {
        Ok(config) => ProxyFingerprint::from_config(&config),
        Err(_) => ProxyFingerprint {
            enabled: false,
            scheme: "http".to_string(),
            host: String::new(),
            port: 7897,
            username: String::new(),
            password: String::new(),
        },
    }
}

/// Build a reqwest client, optionally honouring the user's proxy config.
fn build_http_client_inner(fingerprint: &ProxyFingerprint) -> Result<reqwest::Client> {
    let mut builder = reqwest::Client::builder()
        // Total request timeout (covers entire request lifecycle).
        .timeout(std::time::Duration::from_secs(120))
        // Fast-fail on network-unreachable / DNS-timeout scenarios
        // instead of waiting the full 120s.
        .connect_timeout(std::time::Duration::from_secs(10))
        // Keep idle connections alive to reuse TLS sessions.
        .pool_idle_timeout(std::time::Duration::from_secs(90))
        // Match the AI concurrency budget default.
        .pool_max_idle_per_host(4);

    if fingerprint.enabled {
        let proxy_url = format!(
            "{}://{}:{}",
            fingerprint.scheme, fingerprint.host, fingerprint.port
        );

        let mut proxy = reqwest::Proxy::all(&proxy_url).context("Invalid proxy URL")?;
        if !fingerprint.username.is_empty() {
            proxy = proxy.basic_auth(&fingerprint.username, &fingerprint.password);
        }

        builder = builder.proxy(proxy);
    }

    builder.build().context("Failed to build HTTP client")
}

/// Get or lazily create the shared HTTP client.  Reuses TLS sessions and
/// HTTP/2 connections between requests — eliminates ~100-200ms per request.
/// The cache auto-refreshes when proxy settings change.
fn get_http_client() -> Result<reqwest::Client> {
    let fingerprint = current_proxy_fingerprint();
    let mut guard = SHARED_HTTP_CLIENT
        .lock()
        .map_err(|_| anyhow::anyhow!("HTTP client cache lock poisoned"))?;

    if let Some((cached_fp, client)) = guard.as_ref() {
        if *cached_fp == fingerprint {
            return Ok(client.clone());
        }
    }

    let rebuilt = build_http_client_inner(&fingerprint)
        .with_context(|| "Failed to build HTTP client with current proxy settings")?;
    *guard = Some((fingerprint, rebuilt.clone()));
    Ok(rebuilt)
}

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
        .header("Authorization", format!("Bearer {}", effective_api_key(config)))
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
        .header("Authorization", format!("Bearer {}", effective_api_key(config)))
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

async fn translate_text_in_chunks_streaming<F>(
    config: &AiConfig,
    base_system_prompt: &str,
    text: &str,
    on_delta: &mut F,
) -> Result<String>
where
    F: FnMut(&str) -> Result<()>,
{
    let chunks = split_translation_chunks(text, TRANSLATION_CHUNK_SOFT_LIMIT_CHARS);
    if chunks.len() <= 1 {
        return chat_completion_stream(
            config,
            base_system_prompt,
            text,
            estimate_translation_max_tokens(text),
            on_delta,
        )
        .await;
    }

    let total = chunks.len();
    let mut translated = String::new();

    for (index, chunk) in chunks.iter().enumerate() {
        let chunk_number = index + 1;
        let chunk_prompt = build_translation_chunk_prompt(base_system_prompt, chunk_number, total);
        let chunk_result = chat_completion_stream(
            config,
            &chunk_prompt,
            chunk,
            estimate_translation_max_tokens(chunk),
            on_delta,
        )
        .await
        .with_context(|| format!("Failed to stream-translate chunk {chunk_number}/{total}"))?;

        if chunk_result.trim().is_empty() {
            anyhow::bail!("AI returned empty response for chunk {chunk_number}/{total}");
        }

        translated.push_str(&chunk_result);
        if chunk.ends_with('\n') && !chunk_result.ends_with('\n') {
            translated.push('\n');
            on_delta("\n")?;
        }
    }

    Ok(translated)
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

pub(crate) async fn chat_completion(
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
pub(crate) async fn chat_completion_capped(
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
async fn chat_completion_deterministic(
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
    config.enabled && (!config.api_key.trim().is_empty() || is_local_format(config))
}

fn normalize_mymemory_lang(code: &str) -> String {
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

/// Return a persistent `de` email for MyMemory API usage.
///
/// MyMemory gives anonymous users 5000 words/day. By sending a stable `de`
/// parameter the quota is tracked per-email instead of per-IP, which is more
/// reliable for desktop apps behind NATs. The email is generated once and
/// stored at `~/.skillstar/.mymemory_de`.
fn get_mymemory_de() -> String {
    use std::fs;
    let path = crate::core::paths::mymemory_disabled_path();
    if let Ok(email) = fs::read_to_string(&path) {
        let trimmed = email.trim().to_string();
        if !trimmed.is_empty() {
            return trimmed;
        }
    }
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    let id = uuid::Uuid::new_v4();
    let email = format!("{id}@skillstar.local");
    let _ = fs::write(&path, &email);
    email
}

fn mymemory_usage_path() -> PathBuf {
    crate::core::paths::mymemory_usage_path()
}

fn load_mymemory_usage_stats_inner() -> MymemoryUsageStats {
    let path = mymemory_usage_path();
    std::fs::read_to_string(path)
        .ok()
        .and_then(|raw| serde_json::from_str::<MymemoryUsageStats>(&raw).ok())
        .unwrap_or_default()
}

fn save_mymemory_usage_stats_inner(stats: &MymemoryUsageStats) {
    let path = mymemory_usage_path();
    if let Some(parent) = path.parent() {
        if std::fs::create_dir_all(parent).is_err() {
            return;
        }
    }
    if let Ok(raw) = serde_json::to_string_pretty(stats) {
        let _ = std::fs::write(path, raw);
    }
}

fn lock_mymemory_usage() -> MutexGuard<'static, ()> {
    match MYMEMORY_USAGE_LOCK.lock() {
        Ok(guard) => guard,
        Err(poisoned) => {
            warn!(target: "mymemory", "usage lock poisoned, continuing with recovered state");
            poisoned.into_inner()
        }
    }
}

fn current_daily_reset_date() -> String {
    chrono::Local::now().date_naive().to_string()
}

fn normalize_mymemory_daily_stats(stats: &mut MymemoryUsageStats) -> bool {
    let today = current_daily_reset_date();
    if stats.daily_reset_date == today {
        return false;
    }
    stats.daily_reset_date = today;
    stats.daily_chars_sent = 0;
    true
}

fn record_mymemory_sent_chars(chars_sent: usize) {
    if chars_sent == 0 {
        return;
    }
    let _guard = lock_mymemory_usage();
    let mut stats = load_mymemory_usage_stats_inner();
    normalize_mymemory_daily_stats(&mut stats);
    stats.total_chars_sent = stats.total_chars_sent.saturating_add(chars_sent as u64);
    stats.daily_chars_sent = stats.daily_chars_sent.saturating_add(chars_sent as u64);
    stats.updated_at = chrono::Utc::now().to_rfc3339();
    save_mymemory_usage_stats_inner(&stats);
}

#[must_use]
pub fn get_mymemory_usage_stats() -> MymemoryUsageStats {
    let _guard = lock_mymemory_usage();
    let mut stats = load_mymemory_usage_stats_inner();
    if normalize_mymemory_daily_stats(&mut stats) {
        save_mymemory_usage_stats_inner(&stats);
    }
    stats
}

async fn get_mymemory_de_async() -> String {
    tokio::task::spawn_blocking(get_mymemory_de)
        .await
        .unwrap_or_else(|err| {
            error!(target: "mymemory", error = %err, "get_mymemory_de task failed");
            String::new()
        })
}

async fn record_mymemory_sent_chars_async(chars_sent: usize) {
    if chars_sent == 0 {
        return;
    }
    if let Err(err) =
        tokio::task::spawn_blocking(move || record_mymemory_sent_chars(chars_sent)).await
    {
        error!(target: "mymemory", error = %err, "record_mymemory_sent_chars task failed");
    }
}

#[must_use]
pub async fn get_mymemory_usage_stats_async() -> MymemoryUsageStats {
    tokio::task::spawn_blocking(get_mymemory_usage_stats)
        .await
        .unwrap_or_else(|err| {
            error!(target: "mymemory", error = %err, "get_mymemory_usage_stats task failed");
            MymemoryUsageStats::default()
        })
}

async fn mymemory_call(
    client: &reqwest::Client,
    text: &str,
    langpair: &str,
    de: Option<&str>,
) -> Result<String> {
    record_mymemory_sent_chars_async(text.chars().count()).await;
    let mut params: Vec<(&str, &str)> = vec![("q", text), ("langpair", langpair)];
    if let Some(email) = de {
        params.push(("de", email));
    }
    let url = reqwest::Url::parse_with_params("https://api.mymemory.translated.net/get", &params)
        .context("Failed to build MyMemory URL")?;

    let resp = client
        .get(url)
        .send()
        .await
        .context("Failed to send request to MyMemory API")?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body_text = resp.text().await.unwrap_or_default();
        anyhow::bail!("MyMemory API returned {} — {}", status, body_text);
    }

    let payload = resp
        .json::<serde_json::Value>()
        .await
        .context("Failed to parse MyMemory API response")?;

    let api_status = payload
        .get("responseStatus")
        .and_then(|v| v.as_i64())
        .unwrap_or(200);
    if api_status != 200 {
        let details = payload
            .get("responseDetails")
            .and_then(|v| v.as_str())
            .unwrap_or("Unknown MyMemory error");
        anyhow::bail!("MyMemory API responseStatus={} — {}", api_status, details);
    }

    let translated = payload
        .pointer("/responseData/translatedText")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .unwrap_or("");

    if translated.is_empty() {
        anyhow::bail!("MyMemory returned empty translation");
    }

    if translated.contains("PLEASE SELECT TWO DISTINCT LANGUAGES")
        || translated.contains("MYMEMORY WARNING:")
        || translated.contains("LIMIT EXCEEDED")
    {
        anyhow::bail!("MyMemory returned API warning: {}", translated);
    }

    Ok(translated.to_string())
}

async fn mymemory_translate_short_text(config: &AiConfig, text: &str) -> Result<String> {
    if text.trim().is_empty() {
        return Ok(String::new());
    }

    let source_lang = detect_short_text_source_lang(text);
    let normalized_source = normalize_mymemory_lang(source_lang);
    let target = normalize_mymemory_lang(&config.target_language);
    debug!(
        target: "translate",
        src = %source_lang,
        pair = %format!("{normalized_source}|{target}"),
        text_len = text.len(),
        "mymemory ENTER"
    );

    if normalized_source == target {
        debug!(target: "translate", "mymemory SKIP same-language");
        return Ok(text.to_string());
    }

    let client = get_http_client()?;
    let langpair = format!("{}|{}", normalized_source, target);
    let de = get_mymemory_de_async().await;
    let de_param = (!de.trim().is_empty()).then_some(de);

    // Fast path: skip Markdown↔HTML round-trip for plain text (no formatting)
    let is_plain = !text.contains(['#', '*', '`', '[', '|', '>', '~']);

    let api_input = if is_plain {
        text.to_string()
    } else {
        // Parse Markdown to HTML so MyMemory properly preserves the structural tags
        let parser = pulldown_cmark::Parser::new(text);
        let mut html_input = String::new();
        pulldown_cmark::html::push_html(&mut html_input, parser);
        html_input
    };

    // Try with de (per-email quota)
    let raw_result =
        match mymemory_call(&client, &api_input, &langpair, de_param.as_deref()).await {
            Ok(result) => result,
            Err(e) => {
                warn!(target: "mymemory", error = %e, "de request failed, retrying anonymous");
                // Fallback: anonymous (no de, per-IP quota)
                mymemory_call(&client, &api_input, &langpair, None).await?
            }
        };

    // Convert back to Markdown only if we sent HTML
    let output = if is_plain {
        raw_result.trim().to_string()
    } else {
        html2md::parse_html(&raw_result).trim().to_string()
    };
    Ok(output)
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

    for cap in re.find_iter(response) {
        if let Some(prev_idx) = last_idx {
            let text = response[last_start..cap.start()].trim().to_string();
            items.push((prev_idx, text));
        }
        // Extract the number from [N]
        let num_str = &response[cap.start() + 1..cap.end() - 1].trim();
        if let Ok(num) = num_str.parse::<usize>() {
            last_idx = Some(num);
            last_start = cap.end();
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

/// Translate short text with provider-priority policy while supporting
/// streaming deltas when the active path uses AI.
pub async fn translate_short_text_streaming_with_priority_source<F>(
    config: &AiConfig,
    text: &str,
    on_delta: &mut F,
) -> Result<(String, ShortTextSource)>
where
    F: FnMut(&str) -> Result<()>,
{
    let ai_available = ai_short_text_available(config);
    let mymemory_available = true;

    if !ai_available && !mymemory_available {
        anyhow::bail!(
            "Short-text translation is not configured. Configure AI API key in Settings."
        );
    }

    match config.short_text_priority {
        ShortTextPriority::AiFirst => {
            debug!(target: "translate", priority = "AiFirst", ai_available, "short text priority");
            let mut ai_err: Option<anyhow::Error> = None;
            if ai_available {
                match translate_short_text_streaming(config, text, on_delta).await {
                    Ok(result) if !result.trim().is_empty() => {
                        return Ok((result, ShortTextSource::Ai));
                    }
                    Ok(_) => {
                        warn!(
                            target: "short_text",
                            "AI returned empty response, falling back to MyMemory"
                        );
                        ai_err = Some(anyhow::anyhow!("AI returned empty response"));
                    }
                    Err(err) => {
                        warn!(
                            target: "short_text",
                            error = %err,
                            "AI translation failed, falling back to MyMemory"
                        );
                        ai_err = Some(err);
                    }
                }
            }

            if mymemory_available {
                let context = match ai_err {
                    Some(err) => format!(
                        "MyMemory short-text translation failed after AI path failed: {}",
                        err
                    ),
                    None => "MyMemory short-text translation failed".to_string(),
                };
                let result = mymemory_translate_short_text(config, text)
                    .await
                    .with_context(|| context)?;
                return Ok((result, ShortTextSource::Mymemory));
            }

            if let Some(err) = ai_err {
                return Err(err);
            }
        }
        ShortTextPriority::MymemoryFirst => {
            debug!(target: "translate", priority = "MymemoryFirst", ai_available, "short text priority");
            let mut mymemory_err: Option<anyhow::Error> = None;
            if mymemory_available {
                match mymemory_translate_short_text(config, text).await {
                    Ok(result) if !result.trim().is_empty() => {
                        return Ok((result, ShortTextSource::Mymemory));
                    }
                    Ok(_) => {
                        warn!(
                            target: "short_text",
                            "MyMemory returned empty response, falling back to AI"
                        );
                        mymemory_err = Some(anyhow::anyhow!("MyMemory returned empty response"));
                    }
                    Err(err) => {
                        warn!(
                            target: "short_text",
                            error = %err,
                            "MyMemory translation failed, falling back to AI"
                        );
                        mymemory_err = Some(err);
                    }
                }
            }

            if ai_available {
                let context = match mymemory_err {
                    Some(err) => format!(
                        "AI short-text translation failed after MyMemory path failed: {}",
                        err
                    ),
                    None => "AI short-text translation failed".to_string(),
                };
                let result = translate_short_text_streaming(config, text, on_delta)
                    .await
                    .with_context(|| context)?;
                return Ok((result, ShortTextSource::Ai));
            }

            if let Some(err) = mymemory_err {
                return Err(err);
            }
        }
    }

    anyhow::bail!("Short-text translation is not configured. Configure AI API key in Settings.");
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
            translate_short_text(config, text).await
        }
        Err(stream_err) => {
            warn!(target: "short_text", error = %stream_err, "AI streaming failed, retrying non-streaming");
            translate_short_text(config, text).await
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillPickCandidate {
    pub name: String,
    pub description: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillPickRecommendation {
    pub name: String,
    pub score: u8,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillPickResponse {
    pub recommendations: Vec<SkillPickRecommendation>,
    pub fallback_used: bool,
    pub rounds_succeeded: usize,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct SkillPickCatalogEntry {
    name: String,
    description: String,
    local_score: u8,
}

#[derive(Debug, Clone)]
struct RankedSkillPickCandidate {
    name: String,
    description: String,
    local_score: u8,
}

#[derive(Debug, Clone)]
struct SkillPickRoundRecommendation {
    name: String,
    score: u8,
    reason: String,
    rank: usize,
}

#[derive(Debug, Default)]
struct AggregatedSkillPick {
    votes: usize,
    score_sum: u32,
    best_rank: usize,
    local_score: u8,
    reason: String,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum ParsedSkillPickEnvelope {
    Array(Vec<ParsedSkillPickItem>),
    Wrapped {
        recommendations: Vec<ParsedSkillPickItem>,
    },
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum ParsedSkillPickItem {
    Name(String),
    Rich {
        name: String,
        #[serde(default)]
        score: Option<u8>,
        #[serde(default)]
        reason: Option<String>,
    },
}

fn is_low_signal_match_token(token: &str) -> bool {
    matches!(
        token,
        "a" | "an"
            | "and"
            | "app"
            | "apps"
            | "assistant"
            | "build"
            | "for"
            | "from"
            | "help"
            | "in"
            | "into"
            | "of"
            | "on"
            | "or"
            | "project"
            | "skill"
            | "skills"
            | "system"
            | "the"
            | "to"
            | "tool"
            | "tools"
            | "use"
            | "using"
            | "with"
            | "workflow"
            | "workflows"
            | "ai"
    )
}

fn push_match_token_variant(
    tokens: &mut Vec<String>,
    seen: &mut std::collections::HashSet<String>,
    raw_token: &str,
) {
    let token = raw_token
        .trim_matches(|c: char| matches!(c, '.' | '-' | '_' | '/'))
        .trim();
    if token.len() < 2 || is_low_signal_match_token(token) {
        return;
    }

    let owned = token.to_string();
    if seen.insert(owned.clone()) {
        tokens.push(owned.clone());
    }

    for part in token.split(['.', '-', '_', '/']) {
        let part = part.trim();
        if part.len() < 2 || is_low_signal_match_token(part) {
            continue;
        }
        let owned = part.to_string();
        if seen.insert(owned.clone()) {
            tokens.push(owned);
        }
    }
}

fn extract_match_tokens(text: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut seen = std::collections::HashSet::new();
    let mut current = String::new();

    for ch in text.to_lowercase().chars() {
        if ch.is_ascii_alphanumeric() || matches!(ch, '+' | '#' | '.' | '-' | '_' | '/') {
            current.push(ch);
            continue;
        }

        if !current.is_empty() {
            push_match_token_variant(&mut tokens, &mut seen, &current);
            current.clear();
        }
    }

    if !current.is_empty() {
        push_match_token_variant(&mut tokens, &mut seen, &current);
    }

    tokens
}

fn compute_local_skill_pick_score(
    prompt_lower: &str,
    prompt_tokens: &std::collections::HashSet<String>,
    skill: &SkillPickCandidate,
) -> u8 {
    let skill_name_lower = skill.name.to_lowercase();
    let name_tokens = extract_match_tokens(&skill.name);
    let description_tokens = extract_match_tokens(&skill.description);
    let mut score = 0u32;
    let mut name_hits = 0u32;

    if !skill_name_lower.is_empty() && prompt_lower.contains(&skill_name_lower) {
        score += 70;
        name_hits += 1;
    }

    for token in &name_tokens {
        if prompt_tokens.contains(token) {
            name_hits += 1;
            score += 18 + (token.len() as u32).min(10) * 2;
        }
    }

    for token in description_tokens.iter().take(24) {
        if prompt_tokens.contains(token) {
            score += 6 + (token.len() as u32).min(8);
        }
    }

    if name_hits >= 2 {
        score += 12;
    }

    if !name_tokens.is_empty()
        && name_tokens
            .iter()
            .all(|token| prompt_tokens.contains(token))
    {
        score += 10;
    }

    score.min(100) as u8
}

fn shortlist_skill_pick_candidates(
    prompt: &str,
    skills: Vec<SkillPickCandidate>,
) -> Vec<RankedSkillPickCandidate> {
    let prompt_lower = prompt.to_lowercase();
    let prompt_tokens: std::collections::HashSet<String> =
        extract_match_tokens(prompt).into_iter().collect();

    let mut ranked: Vec<RankedSkillPickCandidate> = skills
        .into_iter()
        .map(|skill| RankedSkillPickCandidate {
            local_score: compute_local_skill_pick_score(&prompt_lower, &prompt_tokens, &skill),
            name: skill.name,
            description: skill.description,
        })
        .collect();

    ranked.sort_by(|a, b| {
        b.local_score
            .cmp(&a.local_score)
            .then_with(|| a.name.cmp(&b.name))
    });

    if ranked.len() <= SKILL_PICK_MAX_CANDIDATES {
        return ranked;
    }

    let top_score = ranked.first().map(|skill| skill.local_score).unwrap_or(0);
    if top_score == 0 {
        ranked.sort_by(|a, b| a.name.cmp(&b.name));
        ranked.truncate(SKILL_PICK_LOW_SIGNAL_MAX_CANDIDATES.min(ranked.len()));
        return ranked;
    }

    ranked.truncate(SKILL_PICK_MAX_CANDIDATES);
    ranked
}

fn extract_json_payload(raw: &str) -> &str {
    let trimmed = raw.trim();
    let array_start = trimmed.find('[');
    let object_start = trimmed.find('{');

    match (array_start, object_start) {
        (Some(array_idx), Some(object_idx)) if object_idx < array_idx => trimmed
            .rfind('}')
            .map(|end| &trimmed[object_idx..=end])
            .unwrap_or(trimmed),
        (Some(array_idx), _) => trimmed
            .rfind(']')
            .map(|end| &trimmed[array_idx..=end])
            .unwrap_or(trimmed),
        (_, Some(object_idx)) => trimmed
            .rfind('}')
            .map(|end| &trimmed[object_idx..=end])
            .unwrap_or(trimmed),
        _ => trimmed,
    }
}

fn default_skill_pick_score(rank: usize) -> u8 {
    std::cmp::max(80u8.saturating_sub((rank as u8).saturating_mul(6)), 55)
}

fn fallback_skill_pick_rank_score(rank: usize) -> u8 {
    std::cmp::max(82u8.saturating_sub((rank as u8).saturating_mul(4)), 40)
}

fn normalize_skill_pick_reason(reason: &str) -> String {
    reason.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn parse_skill_pick_response(
    raw: &str,
    valid_names: &std::collections::HashSet<String>,
) -> Result<Vec<SkillPickRoundRecommendation>> {
    let json_str = extract_json_payload(raw);
    let envelope: ParsedSkillPickEnvelope = serde_json::from_str(json_str).with_context(|| {
        format!(
            "Failed to parse AI skill-pick response as structured JSON: {}",
            json_str
        )
    })?;

    let items = match envelope {
        ParsedSkillPickEnvelope::Array(items) => items,
        ParsedSkillPickEnvelope::Wrapped { recommendations } => recommendations,
    };

    let mut seen = std::collections::HashSet::new();
    let mut parsed = Vec::new();

    for (rank, item) in items.into_iter().enumerate() {
        let (name, score, reason) = match item {
            ParsedSkillPickItem::Name(name) => {
                (name, default_skill_pick_score(rank), String::new())
            }
            ParsedSkillPickItem::Rich {
                name,
                score,
                reason,
            } => (
                name,
                score
                    .unwrap_or_else(|| default_skill_pick_score(rank))
                    .clamp(0, 100),
                reason.unwrap_or_default(),
            ),
        };

        if !valid_names.contains(&name) || !seen.insert(name.clone()) {
            continue;
        }

        parsed.push(SkillPickRoundRecommendation {
            name,
            score,
            reason: normalize_skill_pick_reason(&reason),
            rank,
        });
    }

    Ok(parsed)
}

fn fallback_skill_pick(ranked: &[RankedSkillPickCandidate]) -> Vec<SkillPickRecommendation> {
    let mut recommendations: Vec<SkillPickRecommendation> = ranked
        .iter()
        .filter(|skill| skill.local_score > 0)
        .take(SKILL_PICK_MAX_RECOMMENDATIONS)
        .enumerate()
        .map(|(rank, skill)| SkillPickRecommendation {
            name: skill.name.clone(),
            score: fallback_skill_pick_rank_score(rank).max(skill.local_score),
            reason: String::new(),
        })
        .collect();

    if recommendations.is_empty() {
        recommendations = ranked
            .iter()
            .take(std::cmp::min(SKILL_PICK_MAX_RECOMMENDATIONS, 6))
            .enumerate()
            .map(|(rank, skill)| SkillPickRecommendation {
                name: skill.name.clone(),
                score: fallback_skill_pick_rank_score(rank),
                reason: String::new(),
            })
            .collect();
    }

    recommendations
}

/// Pick the most relevant skills from installed skills based on a user-provided project description.
/// The picker first applies a deterministic local shortlist, then runs a 3-round AI consensus pass,
/// and finally falls back to the deterministic shortlist if the AI output is partial or invalid.
pub async fn pick_skills(
    config: &AiConfig,
    prompt: &str,
    skills: Vec<SkillPickCandidate>,
) -> Result<SkillPickResponse> {
    if skills.is_empty() {
        return Ok(SkillPickResponse {
            recommendations: Vec::new(),
            fallback_used: false,
            rounds_succeeded: 0,
        });
    }

    let ranked_candidates = shortlist_skill_pick_candidates(prompt, skills);
    let valid_names: std::collections::HashSet<String> = ranked_candidates
        .iter()
        .map(|skill| skill.name.clone())
        .collect();
    let skill_catalog = serde_json::to_string_pretty(
        &ranked_candidates
            .iter()
            .map(|skill| SkillPickCatalogEntry {
                name: skill.name.clone(),
                description: skill.description.clone(),
                local_score: skill.local_score,
            })
            .collect::<Vec<_>>(),
    )
    .context("Failed to serialize skill-pick catalog")?;
    let system_prompt = build_skill_pick_system_prompt(&skill_catalog);

    let seeds = [42u64, 123, 7];
    let mut handles = Vec::new();

    for &seed in &seeds {
        let cfg = config.clone();
        let sp = system_prompt.clone();
        let user_prompt = prompt.to_string();
        handles.push(tokio::spawn(async move {
            chat_completion_deterministic(
                &cfg,
                &sp,
                &user_prompt,
                Some(seed),
                SKILL_PICK_ROUND_MAX_TOKENS,
            )
            .await
        }));
    }

    let local_score_lookup: std::collections::HashMap<&str, u8> = ranked_candidates
        .iter()
        .map(|skill| (skill.name.as_str(), skill.local_score))
        .collect();
    let mut aggregated: std::collections::HashMap<String, AggregatedSkillPick> =
        std::collections::HashMap::new();
    let mut raw_success_count = 0usize;
    let mut parse_success_count = 0usize;

    for handle in handles {
        let result = handle
            .await
            .map_err(|e| anyhow::anyhow!("Skill-pick task panicked: {}", e))?;

        match result {
            Ok(raw) => {
                raw_success_count += 1;
                match parse_skill_pick_response(&raw, &valid_names) {
                    Ok(round_recommendations) => {
                        parse_success_count += 1;
                        for recommendation in round_recommendations {
                            let entry = aggregated
                                .entry(recommendation.name.clone())
                                .or_insert_with(|| AggregatedSkillPick {
                                    best_rank: recommendation.rank,
                                    local_score: *local_score_lookup
                                        .get(recommendation.name.as_str())
                                        .unwrap_or(&0),
                                    ..Default::default()
                                });

                            entry.votes += 1;
                            entry.score_sum += recommendation.score as u32;
                            entry.best_rank = entry.best_rank.min(recommendation.rank);
                            if entry.reason.is_empty() && !recommendation.reason.is_empty() {
                                entry.reason = recommendation.reason.clone();
                            }
                        }
                    }
                    Err(err) => {
                        warn!(target: "ai_pick_skills", error = %err, "failed to parse round response");
                    }
                }
            }
            Err(err) => {
                warn!(target: "ai_pick_skills", error = %err, "round failed");
            }
        }
    }

    if raw_success_count == 0 {
        anyhow::bail!("All 3 AI skill-pick rounds failed. Please check your AI provider settings.");
    }

    let threshold = if parse_success_count >= 2 { 2 } else { 1 };
    let mut recommendations: Vec<SkillPickRecommendation> = aggregated
        .into_iter()
        .filter(|(_, aggregate)| aggregate.votes >= threshold)
        .map(|(name, aggregate)| {
            let average_score = (aggregate.score_sum / aggregate.votes as u32) as u8;
            SkillPickRecommendation {
                name,
                score: average_score.max(aggregate.local_score),
                reason: aggregate.reason,
            }
        })
        .collect();

    recommendations.sort_by(|a, b| {
        let left = local_score_lookup
            .get(a.name.as_str())
            .copied()
            .unwrap_or(0);
        let right = local_score_lookup
            .get(b.name.as_str())
            .copied()
            .unwrap_or(0);
        b.score
            .cmp(&a.score)
            .then_with(|| right.cmp(&left))
            .then_with(|| a.name.cmp(&b.name))
    });
    recommendations.truncate(SKILL_PICK_MAX_RECOMMENDATIONS);

    let fallback_used = parse_success_count == 0 || recommendations.is_empty();
    if fallback_used {
        recommendations = fallback_skill_pick(&ranked_candidates);
    }

    Ok(SkillPickResponse {
        recommendations,
        fallback_used,
        rounds_succeeded: parse_success_count,
    })
}

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
        parse_skill_pick_response, shortlist_skill_pick_candidates, split_translation_chunks,
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

        let hy_mt_prompt = super::build_translation_system_prompt(
            &cfg,
            "Simplified Chinese",
            "English (en)",
        );
        assert!(
            hy_mt_prompt.contains("Treat the USER message as source text"),
            "HY-MT local models should use the dedicated HY-MT translation prompt"
        );

        cfg.model = "llama3.1:8b".to_string();
        let generic_prompt = super::build_translation_system_prompt(
            &cfg,
            "Simplified Chinese",
            "English (en)",
        );
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

        let prompt =
            super::build_short_translation_system_prompt(&cfg, "Simplified Chinese", "English (en)");
        assert!(
            !prompt.contains("Treat the USER message as source text"),
            "HY-MT prompt adaptation is scoped to local format only"
        );
    }

    // ── get_mymemory_de tests ───────────────────────────────────────

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
    fn mymemory_de_generates_valid_email() {
        with_temp_data_root(|_dir| {
            let email = super::get_mymemory_de();
            assert!(email.ends_with("@skillstar.local"), "email={email}");
            assert!(email.contains('@'), "must contain @: {email}");
            let local = email.split('@').next().unwrap();
            assert!(
                uuid::Uuid::parse_str(local).is_ok(),
                "local part is not a UUID: {local}"
            );
        });
    }

    #[test]
    fn mymemory_de_persists_across_calls() {
        with_temp_data_root(|dir| {
            let first = super::get_mymemory_de();
            let written = std::fs::read_to_string(dir.join(".mymemory_de")).unwrap();
            assert_eq!(first, written.trim());

            let second = super::get_mymemory_de();
            assert_eq!(
                first, second,
                "same email must be returned on subsequent calls"
            );
        });
    }

    #[test]
    fn mymemory_de_overwrites_corrupt_file() {
        with_temp_data_root(|dir| {
            std::fs::write(dir.join(".mymemory_de"), "   \n").unwrap();
            let email = super::get_mymemory_de();
            assert!(
                !email.trim().is_empty(),
                "should regenerate for empty/whitespace file"
            );
            assert!(email.ends_with("@skillstar.local"));
        });
    }

    // ── MyMemory live translation with de parameter ──────────────────

    #[test]
    fn mymemory_translate_with_de_email() {
        with_temp_data_root(|_dir| {
            let rt = tokio::runtime::Runtime::new().expect("create runtime");
            let result = rt.block_on(async {
                // Generate a fresh de email
                let de = super::get_mymemory_de();
                assert!(!de.is_empty(), "de email must not be empty");
                assert!(de.contains('@'), "de must look like an email: {de}");

                // Build a minimal AiConfig for the call
                let config = super::AiConfig {
                    target_language: "zh-CN".to_string(),
                    ..super::AiConfig::default()
                };

                let text = "Hello, world!";
                super::mymemory_translate_short_text(&config, text).await
            });

            match result {
                Ok(translated) => {
                    assert!(!translated.is_empty(), "translation must not be empty");
                    let has_cjk = translated
                        .chars()
                        .any(|c| ('\u{4E00}'..='\u{9FFF}').contains(&c));
                    assert!(
                        has_cjk,
                        "expected Chinese characters in translation, got: {translated}"
                    );
                }
                Err(e) => {
                    eprintln!("⚠ MyMemory API call failed (network issue?): {e}");
                }
            }
        });
    }

    #[test]
    fn load_config_returns_default_when_json_is_corrupted() {
        with_temp_data_root(|dir| {
            std::fs::write(dir.join("ai_config.json"), "{not-valid-json")
                .expect("write corrupt json");
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

    // ── MyMemory Formatting Stability Tests ─────────────────────────────
    // These tests verify that the `Markdown -> HTML -> TranslatedHTML -> Markdown`
    // pipeline correctly preserves structural elements regardless of MyMemory's translation.

    fn simulate_mymemory_format_pipeline(original_md: &str, translated_html: &str) -> String {
        // Step 1: parse Markdown to HTML (what we send)
        let parser = pulldown_cmark::Parser::new(original_md);
        let mut html_input = String::new();
        pulldown_cmark::html::push_html(&mut html_input, parser);

        // (Mock: MyMemory translates internal text, preserves HTML. `translated_html` is returned.)

        // Step 2: parse HTML back to Markdown
        html2md::parse_html(translated_html).trim().to_string()
    }

    #[test]
    fn test_mymemory_format_plain_text() {
        let md = "Hello world";
        let translated_html = "<p>你好，世界</p>\n";
        let result = simulate_mymemory_format_pipeline(md, translated_html);
        assert_eq!(result, "你好，世界");
    }

    #[test]
    fn test_mymemory_format_bullet_list() {
        let md = "- Item 1\n- Item 2";
        let translated_html = "<ul>\n<li>项目 1</li>\n<li>项目 2</li>\n</ul>\n";
        let result = simulate_mymemory_format_pipeline(md, translated_html);
        assert_eq!(result, "* 项目 1\n* 项目 2");
    }

    #[test]
    fn test_mymemory_format_numbered_list() {
        let md = "1. First\n2. Second";
        let translated_html = "<ol>\n<li>第一</li>\n<li>第二</li>\n</ol>\n";
        let result = simulate_mymemory_format_pipeline(md, translated_html);
        assert_eq!(result, "1. 第一\n2. 第二");
    }

    #[test]
    fn test_mymemory_format_bold_text() {
        let md = "This is **bold** text";
        let translated_html = "<p>这是 <strong>粗体</strong> 文本</p>\n";
        let result = simulate_mymemory_format_pipeline(md, translated_html);
        // html2md might use `**` or `__` for strong, but we check if it's correct valid MD
        assert!(result.contains("**粗体**") || result.contains("__粗体__"));
    }

    #[test]
    fn test_mymemory_format_italic_text() {
        let md = "This is *italic* text";
        let translated_html = "<p>这是 <em>斜体</em> 文本</p>\n";
        let result = simulate_mymemory_format_pipeline(md, translated_html);
        assert!(result.contains("*斜体*") || result.contains("_斜体_"));
    }

    #[test]
    fn test_mymemory_format_headers() {
        let md = "### Section Header\nContent";
        let translated_html = "<h3>章节标题</h3>\n<p>内容</p>\n";
        let result = simulate_mymemory_format_pipeline(md, translated_html);
        assert_eq!(result, "### 章节标题 ###\n\n内容");
    }

    #[test]
    fn test_mymemory_format_inline_code() {
        let md = "Run `npm install`";
        let translated_html = "<p>运行 <code>npm install</code></p>\n";
        let result = simulate_mymemory_format_pipeline(md, translated_html);
        assert_eq!(result, "运行 `npm install`");
    }

    #[test]
    fn test_mymemory_format_links() {
        let md = "[Click here](https://example.com)";
        let translated_html = "<p><a href=\"https://example.com\">点击这里</a></p>\n";
        let result = simulate_mymemory_format_pipeline(md, translated_html);
        assert_eq!(result, "[点击这里](https://example.com)");
    }

    #[test]
    fn test_mymemory_format_complex_nested_list() {
        let md = "- **Feature A**: Description\n- **Feature B**: Desc";
        let translated_html = "<ul>\n<li><strong>功能 A</strong>: 描述</li>\n<li><strong>功能 B</strong>: 描述</li>\n</ul>\n";
        let result = simulate_mymemory_format_pipeline(md, translated_html);
        assert!(result.contains("* **功能 A**: 描述") || result.contains("* __功能 A__: 描述"));
        assert!(result.contains("* **功能 B**: 描述") || result.contains("* __功能 B__: 描述"));
    }

    #[test]
    fn test_mymemory_format_multiline_paragraphs() {
        let md = "Paragraph 1\n\nParagraph 2";
        let translated_html = "<p>段落 1</p>\n<p>段落 2</p>\n";
        let result = simulate_mymemory_format_pipeline(md, translated_html);
        assert_eq!(result, "段落 1\n\n段落 2");
    }
}
