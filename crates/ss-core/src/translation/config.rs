//! User translation preferences.
//!
//! The engine, surface switches, translation style, and LLM endpoint live in
//! `config/translation.json`. Both switches start off.
//! The API key is a credential and lives under `secrets/`, not next to them.

use anyhow::Result;
use serde::{Deserialize, Serialize};

use super::themes::{self, DEFAULT_READER};

pub const DEFAULT_LLM_URL: &str = "https://api.openai.com/v1";
pub const DEFAULT_TARGET: &str = "zh-CN";

/// Languages a translation can be written in. Labels are endonyms.
pub struct TranslationLanguage {
    pub code: &'static str,
    pub label: &'static str,
    /// Name given to the model. Google receives `code`.
    pub prompt: &'static str,
}

pub const TRANSLATION_LANGUAGES: &[TranslationLanguage] = &[
    TranslationLanguage {
        code: "zh-CN",
        label: "简体中文",
        prompt: "Simplified Chinese",
    },
    TranslationLanguage {
        code: "zh-TW",
        label: "繁體中文",
        prompt: "Traditional Chinese",
    },
    TranslationLanguage {
        code: "en",
        label: "English",
        prompt: "English",
    },
    TranslationLanguage {
        code: "ja",
        label: "日本語",
        prompt: "Japanese",
    },
    TranslationLanguage {
        code: "ko",
        label: "한국어",
        prompt: "Korean",
    },
];

pub fn translation_language(code: &str) -> Option<&'static TranslationLanguage> {
    TRANSLATION_LANGUAGES
        .iter()
        .find(|language| language.code == code)
}

pub fn canonical_target(code: &str) -> &'static str {
    translation_language(code)
        .map(|language| language.code)
        .unwrap_or(DEFAULT_TARGET)
}

fn default_target() -> String {
    DEFAULT_TARGET.to_string()
}

/// Account families whose stored API key can drive translation.
///
/// Each one speaks OpenAI chat completions. Login sessions are not used.
pub struct LlmAccountProvider {
    pub catalog_id: &'static str,
    pub base_url: &'static str,
    /// Recommended model. Used until the user pins one from a fetched list.
    pub model_hint: &'static str,
}

pub const LLM_ACCOUNT_PROVIDERS: &[LlmAccountProvider] = &[
    LlmAccountProvider {
        catalog_id: "opencode-go",
        base_url: "https://opencode.ai/zen/go/v1",
        model_hint: "deepseek-v4.1-flash",
    },
    LlmAccountProvider {
        catalog_id: "ollama",
        base_url: "https://ollama.com/v1",
        model_hint: "gemma4:31b",
    },
    LlmAccountProvider {
        catalog_id: "command-code",
        base_url: "https://api.commandcode.ai/provider/v1",
        model_hint: "deepseek/deepseek-v4-flash",
    },
];

pub fn llm_account(catalog_id: &str) -> Option<&'static LlmAccountProvider> {
    LLM_ACCOUNT_PROVIDERS
        .iter()
        .find(|provider| provider.catalog_id == catalog_id)
}

