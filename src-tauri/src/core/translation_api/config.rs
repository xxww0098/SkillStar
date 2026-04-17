//! Translation API configuration — all provider credentials and per-provider settings.
//!
//! API keys are encrypted at rest using AES-256-GCM with a machine-derived key.
//! Per-provider settings control model selection, temperature, endpoint overrides, etc.

use aes_gcm::{
    Aes256Gcm, Nonce,
    aead::{Aead, KeyInit},
};
use base64::{Engine, engine::general_purpose::STANDARD as BASE64};
use serde::{Deserialize, Serialize};

fn default_translation_target_language() -> String {
    "zh-CN".to_string()
}

// ── Encryption Helpers ────────────────────────────────────────────────

/// Derive the machine-specific AES-256-GCM encryption key.
/// Falls back to a zero-filled key if machine_uid is unavailable.
fn get_encryption_key() -> [u8; 32] {
    let uid = machine_uid::get().unwrap_or_default();
    // SHA-256 of machine_uid — exactly 32 bytes for AES-256
    use sha2::{Digest, Sha256};
    let mut hash = Sha256::new();
    hash.update(uid.as_bytes());
    let result = hash.finalize();
    let mut key = [0u8; 32];
    key.copy_from_slice(&result);
    key
}

/// Encrypt a plaintext API key using AES-256-GCM. Returns base64-encoded ciphertext.
pub fn encrypt_api_key(plaintext: &str) -> String {
    if plaintext.is_empty() {
        return String::new();
    }
    let key = get_encryption_key();
    let cipher = Aes256Gcm::new_from_slice(&key).expect("AES-256-GCM accepts 32-byte key");
    let mut nonce_bytes = [0u8; 12];
    for byte in &mut nonce_bytes {
        *byte = rand::random::<u8>();
    }
    let nonce = Nonce::from_slice(&nonce_bytes);
    let ciphertext = cipher
        .encrypt(nonce, plaintext.as_bytes())
        .expect("encryption should not fail with valid key/nonce");
    // Prepend nonce to ciphertext, then base64
    let mut combined = nonce_bytes.to_vec();
    combined.extend(ciphertext);
    BASE64.encode(&combined)
}

/// Decrypt a base64-encoded ciphertext produced by `encrypt_api_key`.
pub fn decrypt_api_key(encoded: &str) -> String {
    if encoded.is_empty() {
        return String::new();
    }
    let combined = match BASE64.decode(encoded) {
        Ok(c) => c,
        Err(_) => return String::new(),
    };
    if combined.len() < 12 {
        return String::new();
    }
    let key = get_encryption_key();
    let cipher = Aes256Gcm::new_from_slice(&key).expect("AES-256-GCM accepts 32-byte key");
    let (nonce_bytes, ciphertext) = combined.split_at(12);
    let nonce = Nonce::from_slice(nonce_bytes);
    match cipher.decrypt(nonce, ciphertext) {
        Ok(plaintext) => String::from_utf8(plaintext).unwrap_or_default(),
        Err(_) => String::new(),
    }
}

// ── Provider Settings ─────────────────────────────────────────────────

