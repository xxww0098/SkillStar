//! Translation API configuration — simplified to DeepL + DeepLX only.
//!
//! API keys are encrypted at rest using AES-256-GCM with a machine-derived key.
//! Quality LLM translation is handled via the Models provider reference.

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

// ── Quality Provider Reference ──────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub struct TranslationQualityProviderRef {
    #[serde(default)]
    pub app_id: String,
    #[serde(default)]
    pub provider_id: String,
}

// ── Translation Settings ────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct TranslationSettings {
    #[serde(default = "default_translation_target_language")]
    pub target_language: String,
    #[serde(default)]
    pub quality_provider_ref: Option<TranslationQualityProviderRef>,
    // Legacy fields — kept for backward compat deserialization, ignored at runtime.
    #[serde(default, skip_serializing)]
    pub mode: Option<serde_json::Value>,
    #[serde(default, skip_serializing)]
    pub fast_provider: Option<serde_json::Value>,
    #[serde(default, skip_serializing)]
    pub allow_emergency_fallback: Option<serde_json::Value>,
    #[serde(default, skip_serializing)]
    pub experimental_providers_enabled: Option<serde_json::Value>,
}

impl Default for TranslationSettings {
    fn default() -> Self {
        Self {
            target_language: default_translation_target_language(),
            quality_provider_ref: None,
            mode: None,
            fast_provider: None,
            allow_emergency_fallback: None,
            experimental_providers_enabled: None,
        }
    }
}

// ── Translation API Config ─────────────────────────────────────────

/// Simplified translation API credentials.
/// Only DeepL (paid) and DeepLX (free) are supported.
/// LLM quality translation is handled via the Models provider reference.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TranslationApiConfig {
    #[serde(default)]
    pub deepl_key: String,
    #[serde(default)]
    pub deeplx_key: String,
    #[serde(default)]
    pub deeplx_url: String,
    // Legacy fields — serde(default) absorbs old config values on load,
    // skip_serializing prevents them from being written back.
    #[serde(default, skip_serializing)]
    pub google_key: Option<String>,
    #[serde(default, skip_serializing)]
    pub azure_key: Option<String>,
    #[serde(default, skip_serializing)]
    pub azure_region: Option<String>,
    #[serde(default, skip_serializing)]
    pub gtx_api_key: Option<String>,
    #[serde(default, skip_serializing)]
    pub deepseek_key: Option<String>,
    #[serde(default, skip_serializing)]
    pub claude_key: Option<String>,
    #[serde(default, skip_serializing)]
    pub openai_key: Option<String>,
    #[serde(default, skip_serializing)]
    pub gemini_key: Option<String>,
    #[serde(default, skip_serializing)]
    pub perplexity_key: Option<String>,
    #[serde(default, skip_serializing)]
    pub azure_openai_key: Option<String>,
    #[serde(default, skip_serializing)]
    pub siliconflow_key: Option<String>,
    #[serde(default, skip_serializing)]
    pub groq_key: Option<String>,
    #[serde(default, skip_serializing)]
    pub openrouter_key: Option<String>,
    #[serde(default, skip_serializing)]
    pub nvidia_key: Option<String>,
    #[serde(default, skip_serializing)]
    pub custom_llm_key: Option<String>,
}

impl TranslationApiConfig {
    /// Decrypt all stored API keys in-place.
    /// Call after loading from disk, before use.
    pub fn decrypt_keys(&mut self) {
        self.deepl_key = decrypt_api_key(&self.deepl_key);
        self.deeplx_key = decrypt_api_key(&self.deeplx_key);
    }

    /// Encrypt all API keys in-place.
    /// Call before saving to disk.
    pub fn encrypt_keys(&mut self) {
        self.deepl_key = encrypt_api_key(&self.deepl_key);
        self.deeplx_key = encrypt_api_key(&self.deeplx_key);
    }

    /// Check whether any API key is configured for a given provider.
    pub fn has_key(&self, provider: &str) -> bool {
        match provider {
            "deepl" => !self.deepl_key.trim().is_empty(),
            "deeplx" => true, // DeepLX always available (bundled free endpoint)
            _ => false,
        }
    }
}
