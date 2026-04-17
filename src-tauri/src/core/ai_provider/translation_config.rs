//! Standalone translation API config (separate file to avoid parse cascade
//! when impl blocks in this module call parent module helpers).

use serde::{Deserialize, Serialize};

/// Configuration for all traditional translation APIs (DeepL, Google, Azure, GTX)
/// and AI LLM translation providers.
/// Keys are stored encrypted at rest in ai.json.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TranslationApiConfig {
    // DeepL (official paid)
    #[serde(default)]
    pub deepl_key: String,
    // DeepLX (community free endpoint — set via settings UI)
    #[serde(default)]
    pub deeplx_url: String,
    // Google Cloud Translate v3
    #[serde(default)]
    pub google_key: String,
    // Azure Translator
    #[serde(default)]
    pub azure_key: String,
    #[serde(default)]
    pub azure_region: String,
    // GTX API (free tier)
    #[serde(default)]
    pub gtx_api_key: String,
    // LLM providers
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
    #[serde(default)]
    pub custom_llm_base_url: String,
    // Behaviour
    /// Ordered list of provider names to try (e.g. ["deepl", "google", "azure"]).
    /// Only providers with non-empty keys are attempted.
    #[serde(default)]
    pub enabled_providers: Vec<String>,
    /// Default provider for short text translation.
    #[serde(default = "default_translation_default_provider")]
    pub default_provider: String,
    /// Default provider for SKILL.md full-document translation.
    #[serde(default = "default_translation_default_skill_provider")]
    pub default_skill_provider: String,
}

fn default_translation_default_provider() -> String {
    "deepl".to_string()
}

fn default_translation_default_skill_provider() -> String {
    "deepseek".to_string()
}

// ── Encryption helpers (standalone in this file to avoid parser cascade) ──

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
    use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};

    if plain.is_empty() {
        return String::new();
    }
    let key = get_encryption_key();
    let cipher = Aes256Gcm::new(&key);
    let nonce = Aes256Gcm::generate_nonce(&mut OsRng);
    match cipher.encrypt(&nonce, plain.as_bytes()) {
        Ok(ciphertext) => {
            let mut combined = nonce.to_vec();
            combined.extend_from_slice(&ciphertext);
            BASE64.encode(combined)
        }
        Err(_) => plain.to_string(),
    }
}

fn decrypt_api_key(encoded: &str) -> String {
    use aes_gcm::{aead::Aead, Aes256Gcm, KeyInit, Nonce};
    use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};

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

impl TranslationApiConfig {
    /// Decrypt all encrypted API key fields in-place.
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

    /// Encrypt all API key fields in-place (call before serializing to disk).
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
}