/// Per-LLM-provider settings (model, temperature, base_url override, etc.)
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TranslationProviderSettings {
    #[serde(default)]
    pub model: String,
    #[serde(default)]
    pub temperature: f32,
    /// Override the default base URL for this provider (optional).
    #[serde(default)]
    pub base_url: String,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum TranslationRouteMode {
    Fast,
    #[default]
    Balanced,
    Quality,
}

impl TranslationRouteMode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Fast => "fast",
            Self::Balanced => "balanced",
            Self::Quality => "quality",
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum TranslationFastProvider {
    #[default]
    DeepL,
    Google,
    Azure,
    Experimental,
}

impl TranslationFastProvider {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::DeepL => "deepl",
            Self::Google => "google",
            Self::Azure => "azure",
            Self::Experimental => "experimental",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub struct TranslationQualityProviderRef {
    #[serde(default)]
    pub app_id: String,
    #[serde(default)]
    pub provider_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct TranslationSettings {
    #[serde(default = "default_translation_target_language")]
    pub target_language: String,
    #[serde(default)]
    pub mode: TranslationRouteMode,
    #[serde(default)]
    pub fast_provider: TranslationFastProvider,
    #[serde(default)]
    pub quality_provider_ref: Option<TranslationQualityProviderRef>,
    #[serde(default = "default_allow_emergency_fallback")]
    pub allow_emergency_fallback: bool,
    #[serde(default)]
    pub experimental_providers_enabled: bool,
}

fn default_allow_emergency_fallback() -> bool {
    true
}

impl Default for TranslationSettings {
    fn default() -> Self {
        Self {
            target_language: default_translation_target_language(),
            mode: TranslationRouteMode::default(),
            fast_provider: TranslationFastProvider::default(),
            quality_provider_ref: None,
            allow_emergency_fallback: default_allow_emergency_fallback(),
            experimental_providers_enabled: false,
        }
    }
}

/// Default provider selection (used as the fallback when no provider is active).
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub enum TranslationDefaultProvider {
    #[default]
    DeepL,
    DeepLX,
    Google,
    Azure,
    Gtx,
    DeepSeek,
    Claude,
    OpenAI,
    Gemini,
    Perplexity,
    AzureOpenAI,
    SiliconFlow,
    Groq,
    OpenRouter,
    Nvidia,
    CustomLLM,
}

/// Which providers are enabled for translation.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TranslationEnabledProviders {
    #[serde(default)]
    pub deepl: bool,
    #[serde(default)]
    pub deeplx: bool,
    #[serde(default)]
    pub google: bool,
    #[serde(default)]
    pub azure: bool,
    #[serde(default)]
    pub gtx: bool,
    #[serde(default)]
    pub deepseek: bool,
    #[serde(default)]
    pub claude: bool,
    #[serde(default)]
    pub openai: bool,
    #[serde(default)]
    pub gemini: bool,
    #[serde(default)]
    pub perplexity: bool,
    #[serde(default)]
    pub azure_openai: bool,
    #[serde(default)]
    pub siliconflow: bool,
    #[serde(default)]
    pub groq: bool,
    #[serde(default)]
    pub openrouter: bool,
    #[serde(default)]
    pub nvidia: bool,
    #[serde(default)]
    pub custom_llm: bool,
}

// ── Translation API Config ─────────────────────────────────────────────

/// All translation API credentials and per-provider settings.
/// Stored encrypted in `~/.skillstar/config/ai.json` under the `translation_api` key.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TranslationApiConfig {
    // ── Traditional API Keys ─────────────────────────────────────────
    #[serde(default)]
    pub deepl_key: String,
    #[serde(default)]
    pub deeplx_url: String,
    #[serde(default)]
    pub google_key: String,
    #[serde(default)]
    pub azure_key: String,
    #[serde(default)]
    pub azure_region: String,
    #[serde(default)]
    pub gtx_api_key: String,

    // ── LLM Provider Keys ───────────────────────────────────────────
    #[serde(default)]
    pub deepseek_key: String,
    #[serde(default)]
    pub claude_key: String,
    #[serde(default)]
    pub openai_key: String,
    #[serde(default)]
    pub gemini_key: String,
    #[serde(default)]
    pub perplexity_key: String,
    #[serde(default)]
    pub azure_openai_key: String,
    #[serde(default)]
    pub siliconflow_key: String,
    #[serde(default)]
    pub groq_key: String,
    #[serde(default)]
    pub openrouter_key: String,
    #[serde(default)]
    pub nvidia_key: String,
    #[serde(default)]
    pub custom_llm_key: String,

    // ── LLM Provider Settings ────────────────────────────────────────
    #[serde(default)]
    pub enabled_providers: TranslationEnabledProviders,
    #[serde(default)]
    pub default_provider: TranslationDefaultProvider,
    #[serde(default)]
    pub default_skill_provider: TranslationDefaultProvider,

    #[serde(default)]
    pub deepseek_settings: TranslationProviderSettings,
    #[serde(default)]
    pub claude_settings: TranslationProviderSettings,
    #[serde(default)]
    pub openai_settings: TranslationProviderSettings,
    #[serde(default)]
    pub gemini_settings: TranslationProviderSettings,
    #[serde(default)]
    pub perplexity_settings: TranslationProviderSettings,
    #[serde(default)]
    pub azure_openai_settings: TranslationProviderSettings,
    #[serde(default)]
    pub siliconflow_settings: TranslationProviderSettings,
    #[serde(default)]
    pub groq_settings: TranslationProviderSettings,
    #[serde(default)]
    pub openrouter_settings: TranslationProviderSettings,
    #[serde(default)]
    pub nvidia_settings: TranslationProviderSettings,
    #[serde(default)]
    pub custom_llm_settings: TranslationProviderSettings,
}

impl TranslationApiConfig {
    /// Decrypt all stored API keys and settings in-place.
    /// Call after loading from disk, before use.
    pub fn decrypt_keys(&mut self) {
        self.deepl_key = decrypt_api_key(&self.deepl_key);
        self.google_key = decrypt_api_key(&self.google_key);
        self.azure_key = decrypt_api_key(&self.azure_key);
        self.gtx_api_key = decrypt_api_key(&self.gtx_api_key);
        self.deepseek_key = decrypt_api_key(&self.deepseek_key);
        self.claude_key = decrypt_api_key(&self.claude_key);
        self.openai_key = decrypt_api_key(&self.openai_key);
        self.gemini_key = decrypt_api_key(&self.gemini_key);
        self.perplexity_key = decrypt_api_key(&self.perplexity_key);
        self.azure_openai_key = decrypt_api_key(&self.azure_openai_key);
        self.siliconflow_key = decrypt_api_key(&self.siliconflow_key);
        self.groq_key = decrypt_api_key(&self.groq_key);
        self.openrouter_key = decrypt_api_key(&self.openrouter_key);
        self.nvidia_key = decrypt_api_key(&self.nvidia_key);
        self.custom_llm_key = decrypt_api_key(&self.custom_llm_key);
    }

    /// Encrypt all API keys in-place.
    /// Call before saving to disk.
    pub fn encrypt_keys(&mut self) {
        self.deepl_key = encrypt_api_key(&self.deepl_key);
        self.google_key = encrypt_api_key(&self.google_key);
        self.azure_key = encrypt_api_key(&self.azure_key);
        self.gtx_api_key = encrypt_api_key(&self.gtx_api_key);
        self.deepseek_key = encrypt_api_key(&self.deepseek_key);
        self.claude_key = encrypt_api_key(&self.claude_key);
        self.openai_key = encrypt_api_key(&self.openai_key);
        self.gemini_key = encrypt_api_key(&self.gemini_key);
        self.perplexity_key = encrypt_api_key(&self.perplexity_key);
        self.azure_openai_key = encrypt_api_key(&self.azure_openai_key);
        self.siliconflow_key = encrypt_api_key(&self.siliconflow_key);
        self.groq_key = encrypt_api_key(&self.groq_key);
        self.openrouter_key = encrypt_api_key(&self.openrouter_key);
        self.nvidia_key = encrypt_api_key(&self.nvidia_key);
        self.custom_llm_key = encrypt_api_key(&self.custom_llm_key);
    }

    /// Get the API key for a named provider.
    pub fn get_key(&self, provider: &str) -> &str {
        match provider {
            "deepl" => &self.deepl_key,
            "deeplx" => &self.deeplx_url,
            "google" => &self.google_key,
            "azure" => &self.azure_key,
            "gtx" => &self.gtx_api_key,
            "deepseek" => &self.deepseek_key,
            "claude" => &self.claude_key,
            "openai" => &self.openai_key,
            "gemini" => &self.gemini_key,
            "perplexity" => &self.perplexity_key,
            "azureopenai" => &self.azure_openai_key,
            "siliconflow" => &self.siliconflow_key,
            "groq" => &self.groq_key,
            "openrouter" => &self.openrouter_key,
            "nvidia" => &self.nvidia_key,
            "customllm" => &self.custom_llm_key,
            _ => "",
        }
    }

    /// Get the per-provider settings for a named provider.
    pub fn get_settings(&self, provider: &str) -> TranslationProviderSettings {
        match provider {
            "deepseek" => self.deepseek_settings.clone(),
            "claude" => self.claude_settings.clone(),
            "openai" => self.openai_settings.clone(),
            "gemini" => self.gemini_settings.clone(),
            "perplexity" => self.perplexity_settings.clone(),
            "azureopenai" => self.azure_openai_settings.clone(),
            "siliconflow" => self.siliconflow_settings.clone(),
            "groq" => self.groq_settings.clone(),
            "openrouter" => self.openrouter_settings.clone(),
            "nvidia" => self.nvidia_settings.clone(),
            "customllm" => self.custom_llm_settings.clone(),
            _ => TranslationProviderSettings::default(),
        }
    }

    /// Check whether any API key is configured for a given provider.
    pub fn has_key(&self, provider: &str) -> bool {
        !self.get_key(provider).is_empty()
    }
}
