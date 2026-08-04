//! Flat-store provider presets (v2): preset registry and creation helper.

use super::*;

// ---------------------------------------------------------------------------
// Native Official seeds (Claude / Codex browser/client login — no API key)
// ---------------------------------------------------------------------------

/// Stable store/preset id for Claude Code native (Official) login.
pub const CLAUDE_OFFICIAL_ID: &str = "claude-official";
/// Stable store/preset id for Codex ChatGPT OAuth (Official) login.
pub const CODEX_OFFICIAL_ID: &str = "codex-official";

/// Whether `preset_id` is a native Official seed (no API key / empty endpoints).
///
/// Distinct from Grok-style `category: "official"` presets that still use keys.
pub fn is_native_official_preset_id(preset_id: &str) -> bool {
    matches!(preset_id, CLAUDE_OFFICIAL_ID | CODEX_OFFICIAL_ID)
}

/// Whether a flat provider row is a native Official seed.
///
/// Matches stable `id` or `preset_id` so renamed display names still qualify.
pub fn is_native_official_provider(provider: &ProviderEntryFlat) -> bool {
    is_native_official_preset_id(&provider.id)
        || provider
            .preset_id
            .as_deref()
            .is_some_and(is_native_official_preset_id)
}

/// Ensure Claude / Codex Official seed rows exist in the store.
///
/// Inserts missing seeds with stable ids (`claude-official` / `codex-official`).
/// Skips when a row with the same `id` or `preset_id` already exists (does not
/// overwrite a user-renamed Official row). Returns `true` when the store was
/// mutated.
pub fn ensure_official_providers(store: &mut FlatProvidersStore) -> bool {
    let mut changed = false;
    for preset_id in [CLAUDE_OFFICIAL_ID, CODEX_OFFICIAL_ID] {
        let exists = store.providers.iter().any(|p| {
            p.id == preset_id || p.preset_id.as_deref() == Some(preset_id)
        });
        if exists {
            continue;
        }
        let Ok(mut entry) = create_from_preset_flat(preset_id, "") else {
            continue;
        };
        // Assign sort_index at the front-ish of the list without reshuffling.
        let max_sort = store.providers.iter().map(|p| p.sort_index).max().unwrap_or(0);
        entry.sort_index = if store.providers.is_empty() {
            0
        } else {
            max_sort + 1
        };
        store.providers.push(entry);
        changed = true;
    }
    changed
}

// ---------------------------------------------------------------------------
// Flat provider preset types (v2 architecture)
// ---------------------------------------------------------------------------

/// A built-in provider preset template for the flat store (v2).
///
/// Each preset defines both OpenAI and Anthropic endpoints plus optional
/// metadata for balance queries and API key acquisition. Models are fetched
/// from the provider after creation rather than baked into presets.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderPresetFlat {
    pub id: String,
    pub name: String,
    /// Category: "domestic", "relay", "official", or "openai_compatible"
    pub category: String,
    pub base_url_openai: String,
    pub base_url_anthropic: String,
    /// Unique "fetch available models" URL for this provider.
    ///
    /// Shared by every agent config (Claude, Codex, …). Most providers expose
    /// an OpenAI-compatible `.../v1/models` endpoint; when empty the frontend
    /// falls back to `base_url_openai + "/models"`.
    #[serde(default)]
    pub models_url: String,
    pub models: Vec<String>,
    pub icon_color: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub api_key_url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub balance_endpoint: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub balance_parser: Option<String>,
    /// Optional alternate base URLs for endpoint speed tests in the UI.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub endpoint_candidates: Vec<String>,
}

