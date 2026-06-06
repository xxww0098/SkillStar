//! Provider resolution from the flat (v2) and legacy (v1) provider stores,
//! plus runtime-config resolution and the language display-name table.
//!
//! Split out of `ai_provider/mod.rs` — pure mechanical move, no behavior change.

use anyhow::{Context, Result};
use tracing::warn;

use skillstar_models::providers;

use super::*;

// ── Provider resolution: flat store (v2) ────────────────────────────

/// Resolve a provider reference from the flat (v2) provider store.
///
/// The flat store is the primary source of truth for providers configured
/// through the Models UI. It stores structured fields (`base_url_openai`,
/// `base_url_anthropic`, `api_key`, `default_model`, `meta`) rather than
/// app-specific raw config blobs.
///
/// Claude config field mapping:
/// - `base_url_anthropic` (→ fallback `base_url_openai`) → `ANTHROPIC_BASE_URL`
/// - `api_key`            → `ANTHROPIC_AUTH_TOKEN` / `ANTHROPIC_API_KEY`
/// - `meta.claude_main_model` | `default_model`  → `ANTHROPIC_MODEL`
/// - `meta.claude_haiku_model`   → `ANTHROPIC_DEFAULT_HAIKU_MODEL`
/// - `meta.claude_sonnet_model`  → `ANTHROPIC_DEFAULT_SONNET_MODEL`
/// - `meta.claude_opus_model`    → `ANTHROPIC_DEFAULT_OPUS_MODEL`
///
/// Codex config field mapping:
/// - `base_url_openai`    → `~/.codex/config.toml: base_url`
/// - `api_key`            → `~/.codex/auth.json: OPENAI_API_KEY`
/// - `default_model`      → `~/.codex/config.toml: model`
pub(crate) fn resolve_from_flat_store(
    config: &mut AiConfig,
    app_id: &str,
    provider_id: &str,
) -> Result<String> {
    let path = providers::flat_store_path();
    let store = providers::read_flat_store(&path)
        .context("Failed to read flat provider store")?;

    let entry = store
        .providers
        .iter()
        .find(|p| p.id == provider_id)
        .ok_or_else(|| anyhow::anyhow!("Provider not found in flat store: {provider_id}"))?;

    let label = entry.name.clone();

    match app_id {
        "claude" => {
            if entry.api_key.trim().is_empty() {
                anyhow::bail!("{label} is missing an API key");
            }

            // Prefer dedicated Anthropic endpoint; fall back to OpenAI-compatible.
            let base_url = [entry.base_url_anthropic.trim(), entry.base_url_openai.trim()]
                .iter()
                .copied()
                .find(|s| !s.is_empty())
                .unwrap_or("https://api.anthropic.com")
                .to_string();

            // Main model: claude_main_model meta > default_model > hard default
            let model = get_meta_str(&entry.meta, "claude_main_model")
                .or_else(|| {
                    let dm = entry.default_model.trim();
                    if dm.is_empty() { None } else { Some(dm.to_string()) }
                })
                .unwrap_or_else(|| "claude-sonnet-4-20250514".to_string());

            config.api_format = ApiFormat::Anthropic;
            config.api_key = entry.api_key.trim().to_string();
            config.base_url = base_url;
            config.model = model;
            config.claude_haiku_model  = get_meta_str(&entry.meta, "claude_haiku_model");
            config.claude_sonnet_model = get_meta_str(&entry.meta, "claude_sonnet_model");
            config.claude_opus_model   = get_meta_str(&entry.meta, "claude_opus_model");
            apply_provider_request_meta(config, &entry.meta);
        }
        "codex" => {
            if entry.api_key.trim().is_empty() {
                anyhow::bail!("{label} is missing an API key");
            }

            let base_url = {
                let url = entry.base_url_openai.trim();
                if url.is_empty() { "https://api.openai.com/v1".to_string() } else { url.to_string() }
            };

            let model = {
                let dm = entry.default_model.trim();
                if dm.is_empty() { "gpt-5.4".to_string() } else { dm.to_string() }
            };

            config.api_format = ApiFormat::Openai;
            config.api_key = entry.api_key.trim().to_string();
            config.base_url = base_url;
            config.model = model;
            apply_provider_request_meta(config, &entry.meta);
        }
        _ => anyhow::bail!("Unsupported AI provider app: {app_id}"),
    }

    Ok(label)
}

// ── Provider resolution: legacy store (v1) ──────────────────────────

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

