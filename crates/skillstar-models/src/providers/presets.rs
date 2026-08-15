//! Flat-store provider presets (v2): preset registry and creation helper.

use super::*;

// ---------------------------------------------------------------------------
// Native Official seeds (Claude / Codex browser/client login — no API key)
// ---------------------------------------------------------------------------

/// Stable store/preset id for Claude Code native (Official) login.
pub const CLAUDE_OFFICIAL_ID: &str = "claude-official";
/// Stable store/preset id for Codex ChatGPT OAuth (Official) login.
pub const CODEX_OFFICIAL_ID: &str = "codex-official";

/// What kind of thing a preset describes.
///
/// v3 spelled this as a free string with four values, one of which —
/// `"official"` — covered two structurally different things: Grok, a vendor you
/// reach with an API key, and the Claude/Codex seeds, which have no key and no
/// endpoints because the point of them is to hand control back to the agent's
/// own login. Telling those apart required an id whitelist
/// (`is_native_official_preset_id`) consulted from six places, and the
/// whitelist's own comment admitted it was standing in for a missing field.
///
/// Splitting the category is that missing field. The whitelist is gone; the
/// question "is this a native-login row" is now answered by the data.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
#[serde(rename_all = "snake_case")]
#[ts(export, export_to = "PresetCategory.ts")]
pub enum PresetCategory {
    /// Chinese domestic model vendors.
    Domestic,
    /// Relays and aggregators that front several upstream vendors.
    Relay,
    /// A vendor's own API, reached with an API key (Grok).
    VendorOfficial,
    /// The agent's own browser/client login. No key, no endpoints — syncing one
    /// means *clearing* SkillStar's managed fields.
    NativeLogin,
    /// The generic "any OpenAI-compatible endpoint" template. Not in the
    /// backend registry; the frontend synthesises it, and it is listed here so
    /// the category type is total across everything the UI can show.
    OpenaiCompatible,
}

impl PresetCategory {
    /// Whether presets in this category are native-login seeds.
    pub fn is_native_login(self) -> bool {
        matches!(self, PresetCategory::NativeLogin)
    }
}

/// Whether `preset_id` names a native-login seed.
///
/// Now a lookup in the registry rather than a hardcoded id list, so adding a
/// native-login preset is one row and no whitelist edit.
pub fn is_native_official_preset_id(preset_id: &str) -> bool {
    get_all_presets_flat()
        .iter()
        .any(|p| p.id == preset_id && p.category.is_native_login())
}

/// Ensure the native-login seed rows exist in the v4 store.
///
/// Inserts missing seeds with stable ids (`claude-official` / `codex-official`).
/// Skips when a row with the same `id` or `preset_id` already exists, so a user
/// who renamed their Official row keeps the rename. Returns `true` when the
/// store was mutated.
pub fn ensure_official_providers(store: &mut super::binding::ProvidersStoreV4) -> bool {
    let mut changed = false;
    for preset_id in [CLAUDE_OFFICIAL_ID, CODEX_OFFICIAL_ID] {
        let exists = store
            .providers
            .iter()
            .any(|p| p.id == preset_id || p.preset_id.as_deref() == Some(preset_id));
        if exists {
            continue;
        }
        let Ok(mut provider) = create_provider_from_preset(preset_id, "") else {
            continue;
        };
        // Append without reshuffling anything the user has ordered.
        let max_sort = store
            .providers
            .iter()
            .map(|p| p.sort_index)
            .max()
            .unwrap_or(0);
        provider.sort_index = if store.providers.is_empty() {
            0
        } else {
            max_sort + 1
        };
        store.providers.push(provider);
        changed = true;
    }
    changed
}

/// The `/v1/responses` endpoint implied by an OpenAI-compatible base URL.
///
/// Only `api.openai.com` is known to speak the Responses API without being
/// probed. Everything else yields `None`, which is what keeps a third-party
/// relay out of Codex's config until a probe says otherwise — writing one in
/// is what produced the `wire_api = "chat"` tables that stop Codex booting.
///
/// Shared by preset creation and the v3 → v4 migration so the two cannot
/// disagree about which hosts get an endpoint for free.
pub fn derive_responses_endpoint(base_url_openai: &str) -> Option<String> {
    let trimmed = base_url_openai.trim();
    if trimmed.contains("api.openai.com") {
        Some(trimmed.to_string())
    } else {
        None
    }
}

/// Build a v4 [`Provider`] row from a built-in preset.
///
/// Native-login seeds keep the preset id as their stable row id and carry an
/// [`Credential::ExternalCli`] rather than an empty key string — "the
/// credential lives in another CLI's store" is a state, not an absence.
pub fn create_provider_from_preset(
    preset_id: &str,
    api_key: &str,
) -> Result<super::provider::Provider> {
    use super::credential::{Credential, NoCredentialReason};
    use super::provider::{Endpoints, Provider, ProviderCaps, Tri};

    let presets = get_all_presets_flat();
    let preset = presets
        .into_iter()
        .find(|p| p.id == preset_id)
        .with_context(|| format!("Preset '{preset_id}' not found in flat preset registry"))?;

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64;

    let native_login = preset.category.is_native_login();
    let id = if native_login {
        preset.id.clone()
    } else {
        Uuid::new_v4().to_string()
    };

    let credential = if native_login {
        Credential::ExternalCli {
            surface: if preset.id == CODEX_OFFICIAL_ID {
                "codex".to_string()
            } else {
                "claude".to_string()
            },
        }
    } else if api_key.trim().is_empty() {
        Credential::None {
            reason: NoCredentialReason::LocalService,
        }
    } else {
        Credential::single_key(Uuid::new_v4().to_string(), api_key)
    };

    let openai_responses = derive_responses_endpoint(&preset.base_url_openai);
    let responses_api = if openai_responses.is_some() {
        Tri::Yes
    } else {
        Tri::Unknown
    };

    let mut provider = Provider::new(id, preset.name);
    provider.preset_id = Some(preset.id);
    provider.endpoints = Endpoints {
        openai_chat: non_empty(&preset.base_url_openai),
        openai_responses,
        anthropic_messages: non_empty(&preset.base_url_anthropic),
        models_list: non_empty(&preset.models_url),
    };
    provider.credential = credential;
    provider.caps = ProviderCaps {
        responses_api,
        ..ProviderCaps::unknown()
    };
    provider.icon_color = Some(preset.icon_color);
    provider.created_at_ms = Some(now);
    Ok(provider)
}

fn non_empty(s: &str) -> Option<String> {
    let trimmed = s.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
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
    pub category: PresetCategory,
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
            category: PresetCategory::Domestic,
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
            category: PresetCategory::Domestic,
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
            category: PresetCategory::Domestic,
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
            category: PresetCategory::Domestic,
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
            category: PresetCategory::Domestic,
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
            category: PresetCategory::Domestic,
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
            category: PresetCategory::Domestic,
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
            category: PresetCategory::Domestic,
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
            category: PresetCategory::Relay,
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
            category: PresetCategory::Relay,
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
            category: PresetCategory::NativeLogin,
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
            category: PresetCategory::NativeLogin,
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
            category: PresetCategory::VendorOfficial,
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