/// Returns all built-in flat provider presets.
///
/// Includes domestic Chinese model providers, relay/proxy services, and
/// OpenAI-compatible endpoints.
pub fn get_all_presets_flat() -> Vec<ProviderPresetFlat> {
    vec![
        // ── 国内模型 (Domestic) ──
        ProviderPresetFlat {
            id: "deepseek".to_string(),
            name: "DeepSeek".to_string(),
            category: "domestic".to_string(),
            base_url_openai: "https://api.deepseek.com/v1".to_string(),
            base_url_anthropic: "https://api.deepseek.com/anthropic".to_string(),
            models_url: "https://api.deepseek.com/v1/models".to_string(),
            models: vec![],
            icon_color: "#4D6BFE".to_string(),
            api_key_url: Some("https://platform.deepseek.com/api_keys".to_string()),
            balance_endpoint: Some("https://api.deepseek.com/user/balance".to_string()),
            balance_parser: Some("deepseek".to_string()),
            endpoint_candidates: vec![
                "https://api.deepseek.com/v1".to_string(),
                "https://api.deepseek.com/anthropic".to_string(),
            ],
        },
        ProviderPresetFlat {
            id: "kimi".to_string(),
            name: "Kimi".to_string(),
            category: "domestic".to_string(),
            base_url_openai: "https://api.moonshot.cn/v1".to_string(),
            base_url_anthropic: "https://api.moonshot.cn/anthropic".to_string(),
            models_url: "https://api.moonshot.cn/v1/models".to_string(),
            models: vec![],
            icon_color: "#5B45E0".to_string(),
            api_key_url: Some("https://platform.moonshot.cn/console/api-keys".to_string()),
            balance_endpoint: Some("https://api.moonshot.cn/v1/users/me/balance".to_string()),
            balance_parser: Some("kimi".to_string()),
            endpoint_candidates: vec![],
        },
        ProviderPresetFlat {
            id: "kimi-coding".to_string(),
            name: "Kimi For Coding".to_string(),
            category: "domestic".to_string(),
            base_url_openai: "https://api.kimi.com/coding/v1".to_string(),
            base_url_anthropic: "https://api.kimi.com/coding/".to_string(),
            models_url: "https://api.moonshot.cn/v1/models".to_string(),
            models: vec![],
            icon_color: "#5B45E0".to_string(),
            api_key_url: Some("https://platform.moonshot.cn/console/api-keys".to_string()),
            balance_endpoint: None,
            balance_parser: None,
            endpoint_candidates: vec![],
        },
        ProviderPresetFlat {
            id: "minimax".to_string(),
            name: "MiniMax".to_string(),
            category: "domestic".to_string(),
            base_url_openai: "https://api.minimax.chat/v1".to_string(),
            base_url_anthropic: "https://api.minimax.chat/anthropic".to_string(),
            models_url: "https://api.minimax.chat/v1/models".to_string(),
            models: vec![],
            icon_color: "#FF6B35".to_string(),
            api_key_url: Some(
                "https://platform.minimaxi.com/user-center/basic-information/interface-key"
                    .to_string(),
            ),
            balance_endpoint: None,
            balance_parser: None,
            endpoint_candidates: vec![],
        },
        ProviderPresetFlat {
            id: "glm".to_string(),
            name: "智谱 GLM".to_string(),
            category: "domestic".to_string(),
            base_url_openai: "https://open.bigmodel.cn/api/paas/v4".to_string(),
            base_url_anthropic: "https://open.bigmodel.cn/api/anthropic".to_string(),
            models_url: "https://open.bigmodel.cn/api/paas/v4/models".to_string(),
            models: vec![],
            icon_color: "#3366FF".to_string(),
            api_key_url: Some("https://open.bigmodel.cn/usercenter/apikeys".to_string()),
            balance_endpoint: None,
            balance_parser: None,
            endpoint_candidates: vec![],
        },
        ProviderPresetFlat {
            id: "glm-coding".to_string(),
            name: "智谱 GLM Coding Plan".to_string(),
            category: "domestic".to_string(),
            base_url_openai: "https://api.z.ai/api/coding/paas/v4".to_string(),
            base_url_anthropic: "https://api.z.ai/api/anthropic".to_string(),
            models_url: "https://api.z.ai/api/coding/paas/v4/models".to_string(),
            models: vec![],
            icon_color: "#3366FF".to_string(),
            api_key_url: Some("https://open.bigmodel.cn/usercenter/apikeys".to_string()),
            balance_endpoint: None,
            balance_parser: None,
            endpoint_candidates: vec![],
        },
        ProviderPresetFlat {
            id: "longcat".to_string(),
            name: "LongCat".to_string(),
            category: "domestic".to_string(),
            base_url_openai: "https://api.longcat.chat/openai/v1".to_string(),
            base_url_anthropic: "https://api.longcat.chat/anthropic".to_string(),
            models_url: "https://api.longcat.chat/openai/v1/models".to_string(),
            models: vec![],
            icon_color: "#FF6A00".to_string(),
            api_key_url: Some("https://longcat.chat/platform/api_keys".to_string()),
            balance_endpoint: None,
            balance_parser: None,
            endpoint_candidates: vec![
                "https://api.longcat.chat/openai/v1".to_string(),
                "https://api.longcat.chat/anthropic".to_string(),
            ],
        },
        ProviderPresetFlat {
            id: "xiaomi-mimo".to_string(),
            name: "小米 MiMo".to_string(),
            category: "domestic".to_string(),
            base_url_openai: "https://api.xiaomimimo.com/v1".to_string(),
            base_url_anthropic: "https://api.xiaomimimo.com/anthropic".to_string(),
            models_url: "https://api.xiaomimimo.com/v1/models".to_string(),
            models: vec![],
            icon_color: "#FF6900".to_string(),
            api_key_url: Some("https://platform.xiaomimimo.com".to_string()),
            balance_endpoint: None,
            balance_parser: None,
            endpoint_candidates: vec![
                "https://api.xiaomimimo.com/v1".to_string(),
                "https://api.xiaomimimo.com/anthropic".to_string(),
            ],
        },
        // ── 官方中转站 (Relay) ──
        ProviderPresetFlat {
            id: "openrouter".to_string(),
            name: "OpenRouter".to_string(),
            category: "relay".to_string(),
            base_url_openai: "https://openrouter.ai/api/v1".to_string(),
            base_url_anthropic: String::new(),
            models_url: "https://openrouter.ai/api/v1/models".to_string(),
            models: vec![],
            icon_color: "#6366F1".to_string(),
            api_key_url: Some("https://openrouter.ai/keys".to_string()),
            balance_endpoint: Some("https://openrouter.ai/api/v1/credits".to_string()),
            balance_parser: Some("openrouter".to_string()),
            endpoint_candidates: vec!["https://openrouter.ai/api/v1".to_string()],
        },
        ProviderPresetFlat {
            id: "siliconflow".to_string(),
            name: "SiliconFlow".to_string(),
            category: "relay".to_string(),
            base_url_openai: "https://api.siliconflow.cn/v1".to_string(),
            base_url_anthropic: String::new(),
            models_url: "https://api.siliconflow.cn/v1/models".to_string(),
            models: vec![],
            icon_color: "#00D4AA".to_string(),
            api_key_url: Some("https://cloud.siliconflow.cn/account/ak".to_string()),
            balance_endpoint: Some("https://api.siliconflow.cn/v1/user/info".to_string()),
            balance_parser: Some("siliconflow".to_string()),
            endpoint_candidates: vec!["https://api.siliconflow.cn/v1".to_string()],
        },
        // ── Native Official (browser/client login; no API key) ──
        // Distinct from Grok below: these seeds use fixed ids and empty
        // endpoints. Identity is by preset/store id, not empty-URL heuristics.
        ProviderPresetFlat {
            id: CLAUDE_OFFICIAL_ID.to_string(),
            name: "Claude Official".to_string(),
            category: "official".to_string(),
            base_url_openai: String::new(),
            base_url_anthropic: String::new(),
            models_url: String::new(),
            models: vec![],
            icon_color: "#D97757".to_string(),
            api_key_url: None,
            balance_endpoint: None,
            balance_parser: None,
            endpoint_candidates: vec![],
        },
        ProviderPresetFlat {
            id: CODEX_OFFICIAL_ID.to_string(),
            name: "Codex Official".to_string(),
            category: "official".to_string(),
            base_url_openai: String::new(),
            base_url_anthropic: String::new(),
            models_url: String::new(),
            models: vec![],
            icon_color: "#10A37F".to_string(),
            api_key_url: None,
            balance_endpoint: None,
            balance_parser: None,
            endpoint_candidates: vec![],
        },
        // ── 官方大厂 (API-key Official vendors) ──
        ProviderPresetFlat {
            id: "grok".to_string(),
            name: "Grok (xAI)".to_string(),
            category: "official".to_string(),
            base_url_openai: "https://api.x.ai/v1".to_string(),
            base_url_anthropic: String::new(),
            models_url: "https://api.x.ai/v1/models".to_string(),
            models: vec![],
            icon_color: "#000000".to_string(),
            api_key_url: Some("https://console.x.ai/".to_string()),
            balance_endpoint: None,
            balance_parser: None,
            endpoint_candidates: vec!["https://api.x.ai/v1".to_string()],
        },
    ]
}

