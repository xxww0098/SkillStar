//! SKILL.MD READER facade (`my_skills/skill_reader.rs` + `reader_style.rs`
//! slice). Registered empty by the wave-3 page base; this slice adds the
//! `#[module]` methods and surfaces them in TSX as `MySkillsReader.*`.
//!
//! Two seams meet here:
//!
//! * the reader's file read — skill_reader.rs `Origin::Installed(name)`
//!   spawns `ss_skills::content::read` on the blocking pool (:110-133); this
//!   facade does the same through `nsh_core`'s `blocking`;
//! * the SKILL.md translation surface — the GPUI shell's process-side
//!   scheduler (`crates/ss-gpui/src/translation.rs`) answers one lookup /
//!   one fill at a time; here `lookup` / `needs_translation` / `fill` carry
//!   the disk-cache half and the TSX module keeps the session bookkeeping
//!   (the marketplace drawer's translate.ts precedent). The surface switch
//!   is `TranslationConfig.translate_skill_md` (translation.rs
//!   `enabled(Surface::SkillMd)`), which the reader only ever READS: the
//!   opening's translate button never writes it.
//!
//! The saved translation style (`reader_style.rs`) is config-in, config-out:
//! `reader_themes` lists the menu, `reader_theme` reads the canonicalized
//! choice, `set_reader_theme` writes it.
//!
//! The Markdown rendering itself is a TSX concern (nsh `TextView` with the
//! spike's GPUI-mapped styles; no restricted-subset renderer needed — the
//! spike verdict). Page mount: `src/pages/my_skills/skillReaderSlot.tsx`.

use nsh_core::prelude::*;
use nsh_core::JsonValue;

use nsh_core::element::__private::JsonMap;

/// The skill-reader seam (`MySkillsReader` in TSX).
#[module]
pub struct MySkillsReader;

#[module]
impl MySkillsReader {
    /// Read one installed skill's SKILL.md (skill_reader.rs
    /// `Origin::Installed` → `ss_skills::content::read`, returning
    /// `content.content`). A cheap local read — one SKILL.md plus the
    /// lockfile lookups `resolve_content_dir` does — so it runs inline
    /// (the my-skills facade's "cheap local reads run inline" precedent;
    /// the GPUI `spawn_blocking` is about its single UI thread, not the
    /// work's size). Err carries the domain chain for the reader's failure
    /// card + retry.
    pub fn read(name: String) -> Result<String, String> {
        ss_skills::content::read(&name)
            .map(|content| content.content)
            .map_err(|err| format!("{err:#}"))
    }

    /// The SKILL.md surface switch the reader opens with
    /// (`TranslationConfig.translate_skill_md`; translation.rs
    /// `enabled(Surface::SkillMd)`, skill_reader.rs `start` :97-102).
    /// Read-only — the reader's translate button never writes this.
    pub fn skill_md_enabled() -> bool {
        ss_core::translation::load_config()
            .map(|config| config.translate_skill_md)
            .unwrap_or(false)
    }

    /// The cached translation of one prose block in the active target
    /// (`translation.rs display_when` → `cache::lookup`). Read-only; a miss
    /// means "not translated yet", never "untranslatable".
    pub fn lookup(source: String) -> Option<String> {
        ss_core::translation::lookup(&source, ss_core::translation::active_target())
    }

    /// Whether the active target would translate this text
    /// (`needs_translation`) — the translate button gate (skill_reader.rs
    /// `offers_translation` :506 asks it per prose block).
    pub fn needs_translation(text: String) -> bool {
        ss_core::translation::needs_translation(&text, ss_core::translation::active_target())
    }

    /// Whether the LLM engine is selected but cannot run (translation.rs
    /// `blocked_hint` gate): the translate button stays quiet instead of
    /// sweeping forever. Read-only.
    ///
    /// Same divergence as `MarketplaceTranslation::descriptions_blocked`
    /// (this crate does not query subscription accounts from `ss-accounts`):
    /// the key check reads the typed key file only, so an account-only setup
    /// reports blocked even when a subscription key would be available.
    pub fn blocked() -> bool {
        let Ok(config) = ss_core::translation::load_config() else {
            return false;
        };
        if config.engine != ss_core::translation::Engine::Llm {
            return false;
        }
        config.llm_model.trim().is_empty()
            || ss_core::translation::load_api_key().trim().is_empty()
    }

