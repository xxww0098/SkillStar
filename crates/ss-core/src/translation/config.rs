//! User translation preferences.
//!
//! The engine, surface switches, translation style, and LLM endpoint live in
//! `config/translation.json`. Both switches start on, so a fresh install
//! translates descriptions and SKILL.md prose without a first visit to
//! Settings (machine engine, Simplified Chinese).
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
    /// On by default; turning it off only hides the lines (cached lines stay
    /// on disk either way).
    pub translate_descriptions: bool,
    /// English paragraphs in the SKILL.md reader. On by default.
    pub translate_skill_md: bool,
    /// Per-skill description-translation choice, keyed by skill name. The
    /// detail column's language button and the selection bar's translate
    /// action both write here. A skill not in the map follows the
    /// `translate_descriptions` switch; one in the map keeps its line in the
    /// chosen language across restarts.
    pub description_choices: std::collections::HashMap<String, bool>,
    /// Style for a translated description and for the line under a paragraph.
    pub reader_theme: String,
    /// Language the translation is written in. Missing or unknown values are Simplified Chinese.
    #[serde(default = "default_target")]
    pub target_lang: String,
    pub llm_base_url: String,
    /// Model id sent to chat completions (entered manually or picked from the
    /// fetched `/models` list in Settings → Translation).
    pub llm_model: String,
    /// Whether the user picked this model id from a fetched model list.
    pub llm_model_pinned: bool,
    /// Legacy account-binding field retained for config backward compatibility;
    /// cleared unconditionally by [`TranslationConfig::normalized`].
    pub llm_account_id: String,
    /// Legacy account-catalog field retained for config backward compatibility;
    /// cleared unconditionally by [`TranslationConfig::normalized`].
    pub llm_account_catalog: String,
}