/// Create a new flat provider entry from a built-in preset.
///
/// Looks up the preset by ID, sets the current timestamp, and copies all
/// relevant fields from the preset template. Most presets get a fresh UUID;
/// native Official seeds (`claude-official` / `codex-official`) use a stable
/// id matching the preset id and allow an empty API key.
///
/// # Arguments
/// * `preset_id` - The ID of the preset to use (e.g., "deepseek", "kimi")
/// * `api_key` - The user's API key for this provider (may be empty for Official)
///
/// # Returns
/// A fully populated `ProviderEntryFlat` ready to be inserted into the store.
///
/// # Errors
/// Returns an error if the `preset_id` is not found in the preset registry.
pub fn create_from_preset_flat(preset_id: &str, api_key: &str) -> Result<ProviderEntryFlat> {
    let presets = get_all_presets_flat();
    let preset = presets
        .into_iter()
        .find(|p| p.id == preset_id)
        .with_context(|| format!("Preset '{}' not found in flat preset registry", preset_id))?;

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64;

    let native_official = is_native_official_preset_id(preset_id);
    let id = if native_official {
        preset_id.to_string()
    } else {
        Uuid::new_v4().to_string()
    };
    // Codex Official binds ChatGPT OAuth — never write OPENAI_API_KEY.
    let codex_auth_mode = if preset_id == CODEX_OFFICIAL_ID {
        "oauth".to_string()
    } else {
        default_codex_auth_mode()
    };

    Ok(ProviderEntryFlat {
        id,
        name: preset.name,
        base_url_openai: preset.base_url_openai,
        base_url_anthropic: preset.base_url_anthropic,
        models_url: preset.models_url,
        api_key: api_key.to_string(),
        models: vec![],
        default_model: String::new(),
        sort_index: 0,
        preset_id: Some(preset.id),
        icon_color: Some(preset.icon_color),
        notes: None,
        created_at: Some(now),
        meta: None,
        codex_wire_api: default_codex_wire_api(),
        codex_auth_mode,
    })
}