    /// The live engine call the reader schedules for its prose blocks
    /// (translation.rs `schedule_when` → `fill`): translate `sources` into
    /// the active target, writing every success into the disk cache before
    /// returning. Resolves `{done: [{source, text}], rejection: {model,
    /// detail} | null}` — `rejection` is the model's HTTP-400 refusal; the
    /// TSX session surfaces it once and the next translate press clears it
    /// (`retry_rejected` parity, client-side).
    ///
    /// Same subscription-account divergence as `blocked`: the LLM auth carries
    /// the typed key file, not a subscription account key.
    pub async fn fill(sources: Vec<String>) -> Result<JsonValue, String> {
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

    /// The reader's translation-style menu (reader_style.rs `menu` →
    /// `themes::reader_themes`): `[{id, labelKey}]` in menu order. The
    /// label keys are the shared catalog's `settings.tranTheme*`; the page
    /// resolves them with its own string table. Read-only, pure.
    pub fn reader_themes() -> JsonValue {
        JsonValue::Array(
            ss_core::translation::reader_themes()
                .map(|theme| {
                    let mut entry = JsonMap::new();
                    entry.insert("id".to_owned(), JsonValue::String(theme.id.to_owned()));
                    entry.insert(
                        "labelKey".to_owned(),
                        JsonValue::String(theme.label_key.to_owned()),
                    );
                    JsonValue::Object(entry)
                })
                .collect(),
        )
    }

    /// The saved reader style (`config.reader_theme`, canonicalized on load;
    /// `DEFAULT_READER` "quote" when unset or unknown). Read-only.
    pub fn reader_theme() -> String {
        ss_core::translation::load_config()
            .map(|config| config.reader_theme)
            .unwrap_or_else(|_| ss_core::translation::DEFAULT_READER.to_string())
    }

    /// Pick a style (reader_style.rs `apply_theme` :143): write
    /// `reader_theme`; the open reader repaints through the page's
    /// generation signal. An unknown id is stored verbatim and reads back
    /// as the default through the config's canonicalization — same as the
    /// GPUI write.
    pub fn set_reader_theme(id: String) -> Result<(), String> {
        let mut config = ss_core::translation::load_config().unwrap_or_default();
        config.reader_theme = id;
        ss_core::translation::save_config(&config).map_err(|err| format!("{err:#}"))
    }
}

#[cfg(test)]
mod tests {
    use super::MySkillsReader;

    /// The shared sandbox guard (facades/mod.rs TEST_ENV_LOCK): every facade
    /// suite that flips SKILLSTAR_DATA_DIR borrows it so the cargo threads
    /// serialize — module-local locks cannot (wave-1 lesson).
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
                "skillstar-nsh-reader-{}-{}",
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

    /// The `#[module]` macro turns `async fn` into a task-start call in JS;
    /// from Rust tests the same bodies run as plain futures. These calls
    /// complete on first poll (the deck/my-skills facade `spin` precedent).
    fn spin<T>(future: impl std::future::Future<Output = T>) -> T {
        let mut future = std::pin::pin!(future);
        let waker = std::task::Waker::noop();
        let mut cx = std::task::Context::from_waker(waker);
        match future.as_mut().poll(&mut cx) {
            std::task::Poll::Ready(value) => value,
            std::task::Poll::Pending => unreachable!("domain calls finish on first poll"),
        }
    }

    /// The Origin::Installed round trip: a skill written into the isolated
    /// hub reads back its SKILL.md verbatim; a missing name rejects with the
    /// domain chain (skill_reader.rs Phase::Failed). `read` runs inline (a
    /// cheap local read), so this drives it directly.
    #[test]
    fn read_round_trips_a_hub_skill_and_rejects_missing_names() {
        let _iso = IsolatedDataDir::new();
        let dir = ss_core::infra::paths::hub_skills_dir().join("demo-skill");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("SKILL.md"),
            "---\nname: demo-skill\n---\n\n# Demo\n\nHello world.\n",
        )
        .unwrap();