impl Default for TranslationConfig {
    fn default() -> Self {
        Self {
            engine: Engine::Machine,
            translate_descriptions: true,
            translate_skill_md: true,
            description_choices: std::collections::HashMap::new(),
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
        self.llm_account_id.clear();
        self.llm_account_catalog.clear();
        self
    }

    /// Chat-completions root.
    pub fn llm_base(&self) -> String {
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
static API_KEY_CACHE: std::sync::Mutex<Option<(std::path::PathBuf, String)>> =
    std::sync::Mutex::new(None);

fn read_config_file() -> Result<TranslationConfig> {
    let path = crate::infra::paths::translation_config_path();
    if !path.exists() {
        return Ok(TranslationConfig::default());
    }
    let content = std::fs::read_to_string(&path)?;
    let config: TranslationConfig = serde_json::from_str(&content).unwrap_or_default();
    Ok(config.normalized())
}

fn with_cached_config<T>(f: impl FnOnce(&TranslationConfig) -> T) -> Result<T> {
    let root = crate::infra::paths::data_root();
    let mut guard = CACHED
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    if let Some(cached) = guard.as_ref()
        && cached.root == root
    {
        return Ok(f(&cached.config));
    }
    let config = read_config_file()?;
    let out = f(&config);
    *guard = Some(CachedConfig { root, config });
    Ok(out)
}

#[cfg(not(test))]
fn config_writer_tx() -> &'static std::sync::mpsc::Sender<(std::path::PathBuf, Vec<u8>)> {
    static TX: std::sync::OnceLock<std::sync::mpsc::Sender<(std::path::PathBuf, Vec<u8>)>> =
        std::sync::OnceLock::new();
    TX.get_or_init(|| {
        let (tx, rx) = std::sync::mpsc::channel::<(std::path::PathBuf, Vec<u8>)>();
        let _ = std::thread::Builder::new()
            .name("ss-translation-config-writer".to_string())
            .spawn(move || {
                while let Ok((mut path, mut bytes)) = rx.recv() {
                    while let Ok((next_path, next_bytes)) = rx.try_recv() {
                        if next_path == path {
                            bytes = next_bytes;
                        } else {
                            if let Err(err) = crate::infra::fs_ops::atomic_write(&path, &bytes) {
                                tracing::warn!("failed to write translation config: {err}");
                            }
                            path = next_path;
                            bytes = next_bytes;
                        }
                    }
                    if let Err(err) = crate::infra::fs_ops::atomic_write(&path, &bytes) {
                        tracing::warn!("failed to write translation config: {err}");
                    }
                }
            });
        tx
    })
}

fn persist_config_locked(root: std::path::PathBuf, config: TranslationConfig) -> Result<()> {
    let content = serde_json::to_string_pretty(&config)?;
    let path = crate::infra::paths::translation_config_path();
    let mut guard = CACHED
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    *guard = Some(CachedConfig { root, config });
    #[cfg(not(test))]
    {
        let bytes = content.into_bytes();
        if let Err(err) = config_writer_tx().send((path.clone(), bytes)) {
            let (_, bytes) = err.0;
            crate::infra::fs_ops::atomic_write(&path, &bytes)?;
        }
        Ok(())
    }
    #[cfg(test)]
    {
        crate::infra::fs_ops::atomic_write(&path, content.as_bytes())?;
        Ok(())
    }
}

pub fn load_config() -> Result<TranslationConfig> {
    with_cached_config(Clone::clone)
}

pub(super) fn scope_for_active(target: &str) -> String {
    with_cached_config(|cfg| cfg.scope(target))
        .unwrap_or_else(|_| TranslationConfig::default().scope(target))
}

pub(super) fn active_target_code() -> &'static str {
    with_cached_config(|cfg| canonical_target(&cfg.target_lang)).unwrap_or(DEFAULT_TARGET)
}

pub fn descriptions_enabled() -> bool {
    with_cached_config(|cfg| cfg.translate_descriptions).unwrap_or(false)
}

pub fn description_choice(skill: &str) -> Option<bool> {
    with_cached_config(|cfg| cfg.description_choices.get(skill).copied())
        .ok()
        .flatten()
}

pub fn descriptions_blocked() -> bool {
    let Ok((is_llm, model_empty)) = with_cached_config(|cfg| {
        (cfg.engine == Engine::Llm, cfg.llm_model.trim().is_empty())
    }) else {
        return false;
    };
    if !is_llm {
        return false;
    }
    model_empty || load_api_key().trim().is_empty()
}

pub fn save_config(config: &TranslationConfig) -> Result<()> {
    let config = config.clone().normalized();
    let root = crate::infra::paths::data_root();
    persist_config_locked(root, config)
}

/// Remember one skill's description-translation choice, so the card and the
/// drawer keep that language across restarts.
pub fn set_description_choice(skill: &str, translated: bool) -> Result<()> {
    set_description_choices([(skill, translated)])
}

/// Remember several description-translation choices in one write.
/// An empty name is skipped. A choice that is already stored is left as it is.
pub fn set_description_choices<'a>(
    choices: impl IntoIterator<Item = (&'a str, bool)>,
) -> Result<()> {
    let root = crate::infra::paths::data_root();
    let mut config = {
        let mut guard = CACHED
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if let Some(cached) = guard.as_ref()
            && cached.root == root
        {
            cached.config.clone()
        } else {
            let loaded = read_config_file()?;
            *guard = Some(CachedConfig {
                root: root.clone(),
                config: loaded.clone(),
            });
            loaded
        }
    };
    let mut changed = false;
    for (skill, translated) in choices {
        if skill.is_empty() || config.description_choices.get(skill) == Some(&translated) {
            continue;
        }
        config
            .description_choices
            .insert(skill.to_string(), translated);
        changed = true;
    }
    if changed {
        persist_config_locked(root, config)?;
    }
    Ok(())
}

pub fn load_api_key() -> String {
    let root = crate::infra::paths::data_root();
    let mut guard = API_KEY_CACHE
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    if let Some((cached_root, cached_key)) = guard.as_ref()
        && *cached_root == root
    {
        return cached_key.clone();
    }
    let path = crate::infra::paths::translation_api_key_path();
    let key = std::fs::read_to_string(path)
        .unwrap_or_default()
        .trim()
        .to_string();
    *guard = Some((root, key.clone()));
    key
}

pub fn save_api_key(key: &str) -> Result<()> {
    let path = crate::infra::paths::translation_api_key_path();
    let trimmed = key.trim();
    crate::infra::fs_ops::atomic_write(&path, trimmed.as_bytes())?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600));
    }
    let root = crate::infra::paths::data_root();
    let mut guard = API_KEY_CACHE
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    *guard = Some((root, trimmed.to_string()));
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{
        Engine, TranslationConfig, load_api_key, load_config, save_api_key, save_config,
        set_description_choice, set_description_choices,
    };
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
    fn old_config_without_surface_flags_takes_the_on_default() {
        let config: TranslationConfig = serde_json::from_str(r#"{"engine":"machine"}"#).unwrap();
        assert!(config.translate_descriptions);
        assert!(config.translate_skill_md);
        assert!(config.description_choices.is_empty());
        assert_eq!(config.engine, Engine::Machine);
        assert_eq!(config.target_lang, "zh-CN");
    }

    #[test]
    fn an_explicit_off_survives_a_reload() {
        let config: TranslationConfig = serde_json::from_str(
            r#"{"engine":"machine","translate_descriptions":false,"translate_skill_md":false}"#,
        )
        .unwrap();
        assert!(
            !config.translate_descriptions && !config.translate_skill_md,
            "the user's own off must win over the on default"
        );
        let config = config.normalized();
        assert!(!config.translate_descriptions);
        assert!(!config.translate_skill_md);
    }

    #[test]
    fn description_choice_survives_save_and_reload() {
        let (_guard, _temp) = isolated();
        set_description_choice("demo", true).unwrap();
        set_description_choice("other", false).unwrap();
        // A later choice for the same skill replaces the earlier one.
        set_description_choice("demo", false).unwrap();
        let loaded = load_config().unwrap();
        assert_eq!(
            loaded.description_choices.get("demo"),
            Some(&false),
            "the last choice for a skill must win"
        );
        assert_eq!(loaded.description_choices.get("other"), Some(&false));
        // A full save from a loaded config keeps the choices.
        save_config(&loaded).unwrap();
        assert_eq!(
            load_config().unwrap().description_choices,
            loaded.description_choices
        );
        unsafe {
            std::env::remove_var("SKILLSTAR_DATA_DIR");
        }
    }

    #[test]
    fn several_description_choices_save_together_and_keep_the_rest() {
        let (_guard, _temp) = isolated();
        set_description_choice("kept", false).unwrap();
        set_description_choices([("alpha", true), ("", true), ("kept", false)]).unwrap();
        let loaded = load_config().unwrap();
        assert_eq!(loaded.description_choices.get("alpha"), Some(&true));
        assert_eq!(
            loaded.description_choices.get("kept"),
            Some(&false),
            "an unchanged choice must stay, and an empty name must not be stored"
        );
        assert!(!loaded.description_choices.contains_key(""));
        unsafe {
            std::env::remove_var("SKILLSTAR_DATA_DIR");
        }
    }

    #[test]
    fn unknown_target_falls_back_to_simplified_chinese() {
        let config: TranslationConfig =
            serde_json::from_str(r#"{"target_lang":"klingon"}"#).unwrap();
        assert_eq!(config.normalized().target_lang, "zh-CN");
    }

    #[test]
    fn missing_config_is_machine_translation_with_both_surfaces_on() {
        let (_guard, _temp) = isolated();
        let config = load_config().unwrap();
        assert_eq!(config.engine, Engine::Machine);
        assert!(config.translate_descriptions);
        assert!(config.translate_skill_md);
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
}
