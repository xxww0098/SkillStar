//! Flat-store provider presets (v2): preset registry and creation helper.

use super::*;

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
    /// Category: "domestic", "relay", or "openai_compatible"
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
            api_key_url: Some("https://platform.minimaxi.com/user-center/basic-information/interface-key".to_string()),
            balance_endpoint: None,
            balance_parser: None,
            endpoint_candidates: vec![],
        },
        ProviderPresetFlat {
            id: "qwen".to_string(),
            name: "通义千问".to_string(),
            category: "domestic".to_string(),
            base_url_openai: "https://dashscope.aliyuncs.com/compatible-mode/v1".to_string(),
            base_url_anthropic: "https://dashscope.aliyuncs.com/api/v2/apps/anthropic".to_string(),
            models_url: "https://dashscope.aliyuncs.com/compatible-mode/v1/models".to_string(),
            models: vec![],
            icon_color: "#6236FF".to_string(),
            api_key_url: Some("https://dashscope.console.aliyun.com/apiKey".to_string()),
            balance_endpoint: None,
            balance_parser: None,
            endpoint_candidates: vec![],
        },
        ProviderPresetFlat {
            id: "qwen-coding".to_string(),
            name: "通义千问 Coding Plan".to_string(),
            category: "domestic".to_string(),
            base_url_openai: "https://coding-intl.dashscope.aliyuncs.com/v1".to_string(),
            base_url_anthropic: "https://coding-intl.dashscope.aliyuncs.com/apps/anthropic".to_string(),
            models_url: "https://coding-intl.dashscope.aliyuncs.com/v1/models".to_string(),
            models: vec![],
            icon_color: "#6236FF".to_string(),
            api_key_url: Some("https://dashscope.console.aliyun.com/apiKey".to_string()),
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
            id: "volcengine".to_string(),
            name: "火山方舟".to_string(),
            category: "domestic".to_string(),
            base_url_openai: "https://ark.cn-beijing.volces.com/api/v3".to_string(),
            base_url_anthropic: "https://ark.cn-beijing.volces.com/api/v3/anthropic".to_string(),
            models_url: "https://ark.cn-beijing.volces.com/api/v3/models".to_string(),
            models: vec![],
            icon_color: "#FF4D4F".to_string(),
            api_key_url: Some("https://console.volcengine.com/ark/region:ark+cn-beijing/apiKey".to_string()),
            balance_endpoint: None,
            balance_parser: None,
            endpoint_candidates: vec![],
        },
        ProviderPresetFlat {
            id: "mimo".to_string(),
            name: "小米 MiMo".to_string(),
            category: "domestic".to_string(),
            base_url_openai: "https://platform.xiaomimimo.com/v1".to_string(),
            base_url_anthropic: "https://platform.xiaomimimo.com/anthropic".to_string(),
            models_url: "https://platform.xiaomimimo.com/v1/models".to_string(),
            models: vec![],
            icon_color: "#FF6900".to_string(),
            api_key_url: Some("https://platform.xiaomimimo.com".to_string()),
            balance_endpoint: None,
            balance_parser: None,
            endpoint_candidates: vec![],
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
        // ── 官方大厂 (Official) ──
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
/// Looks up the preset by ID, generates a UUID, sets the current timestamp,
/// and copies all relevant fields from the preset template.
///
/// # Arguments
/// * `preset_id` - The ID of the preset to use (e.g., "deepseek", "kimi")
/// * `api_key` - The user's API key for this provider
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

    Ok(ProviderEntryFlat {
        id: Uuid::new_v4().to_string(),
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
        codex_auth_mode: default_codex_auth_mode(),
    })
}