        let content = MySkillsReader::read("demo-skill".to_owned()).expect("hub skill reads");
        assert!(content.starts_with("---\nname: demo-skill"));
        assert!(content.contains("# Demo"));

        let err = MySkillsReader::read("missing-skill".to_owned())
            .expect_err("a missing skill is a failure card, not a panic");
        assert!(!err.is_empty());
    }

    /// The SKILL.md surface switch starts off and reads back after a save —
    /// the reader only reads it (translation_chosen never writes).
    #[test]
    fn the_skill_md_switch_reads_the_config() {
        let _iso = IsolatedDataDir::new();
        assert!(!MySkillsReader::skill_md_enabled());
        let mut config = ss_core::translation::TranslationConfig {
            translate_skill_md: true,
            ..Default::default()
        };
        ss_core::translation::save_config(&config).unwrap();
        assert!(MySkillsReader::skill_md_enabled());
        config.translate_skill_md = false;
        ss_core::translation::save_config(&config).unwrap();
        assert!(!MySkillsReader::skill_md_enabled());
    }

    /// The translate-button gate: prose the target needs is offered, copy
    /// already in the target is not; an unknown cache line stays a miss.
    #[test]
    fn needs_translation_gates_and_misses_stay_misses() {
        let _iso = IsolatedDataDir::new();
        assert!(MySkillsReader::needs_translation("Hello world.".into()));
        assert!(!MySkillsReader::needs_translation("你好，世界。".into()));
        assert_eq!(MySkillsReader::lookup("never-cached".into()), None);
    }

    /// The style menu: the catalog's order and ids (themes.rs THEMES), the
    /// default "quote", a write round trip, and the unknown-id
    /// canonicalization the config's load applies.
    #[test]
    fn the_style_menu_round_trips_and_canonicalizes() {
        let _iso = IsolatedDataDir::new();
        let themes = MySkillsReader::reader_themes();
        let list = themes.as_array().expect("themes array");
        let ids: Vec<&str> = list
            .iter()
            .map(|theme| theme["id"].as_str().expect("id"))
            .collect();
        assert_eq!(
            ids,
            vec![
                "none", "line", "dot", "dash", "dbold", "wavy", "wbold", "box", "xbold", "mark",
                "gmark", "fuzzy", "hi", "quote", "grad", "blink", "glow", "color",
            ],
            "the menu order is themes.rs THEMES"
        );
        assert!(list
            .iter()
            .all(|theme| theme["labelKey"].as_str().is_some()));

        assert_eq!(MySkillsReader::reader_theme(), "quote");
        MySkillsReader::set_reader_theme("line".to_owned()).expect("style write");
        assert_eq!(MySkillsReader::reader_theme(), "line");

        // An unknown id is stored verbatim (the GPUI write) but reads back
        // through the load-time canonicalization to the default.
        MySkillsReader::set_reader_theme("not-a-theme".to_owned()).expect("verbatim write");
        assert_eq!(MySkillsReader::reader_theme(), "quote");
    }

    /// The fill call with a source the target will not translate: nothing
    /// to ask the engine, so it resolves with an empty done list and no
    /// rejection — no network in this test (the marketplace facade's
    /// same-shaped check).
    #[test]
    fn fill_skips_copy_already_in_the_target() {
        let _iso = IsolatedDataDir::new();
        let report = spin(MySkillsReader::fill(vec!["你好，世界。".to_owned()]))
            .expect("fill over an isolated dir");
        assert_eq!(report["done"].as_array().map(Vec::len), Some(0));
        assert!(report["rejection"].is_null());
    }

    /// The machine engine (the default) is never blocked.
    #[test]
    fn the_default_engine_is_never_blocked() {
        let _iso = IsolatedDataDir::new();
        assert!(!MySkillsReader::blocked());
    }
}
