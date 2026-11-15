//! Translation facade — the read-side bits the market surfaces need
//! (wave 2 base), plus the description-choice write and the live engine
//! call the market detail-drawer slice owns (its `translate.rs` parity,
//! added by that slice inside this file).
//!
//! The ss-gpui side keeps its UI-layer cache/scheduler in
//! `crates/ss-gpui/src/translation.rs`; the domain half lives in
//! `ss_core::translation`, and that is what this file exposes. The
//! session-level bookkeeping the GPUI scheduler keeps process-side
//! (inflight dedupe, failure marks, the model-rejection slot) stays in the
//! TSX drawer module: the facade answers one lookup / one fill at a time.

use nsh_core::prelude::*;
use nsh_core::JsonValue;

use nsh_core::element::__private::JsonMap;

/// The translation seam (`MarketplaceTranslation` in TSX).
#[module]
pub struct MarketplaceTranslation;

#[module]
impl MarketplaceTranslation {
    /// The language translations are written in (`active_target` — a
    /// canonical BCP-47 tag, default `zh-CN`). Pure read of the loaded
    /// config; read-only.
    pub fn active_target() -> String {
        ss_core::translation::active_target().to_owned()
    }

    /// Whether the description surface's Settings switch is on
    /// (`TranslationConfig.translate_descriptions`: skill cards, the detail
    /// column, and the market). Read-only.
    pub fn descriptions_enabled() -> bool {
        ss_core::translation::load_config()
            .map(|config| config.translate_descriptions)
            .unwrap_or(false)
    }

    /// The stored per-skill description choice from the detail column's
    /// language button (`TranslationConfig.description_choices`, keyed by
    /// skill name): `true` / `false` when the skill chose, `null` when it
    /// follows the surface switch. Read-only.
    pub fn description_choice(skill: String) -> Option<bool> {
        ss_core::translation::load_config()
            .ok()
            .and_then(|config| config.description_choices.get(&skill).copied())
    }

    /// Remember one button press — the drawer's translate button writes the
    /// choice for this opening (`set_description_choice`,
    /// translate.rs `apply_description_choice`); the card and the column
    /// keep that language across restarts. The "turn a rejected model back
    /// on" half is client-side session state (the TSX module drops it).
    pub fn set_description_choice(skill: String, translated: bool) -> Result<(), String> {
        ss_core::translation::set_description_choice(&skill, translated)
            .map_err(|err| format!("{err:#}"))
    }

    /// The cached translation of one source in the active target, when it
    /// exists (`cache::lookup`). Read-only; a miss means "not translated
    /// yet", never "untranslatable" — pair with `needs_translation`.
    pub fn lookup(source: String) -> Option<String> {
        ss_core::translation::lookup(&source, ss_core::translation::active_target())
    }

    /// Whether the active target would translate this text at all
    /// (`needs_translation`): copy already in the target (Chinese for
    /// zh-CN) resolves false and the drawer shows no language button.
    /// Read-only, pure.
    pub fn needs_translation(text: String) -> bool {
        ss_core::translation::needs_translation(&text, ss_core::translation::active_target())
    }

    /// Whether the LLM engine is selected but cannot run (no model or no
    /// key) — the GPUI `blocked_hint` gate: the translate button stays
    /// quiet instead of spinning forever. Read-only.
    ///
    /// Divergence from subscription-account auth (this crate does not query
    /// `ss-accounts` for LLM keys): the key check reads the translation key
    /// file only, so an account-only setup reports blocked.
    pub fn descriptions_blocked() -> bool {
        let Ok(config) = ss_core::translation::load_config() else {
            return false;
        };
        if config.engine != ss_core::translation::Engine::Llm {
            return false;
        }
        config.llm_model.trim().is_empty()
            || ss_core::translation::load_api_key().trim().is_empty()
    }

    /// The live engine call for the drawer's translate button
    /// (`translation::fill`): translate `sources` into the active target,
    /// writing every success into the disk cache before returning.
    /// Resolves `{done: [{source, text}], rejection: {model, detail} |
    /// null}` — `rejection` is the model's HTTP-400 refusal; the shell
    /// surfaces it once and the next button press clears the session slot
    /// (retry_rejected parity, client-side).
    ///
    /// Same subscription-account divergence as `descriptions_blocked`: the LLM
    /// auth carries the typed key file, not a subscription account key.
    pub async fn fill_descriptions(sources: Vec<String>) -> Result<JsonValue, String> {
        let config = ss_core::translation::load_config().map_err(|err| format!("{err:#}"))?;
        let target = ss_core::translation::active_target().to_owned();
        let auth = ss_core::translation::LlmAuth {
            base_url: config.llm_base(),
            api_key: ss_core::translation::load_api_key(),
        };
        let report = ss_core::translation::fill(sources, &target, auth).await;
        let mut map = JsonMap::new();
        map.insert(
            "done".to_owned(),
            JsonValue::Array(
                report
                    .done
                    .iter()
                    .map(|item| {
                        let mut entry = JsonMap::new();
                        entry.insert("source".to_owned(), JsonValue::String(item.source.clone()));
                        entry.insert("text".to_owned(), JsonValue::String(item.text.clone()));
                        JsonValue::Object(entry)
                    })
                    .collect(),
            ),
        );
        map.insert(
            "rejection".to_owned(),
            match &report.rejection {
                Some(reject) => {
                    let mut entry = JsonMap::new();
                    entry.insert("model".to_owned(), JsonValue::String(reject.model.clone()));
                    entry.insert("detail".to_owned(), JsonValue::String(reject.detail.clone()));
                    JsonValue::Object(entry)
                }
                None => JsonValue::Null,
            },
        );
        Ok(JsonValue::Object(map))
    }
}

