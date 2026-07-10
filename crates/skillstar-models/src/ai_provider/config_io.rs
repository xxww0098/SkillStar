//! Concurrency limiting, config load/save, API-key crypto, and legacy
//! Codex TOML / `model_providers.json` meta parsing.
//!
//! Split out of `ai_provider/mod.rs` — pure mechanical move, no behavior change.

use anyhow::{Context, Result};
use std::path::PathBuf;
use std::sync::{Arc, LazyLock, Mutex};
use tracing::{error, warn};

use super::constants::AI_CONFIG_CACHE_TTL;
use super::*;

/// Cached concurrency limiter: `(budget, semaphore)` rebuilt when the budget changes.
type SemaphoreCache = Mutex<Option<(u32, Arc<tokio::sync::Semaphore>)>>;

static AI_REQUEST_SEMAPHORE: LazyLock<SemaphoreCache> = LazyLock::new(|| Mutex::new(None));

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
    config.max_concurrent_requests.max(1)
}

fn get_ai_request_semaphore(config: &AiConfig) -> Result<Arc<tokio::sync::Semaphore>> {
    let budget = ai_request_concurrency_budget(config);
    let mut guard = AI_REQUEST_SEMAPHORE
        .lock()
        .map_err(|_| anyhow::anyhow!("AI request semaphore lock poisoned"))?;

    if let Some((cached_budget, semaphore)) = guard.as_ref()
        && *cached_budget == budget
    {
        return Ok(semaphore.clone());
    }

    let semaphore = Arc::new(tokio::sync::Semaphore::new(budget as usize));
    *guard = Some((budget, semaphore.clone()));
    Ok(semaphore)
}

pub(crate) async fn acquire_ai_request_permit(
    config: &AiConfig,
) -> Result<tokio::sync::OwnedSemaphorePermit> {
    let semaphore = get_ai_request_semaphore(config)?;
    semaphore
        .acquire_owned()
        .await
        .map_err(|_| anyhow::anyhow!("AI request semaphore closed"))
}

pub(crate) fn config_path() -> PathBuf {
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
    if let Ok(guard) = AI_CONFIG_CACHE.lock()
        && let Some((ts, cached)) = guard.as_ref()
        && ts.elapsed() < AI_CONFIG_CACHE_TTL
    {
        return cached.clone();
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

// ── TOML helpers (legacy Codex config.toml parsing) ─────────────────

/// Scan all lines in `config_text` for a top-level `field = "value"` assignment.
///
/// Intentionally ignores TOML section headers so it can find fields regardless
/// of nesting level — Codex uses both flat (`base_url = "…"`) and nested
/// (`[model_providers.X] / base_url = "…"`) formats, and the first match wins.
pub(crate) fn parse_toml_string_field(config_text: &str, field: &str) -> Option<String> {
    let prefix = format!("{field} =");
    for line in config_text.lines() {
        let trimmed = line.trim();
        if !trimmed.starts_with(&prefix) {
            continue;
        }
        let rhs = trimmed.split_once('=')?.1.trim();
        if rhs.starts_with('"') {
            let mut chars = rhs.chars();
            let _ = chars.next(); // consume opening quote
            let mut collected = String::new();
            for ch in chars {
                if ch == '"' {
                    break;
                }
                collected.push(ch);
            }
            return Some(collected).filter(|v| !v.trim().is_empty());
        }
    }
    None
}

/// Extract `base_url` from the active `[model_providers.<id>]` table.
///
/// Codex config.toml can use the nested format:
/// ```toml
/// model_provider = "ccswitch"
///
/// [model_providers.ccswitch]
/// base_url = "https://..."
/// wire_api = "responses"
/// ```
/// This helper finds the active provider section and returns its `base_url`.
pub(crate) fn parse_codex_active_provider_base_url(config_text: &str) -> Option<String> {
    let model_provider = parse_toml_string_field(config_text, "model_provider")?;
    let section_header = format!("[model_providers.{model_provider}]");
    let mut in_section = false;

    for line in config_text.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('[') {
            in_section = trimmed == section_header.as_str();
            continue;
        }
        if !in_section {
            continue;
        }
        let prefix = "base_url =";
        if trimmed.starts_with(prefix) {
            let rhs = trimmed.split_once('=')?.1.trim();
            if rhs.starts_with('"') {
                let inner: String = rhs.chars().skip(1).take_while(|&c| c != '"').collect();
                let inner = inner.trim();
                if !inner.is_empty() {
                    return Some(inner.to_string());
                }
            }
        }
    }
    None
}

// ── Meta helpers ────────────────────────────────────────────────────

/// Extract a non-empty string value from a JSON meta object.
pub(crate) fn get_meta_str(meta: &Option<serde_json::Value>, key: &str) -> Option<String> {
    meta.as_ref()
        .and_then(|m| m.get(key))
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
}

fn meta_u64(meta: &Option<serde_json::Value>, key: &str) -> Option<u64> {
    meta.as_ref().and_then(|m| m.get(key)).and_then(|v| {
        v.as_u64()
            .or_else(|| v.as_i64().and_then(|n| u64::try_from(n).ok()))
            .or_else(|| v.as_f64().map(|n| n as u64))
    })
}

/// Apply per-provider HTTP tuning from `model_providers.json` meta into runtime config.
pub(crate) fn apply_provider_request_meta(config: &mut AiConfig, meta: &Option<serde_json::Value>) {
    let timeout = meta_u64(meta, "timeout").filter(|&s| (5..=600).contains(&s));
    config.request_timeout_secs = timeout;
}