/// Resolved endpoint and bearer token for one LLM request.
///
/// The key stays out of `Debug` so a log of this value cannot print it.
#[derive(Clone, Default)]
pub struct LlmAuth {
    pub base_url: String,
    pub api_key: String,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Engine {
    #[default]
    Machine,
    Llm,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct TranslationConfig {
    pub engine: Engine,
    /// English descriptions on skill cards, the detail column, and the market.
    /// Off until the user turns it on. Cached lines stay on disk either way.
    pub translate_descriptions: bool,
    /// English paragraphs in the SKILL.md reader.
    pub translate_skill_md: bool,
    /// Style for a translated description and for the line under a paragraph.
    pub reader_theme: String,
    /// Language the translation is written in. Missing or unknown values are Simplified Chinese.
    #[serde(default = "default_target")]
    pub target_lang: String,
    pub llm_base_url: String,
    /// Model id sent to chat completions. While `llm_model_pinned` is false
    /// and an account is selected, load rewrites this to that service's
    /// `model_hint`.
    pub llm_model: String,
    /// The user picked this id from a fetched model list.
    pub llm_model_pinned: bool,
    /// Subscription id whose API key is used. Empty means the typed key.
    pub llm_account_id: String,
    /// `opencode-go`, `ollama`, or `command-code`. Empty with a manual key.
    pub llm_account_catalog: String,
}

impl Default for TranslationConfig {
    fn default() -> Self {
        Self {
            engine: Engine::Machine,
            translate_descriptions: false,
            translate_skill_md: false,
            reader_theme: DEFAULT_READER.to_string(),
            target_lang: DEFAULT_TARGET.to_string(),
            llm_base_url: DEFAULT_LLM_URL.to_string(),
            llm_model: String::new(),
            llm_model_pinned: false,
            llm_account_id: String::new(),
            llm_account_catalog: String::new(),
        }
    }
}

impl TranslationConfig {
    pub fn normalized(mut self) -> Self {
        self.reader_theme = themes::canonical_reader(&self.reader_theme).to_string();
        self.target_lang = canonical_target(&self.target_lang).to_string();
        if self.llm_base_url.trim().is_empty() {
            self.llm_base_url = DEFAULT_LLM_URL.to_string();
        }
        self.llm_model = self.llm_model.trim().to_string();
        self.llm_account_id = self.llm_account_id.trim().to_string();
        self.llm_account_catalog = self.llm_account_catalog.trim().to_string();
        if let Some(provider) = llm_account(&self.llm_account_catalog)
            && !self.llm_account_id.is_empty()
        {
            if !self.llm_model_pinned {
                self.llm_model = provider.model_hint.to_string();
            }
        } else {
            self.llm_account_id.clear();
            self.llm_account_catalog.clear();
            self.llm_model_pinned = false;
        }
        self
    }

    /// Chat-completions root. An accepted account wins over the typed URL.
    pub fn llm_base(&self) -> String {
        if !self.llm_account_id.is_empty()
            && let Some(provider) = llm_account(&self.llm_account_catalog)
        {
            return provider.base_url.trim_end_matches('/').to_string();
        }
        self.llm_base_url.trim().trim_end_matches('/').to_string()
    }

    /// Cache identity. The API key is not part of it.
    pub fn scope(&self, target: &str) -> String {
        match self.engine {
            Engine::Machine => format!("machine|{target}"),
            Engine::Llm => format!("llm|{}|{}|{target}", self.llm_base(), self.llm_model.trim()),
        }
    }
}

struct CachedConfig {
    root: std::path::PathBuf,
    config: TranslationConfig,
}

static CACHED: std::sync::Mutex<Option<CachedConfig>> = std::sync::Mutex::new(None);

fn read_config_file() -> Result<TranslationConfig> {
    let path = crate::infra::paths::translation_config_path();
    if !path.exists() {
        return Ok(TranslationConfig::default());
    }
    let content = std::fs::read_to_string(&path)?;
    let config: TranslationConfig = serde_json::from_str(&content).unwrap_or_default();
    Ok(config.normalized())
}

pub fn load_config() -> Result<TranslationConfig> {
    let root = crate::infra::paths::data_root();
    let mut guard = CACHED
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    if let Some(cached) = guard.as_ref()
        && cached.root == root
    {
        return Ok(cached.config.clone());
    }
    let config = read_config_file()?;
    *guard = Some(CachedConfig {
        root,
        config: config.clone(),
    });
    Ok(config)
}

pub fn save_config(config: &TranslationConfig) -> Result<()> {
    let config = config.clone().normalized();
    let content = serde_json::to_string_pretty(&config)?;
    crate::infra::fs_ops::atomic_write(
        &crate::infra::paths::translation_config_path(),
        content.as_bytes(),
    )?;
    let root = crate::infra::paths::data_root();
    let mut guard = CACHED
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    *guard = Some(CachedConfig { root, config });
    Ok(())
}

pub fn load_api_key() -> String {
    let path = crate::infra::paths::translation_api_key_path();
    std::fs::read_to_string(path)
        .unwrap_or_default()
        .trim()
        .to_string()
}

pub fn save_api_key(key: &str) -> Result<()> {
    let path = crate::infra::paths::translation_api_key_path();
    crate::infra::fs_ops::atomic_write(&path, key.trim().as_bytes())?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{Engine, TranslationConfig, load_api_key, load_config, save_api_key, save_config};
    use tempfile::TempDir;

    fn isolated() -> (std::sync::MutexGuard<'static, ()>, TempDir) {
        let guard = crate::config::test_env_lock()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let temp = TempDir::new().unwrap();
        unsafe {
            std::env::set_var("SKILLSTAR_DATA_DIR", temp.path());
        }
        (guard, temp)
    }

    #[test]
    fn old_card_theme_is_ignored_and_reader_theme_stays() {
        let config: TranslationConfig =
            serde_json::from_str(r#"{"reader_theme":"wavy","card_theme":"hi"}"#).unwrap();
        let config = config.normalized();
        assert_eq!(config.reader_theme, "wavy");
    }

    #[test]
    fn old_config_without_surface_flags_stays_off() {
        let config: TranslationConfig = serde_json::from_str(r#"{"engine":"machine"}"#).unwrap();
        assert!(!config.translate_descriptions);
        assert!(!config.translate_skill_md);
        assert_eq!(config.engine, Engine::Machine);
        assert_eq!(config.target_lang, "zh-CN");
    }

    #[test]
    fn unknown_target_falls_back_to_simplified_chinese() {
        let config: TranslationConfig =
            serde_json::from_str(r#"{"target_lang":"klingon"}"#).unwrap();
        assert_eq!(config.normalized().target_lang, "zh-CN");
    }

    #[test]
    fn missing_config_is_machine_translation() {
        let (_guard, _temp) = isolated();
        let config = load_config().unwrap();
        assert_eq!(config.engine, Engine::Machine);
        assert!(!config.translate_descriptions);
        assert!(!config.translate_skill_md);
        assert_eq!(config.reader_theme, "quote");
        assert_eq!(config.target_lang, "zh-CN");
        assert!(load_api_key().is_empty());
        unsafe {
            std::env::remove_var("SKILLSTAR_DATA_DIR");
        }
    }

    #[test]
    fn save_roundtrip_keeps_engine_themes_and_key() {
        let (_guard, _temp) = isolated();
        let config = TranslationConfig {
            engine: Engine::Llm,
            translate_descriptions: true,
            translate_skill_md: true,
            reader_theme: "not-a-theme".into(),
            llm_base_url: "https://example.test/v1/".into(),
            llm_model: "demo".into(),
            ..TranslationConfig::default()
        };
        save_config(&config).unwrap();
        save_api_key("secret-key").unwrap();
        let loaded = load_config().unwrap();
        assert_eq!(loaded.engine, Engine::Llm);
        assert!(loaded.translate_descriptions);
        assert!(loaded.translate_skill_md);
        assert_eq!(loaded.reader_theme, "quote");
        assert_eq!(loaded.llm_model, "demo");
        assert_eq!(load_api_key(), "secret-key");
        assert_eq!(
            loaded.scope("zh-CN"),
            "llm|https://example.test/v1|demo|zh-CN"
        );
        unsafe {
            std::env::remove_var("SKILLSTAR_DATA_DIR");
        }
    }

    #[test]
    fn account_scope_uses_the_provider_endpoint() {
        let config = TranslationConfig {
            engine: Engine::Llm,
            llm_account_id: "sub-1".into(),
            llm_account_catalog: "opencode-go".into(),
            llm_base_url: "https://api.openai.com/v1".into(),
            llm_model: "kimi-k2.6".into(),
            llm_model_pinned: true,
            ..TranslationConfig::default()
        }
        .normalized();
        assert_eq!(config.llm_model, "kimi-k2.6");
        assert_eq!(
            config.scope("zh-CN"),
            "llm|https://opencode.ai/zen/go/v1|kimi-k2.6|zh-CN"
        );
        let rejected = TranslationConfig {
            llm_account_id: "sub".into(),
            llm_account_catalog: "codex".into(),
            ..TranslationConfig::default()
        }
        .normalized();
        assert!(rejected.llm_account_id.is_empty());
        assert!(rejected.llm_account_catalog.is_empty());
        assert!(!rejected.llm_model_pinned);
    }

    #[test]
    fn unpinned_opencode_go_uses_deepseek_v4_1_flash() {
        let config = TranslationConfig {
            engine: Engine::Llm,
            llm_account_id: "sub-1".into(),
            llm_account_catalog: "opencode-go".into(),
            llm_model: "kimi-k2.6".into(),
            ..TranslationConfig::default()
        }
        .normalized();
        assert!(!config.llm_model_pinned);
        assert_eq!(config.llm_model, "deepseek-v4.1-flash");
        assert_eq!(
            config.scope("zh-CN"),
            "llm|https://opencode.ai/zen/go/v1|deepseek-v4.1-flash|zh-CN"
        );

        let old: TranslationConfig = serde_json::from_str(
            r#"{"engine":"llm","llm_account_id":"sub-1","llm_account_catalog":"opencode-go","llm_model":"kimi-k2.6"}"#,
        )
        .unwrap();
        assert_eq!(old.normalized().llm_model, "deepseek-v4.1-flash");
    }
}