/// Resolve a provider reference from the legacy (v1) per-app provider store.
///
/// The v1 store uses app-specific config blobs:
///
/// Claude: `settings_config["env"]` — ANTHROPIC_* env vars
/// Codex:  `settings_config["auth"]["OPENAI_API_KEY"]` +
///         `settings_config["config"]` (TOML string)
pub(crate) fn resolve_from_legacy_store(
    config: &mut AiConfig,
    app_id: &str,
    provider_id: &str,
) -> Result<String> {
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
                .and_then(|v| v.as_object())
                .ok_or_else(|| anyhow::anyhow!("Claude provider env is missing"))?;

            // API key: prefer ANTHROPIC_AUTH_TOKEN (Claude Code native), accept ANTHROPIC_API_KEY
            let api_key = env
                .get("ANTHROPIC_AUTH_TOKEN")
                .or_else(|| env.get("ANTHROPIC_API_KEY"))
                .and_then(|v| v.as_str())
                .map(str::trim)
                .filter(|v| !v.is_empty())
                .ok_or_else(|| anyhow::anyhow!("{label} is missing an API key"))?;

            let base_url = env
                .get("ANTHROPIC_BASE_URL")
                .and_then(|v| v.as_str())
                .map(str::trim)
                .filter(|v| !v.is_empty())
                .unwrap_or("https://api.anthropic.com");

            // Primary model: ANTHROPIC_MODEL, fallback CLAUDE_CODE_MODEL
            let model = env
                .get("ANTHROPIC_MODEL")
                .or_else(|| env.get("CLAUDE_CODE_MODEL"))
                .and_then(|v| v.as_str())
                .map(str::trim)
                .filter(|v| !v.is_empty())
                .unwrap_or("claude-sonnet-4-20250514");

            config.api_format = ApiFormat::Anthropic;
            config.api_key = api_key.to_string();
            config.base_url = base_url.to_string();
            config.model = model.to_string();

            // Secondary model tier overrides (ANTHROPIC_DEFAULT_*_MODEL)
            config.claude_haiku_model = env
                .get("ANTHROPIC_DEFAULT_HAIKU_MODEL")
                .and_then(|v| v.as_str())
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(str::to_string);
            config.claude_sonnet_model = env
                .get("ANTHROPIC_DEFAULT_SONNET_MODEL")
                .and_then(|v| v.as_str())
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(str::to_string);
            config.claude_opus_model = env
                .get("ANTHROPIC_DEFAULT_OPUS_MODEL")
                .and_then(|v| v.as_str())
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(str::to_string);
        }
        "codex" => {
            let auth = entry
                .settings_config
                .get("auth")
                .and_then(|v| v.as_object())
                .ok_or_else(|| anyhow::anyhow!("Codex provider auth is missing"))?;
            let config_text = entry
                .settings_config
                .get("config")
                .and_then(|v| v.as_str())
                .unwrap_or_default();

            let api_key = auth
                .get("OPENAI_API_KEY")
                .and_then(|v| v.as_str())
                .map(str::trim)
                .filter(|v| !v.is_empty())
                .ok_or_else(|| anyhow::anyhow!("{label} is missing an API key"))?;

            // Base URL lookup order:
            // 1. top-level openai_base_url (legacy cc-switch field)
            // 2. [model_providers.<active>].base_url (new cc-switch nested format)
            // 3. top-level base_url (simple flat format)
            // 4. meta.baseURL
            // 5. default OpenAI endpoint
            let base_url = parse_toml_string_field(config_text, "openai_base_url")
                .or_else(|| parse_codex_active_provider_base_url(config_text))
                .or_else(|| parse_toml_string_field(config_text, "base_url"))
                .or_else(|| {
                    entry
                        .meta
                        .as_ref()
                        .and_then(|m| m.get("baseURL"))
                        .and_then(|v| v.as_str())
                        .map(str::trim)
                        .filter(|v| !v.is_empty())
                        .map(str::to_string)
                })
                .unwrap_or_else(|| "https://api.openai.com/v1".to_string());

            let model = parse_toml_string_field(config_text, "model")
                .unwrap_or_else(|| {
                    if config.model.trim().is_empty() { "gpt-5.4".to_string() } else { config.model.clone() }
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

// ── Public resolution entry point ────────────────────────────────────

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

    // Try the flat store (v2) first — this is where the Models UI stores providers.
    match resolve_from_flat_store(config, app_id, provider_id) {
        Ok(label) => return Ok(label),
        Err(e) => {
            warn!(
                target: "ai_provider",
                app_id,
                provider_id,
                error = %e,
                "flat store lookup failed, falling back to legacy store"
            );
        }
    }

    // Fall back to the legacy per-app store (v1) for backward compatibility.
    resolve_from_legacy_store(config, app_id, provider_id)
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