#[cfg(test)]
mod tests {
    use super::MarketplaceTranslation;

    /// The GPUI suites' IsolatedDataDir pattern (facades/mod.rs): re-root
    /// `SKILLSTAR_DATA_DIR` for the duration, borrowing the shared
    /// TEST_ENV_LOCK so every facade suite that flips the variable
    /// serializes against each other (racing threads abort the binary).
    struct IsolatedDataDir {
        _lock: std::sync::MutexGuard<'static, ()>,
        previous: Option<std::ffi::OsString>,
        root: std::path::PathBuf,
    }

    impl IsolatedDataDir {
        fn new() -> Self {
            let lock = crate::facades::TEST_ENV_LOCK
                .lock()
                .unwrap_or_else(|p| p.into_inner());
            let root = std::env::temp_dir().join(format!(
                "skillstar-nsh-translation-{}-{}",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_nanos()
            ));
            std::fs::create_dir_all(&root).unwrap();
            let previous = std::env::var_os("SKILLSTAR_DATA_DIR");
            unsafe { std::env::set_var("SKILLSTAR_DATA_DIR", &root) };
            Self { _lock: lock, previous, root }
        }
    }

    impl Drop for IsolatedDataDir {
        fn drop(&mut self) {
            unsafe {
                match self.previous.take() {
                    Some(value) => std::env::set_var("SKILLSTAR_DATA_DIR", value),
                    None => std::env::remove_var("SKILLSTAR_DATA_DIR"),
                }
            }
            let _ = std::fs::remove_dir_all(&self.root);
        }
    }

    /// The drawer button's write/read round trip: `set_description_choice`
    /// lands in the config the read side (`description_choice`) and the
    /// page's `resolved_translate` read from.
    #[test]
    fn description_choice_round_trips_through_the_config() {
        let _iso = IsolatedDataDir::new();
        assert_eq!(MarketplaceTranslation::description_choice("demo".into()), None);
        MarketplaceTranslation::set_description_choice("demo".into(), true).unwrap();
        assert_eq!(
            MarketplaceTranslation::description_choice("demo".into()),
            Some(true)
        );
        // A second skill is untouched by the first one's choice.
        assert_eq!(
            MarketplaceTranslation::description_choice("other".into()),
            None
        );
    }

    /// The language-button gate: copy already in the target offers no
    /// translation (needs_translation false), and an unknown cache line
    /// stays a miss. Pure / disk-only, no network.
    #[test]
    fn needs_translation_gates_the_button_and_misses_stay_misses() {
        let _iso = IsolatedDataDir::new();
        // The isolated dir defaults to zh-CN.
        assert!(MarketplaceTranslation::needs_translation(
            "Build deep modules for agents.".into()
        ));
        assert!(!MarketplaceTranslation::needs_translation(
            "为代理构建深模块。".into()
        ));
        assert_eq!(
            MarketplaceTranslation::lookup("never-cached-line".into()),
            None
        );
    }

    /// The fill call's wire shape with a source the target will not
    /// translate: nothing to ask the engine, so the call resolves with an
    /// empty done list and no rejection — no network in this test. Domain
    /// futures here finish on the first poll (the `spin` the deck facade
    /// tests use); `fill` with an empty pending batch does exactly that.
    #[test]
    fn fill_skips_copy_already_in_the_target() {
        let _iso = IsolatedDataDir::new();
        let mut future = std::pin::pin!(MarketplaceTranslation::fill_descriptions(
            vec!["为代理构建深模块。".to_owned()]
        ));
        let waker = std::task::Waker::noop();
        let mut cx = std::task::Context::from_waker(waker);
        let report = match future.as_mut().poll(&mut cx) {
            std::task::Poll::Ready(value) => {
                value.expect("fill over an isolated dir")
            }
            std::task::Poll::Pending => unreachable!("an empty batch finishes on first poll"),
        };
        assert_eq!(report["done"].as_array().map(Vec::len), Some(0));
        assert!(report["rejection"].is_null());
    }

    /// The machine engine (the default) is never blocked; the blocked gate
    /// only speaks for the LLM engine (translate.rs blocked_hint parity).
    #[test]
    fn the_default_engine_is_never_blocked() {
        let _iso = IsolatedDataDir::new();
        assert!(!MarketplaceTranslation::descriptions_blocked());
    }
}
