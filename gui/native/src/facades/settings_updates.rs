//! Settings updates+translation facade — the skill-updates / translation
//! slice owns this file (`SettingsUpdates` in TSX; GPUI sources
//! `crates/ss-gpui/src/settings/skill_updates.rs` and `settings/translation.rs`,
//! TSX slots `src/pages/settings/sections/skillUpdates.tsx` and
//! `sections/translation.tsx`). Pre-registered empty by the settings-base
//! wave so the slice never touches a shared registry — it only adds methods
//! inside this file. Do not edit `settings.rs` (the base seam) or the
//! `lib.rs` / `facades/mod.rs` registration.
//!
//! Domain seams this slice wires (already dependencies of this crate):
//! - `ss_core::config::skill_updates::{load_config, save_config,
//!   resolve_interval_minutes, INTERVAL_CHOICES_MINUTES}` — the mode switch
//!   and the interval pills (skill_updates.rs save_skill_updates parity;
//!   interval normalization happens here so TS only ever sees the 5 grades).
//! - `ss_core::translation::{load_config, save_config, load_api_key,
//!   save_api_key, TRANSLATION_LANGUAGES, list_models}` — the engine slider,
//!   target language, surface switches and the model field
//!   (settings/translation.rs parity). `save_translation` keeps the drawer's
//!   per-skill `description_choices` and the reader's `reader_theme` from
//!   disk, exactly like the GPUI `save_translation`, so a save from this page
//!   cannot wipe a choice made elsewhere. After a save the TSX side publishes
//!   `translationPrefsChanged` (shell/events.ts) so keep-alive pages repaint
//!   from cache.
//!
//! The LLM translation path takes a **hand-typed** endpoint and key
//! (`llm_base_url` in the config, the key in its own 0600 file through
//! `load_api_key`/`save_api_key`). The accounts domain is no longer a source
//! of keys here: the accounts page owns those rows, and a translation key is
//! one the user pastes (the account's own credential may be a login token,
//! which is not an LLM bearer key).
//!
//! `pull_llm_models` is the only network call; its missing-key path resolves
//! before any socket opens, and the cargo test below stops exactly there (the
//! real HTTP path is exercised by ss-core's own suite).
//!
//! Test discipline (wave-1 lesson): any test touching the
//! `SKILLSTAR_DATA_DIR` sandbox borrows the shared `TEST_ENV_LOCK` from
//! `facades/mod.rs` for the whole sandbox lifetime — never a module-private
//! lock, never `--test-threads=1`.

use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::time::SystemTime;

use nsh_core::prelude::*;
use nsh_core::JsonValue;

use nsh_core::element::__private::JsonMap;

type FileStamp = Option<(u64, Option<SystemTime>)>;

struct CachedSkillUpdates {
    path: PathBuf,
    stamp: FileStamp,
    config: ss_core::config::skill_updates::SkillUpdateConfig,
    pending_writes: u64,
}

static SKILL_UPDATES_CACHE: OnceLock<Mutex<Option<CachedSkillUpdates>>> = OnceLock::new();

fn skill_updates_cache() -> &'static Mutex<Option<CachedSkillUpdates>> {
    SKILL_UPDATES_CACHE.get_or_init(|| Mutex::new(None))
}

fn file_stamp(path: &Path) -> FileStamp {
    std::fs::metadata(path)
        .ok()
        .map(|meta| (meta.len(), meta.modified().ok()))
}

fn read_skill_updates_cached() -> ss_core::config::skill_updates::SkillUpdateConfig {
    let path = ss_core::infra::paths::skill_updates_config_path();
    let mut guard = skill_updates_cache()
        .lock()
        .expect("skill_updates cache lock poisoned");
    if let Some(cached) = guard.as_ref()
        && cached.path == path
        && (cached.pending_writes > 0 || file_stamp(&path) == cached.stamp)
    {
        return cached.config.clone();
    }
    let stamp = file_stamp(&path);
    let config = ss_core::config::skill_updates::load_config().unwrap_or_default();
    *guard = Some(CachedSkillUpdates {
        path,
        stamp,
        config: config.clone(),
        pending_writes: 0,
    });
    config
}

#[cfg(not(test))]
fn skill_updates_writer_tx(
) -> &'static std::sync::mpsc::Sender<(PathBuf, ss_core::config::skill_updates::SkillUpdateConfig)>
{
    static TX: OnceLock<
        std::sync::mpsc::Sender<(PathBuf, ss_core::config::skill_updates::SkillUpdateConfig)>,
    > = OnceLock::new();
    TX.get_or_init(|| {
        let (tx, rx) = std::sync::mpsc::channel::<(
            PathBuf,
            ss_core::config::skill_updates::SkillUpdateConfig,
        )>();
        let _ = std::thread::Builder::new()
            .name("ss-gui-skill-updates-writer".to_string())
            .spawn(move || {
                while let Ok((mut path, mut config)) = rx.recv() {
                    let mut flushed_count: u64 = 1;
                    while let Ok((next_path, next_config)) = rx.try_recv() {
                        if next_path == path {
                            config = next_config;
                            flushed_count += 1;
                        } else {
                            flush_one_skill_updates_write(&path, &config, flushed_count);
                            path = next_path;
                            config = next_config;
                            flushed_count = 1;
                        }
                    }
                    flush_one_skill_updates_write(&path, &config, flushed_count);
                }
            });
        tx
    })
}

#[cfg(not(test))]
fn flush_one_skill_updates_write(
    path: &Path,
    config: &ss_core::config::skill_updates::SkillUpdateConfig,
    flushed_count: u64,
) {
    if let Err(err) = ss_core::config::skill_updates::save_config(config) {
        eprintln!("settings_updates: failed to persist {}: {err:#}", path.display());
    }
    let stamp = file_stamp(path);
    let mut guard = skill_updates_cache()
        .lock()
        .expect("skill_updates cache lock poisoned");
    if let Some(cached) = guard.as_mut()
        && cached.path == path
    {
        cached.pending_writes = cached.pending_writes.saturating_sub(flushed_count);
        if cached.pending_writes == 0 {
            cached.stamp = stamp;
        }
    }
}

fn save_skill_updates_cached(
    config: ss_core::config::skill_updates::SkillUpdateConfig,
) -> Result<(), String> {
    let path = ss_core::infra::paths::skill_updates_config_path();
    let mut guard = skill_updates_cache()
        .lock()
        .expect("skill_updates cache lock poisoned");
    let entry = guard.get_or_insert_with(|| CachedSkillUpdates {
        path: path.clone(),
        stamp: None,
        config: config.clone(),
        pending_writes: 0,
    });
    entry.path = path.clone();
    entry.config = config.clone();

    #[cfg(test)]
    {
        drop(guard);
        ss_core::config::skill_updates::save_config(&config).map_err(|err| format!("{err:#}"))?;
        let stamp = file_stamp(&path);
        let mut guard = skill_updates_cache()
            .lock()
            .expect("skill_updates cache lock poisoned");
        if let Some(cached) = guard.as_mut()
            && cached.path == path
        {
            cached.stamp = stamp;
        }
        Ok(())
    }

    #[cfg(not(test))]
    {
        entry.pending_writes += 1;
        let send_res = skill_updates_writer_tx().send((path.clone(), config.clone()));
        drop(guard);
        if send_res.is_err() {
            let result =
                ss_core::config::skill_updates::save_config(&config).map_err(|err| format!("{err:#}"));
            let stamp = file_stamp(&path);
            let mut guard = skill_updates_cache()
                .lock()
                .expect("skill_updates cache lock poisoned");
            if let Some(cached) = guard.as_mut()
                && cached.path == path
            {
                cached.pending_writes = cached.pending_writes.saturating_sub(1);
                if cached.pending_writes == 0 {
                    cached.stamp = stamp;
                }
            }
            return result;
        }
        Ok(())
    }
}

/// The skill-updates + translation settings seam (`SettingsUpdates` in TSX).
#[module]
pub struct SettingsUpdates;

#[module]
impl SettingsUpdates {
    /// `skill_updates::load_config` parity: `{auto_update, interval_minutes}`,
    /// missing file → manual / hourly. Backed by in-memory cache.
    pub fn load_skill_updates() -> JsonValue {
        let config = read_skill_updates_cached();
        serde_json::to_value(&config).unwrap_or(JsonValue::Null)
    }

    /// `save_skill_updates` parity: the picked interval rides
    /// `resolve_interval_minutes` here, so anything off the five grades
    /// stores as the hourly default (the domain load does the same on the
    /// way in). Atomic write via background writer thread.
    pub fn save_skill_updates(auto_update: bool, interval_minutes: u64) -> Result<(), String> {
        let config = ss_core::config::skill_updates::SkillUpdateConfig {
            auto_update,
            interval_minutes: ss_core::config::skill_updates::resolve_interval_minutes(
                interval_minutes,
            ),
        };
        save_skill_updates_cached(config)
    }

    /// `translation::load_config` parity: the whole
    /// `TranslationConfig` (serde snake_case), already normalized by the
    /// domain. Read-only.
    pub fn load_translation() -> JsonValue {
        let config = ss_core::translation::load_config().unwrap_or_default();
        serde_json::to_value(&config).unwrap_or(JsonValue::Null)
    }

    /// `save_translation` parity: this page never edits per-skill description
    /// choices or the reader style, so both ride the disk copy (a save cannot
    /// wipe a choice the drawer or the SKILL.md reader made while this page
    /// held an older copy). Resolves with the reloaded, normalized config.
    pub fn save_translation(config: JsonValue) -> Result<JsonValue, String> {
        let mut parsed: ss_core::translation::TranslationConfig =
            serde_json::from_value(config).map_err(|err| format!("{err:#}"))?;
        if let Ok(disk) = ss_core::translation::load_config() {
            parsed.description_choices = disk.description_choices;
            parsed.reader_theme = disk.reader_theme;
        }
        ss_core::translation::save_config(&parsed).map_err(|err| format!("{err:#}"))?;
        Ok(serde_json::to_value(&ss_core::translation::load_config().unwrap_or_default())
            .unwrap_or(JsonValue::Null))
    }

    /// `TRANSLATION_LANGUAGES`: the five `{code, label}` rows (endonyms) the
    /// target-language slider rides. Pure.
    pub fn translation_languages() -> JsonValue {
        let rows: Vec<JsonValue> = ss_core::translation::TRANSLATION_LANGUAGES
            .iter()
            .map(|language| {
                let mut map = JsonMap::new();
                map.insert(
                    "code".to_owned(),
                    JsonValue::String(language.code.to_owned()),
                );
                map.insert(
                    "label".to_owned(),
                    JsonValue::String(language.label.to_owned()),
                );
                JsonValue::Object(map)
            })
            .collect();
        JsonValue::Array(rows)
    }

    /// The hand-typed LLM key (`translation::load_api_key`: its own 0600 file,
    /// never part of the config JSON). Read-only.
    pub fn load_llm_key() -> String {
        ss_core::translation::load_api_key()
    }

    /// Store the hand-typed LLM key (`translation::save_api_key`, atomic +
    /// 0600). Trimmed here so a stray newline never reaches the bearer header.
    pub fn save_llm_key(key: String) -> Result<(), String> {
        ss_core::translation::save_api_key(&key).map_err(|err| format!("{err:#}"))
    }

    /// `pull_llm_models` parity: fetch the model catalog for the endpoint the
    /// saved config names (`GET {llm_base_url}/models` with the typed key —
    /// the same future the GPUI page spawns). Resolves with a wire envelope
    /// instead of a bare list so the TSX side can pick the right i18n
    /// message: `{"ok": true, "models": [..]}` or `{"ok": false, "error":
    /// "missing_base" | "missing_key" | "empty" | "status" | "request",
    /// "status": <code?>}`. `missing_base` and `missing_key` resolve before
    /// any network I/O.
    pub async fn pull_llm_models() -> Result<JsonValue, String> {
        let config = ss_core::translation::load_config().unwrap_or_default();
        let base = config.llm_base_url.trim().to_owned();
        if base.is_empty() {
            return Ok(pull_error("missing_base", None));
        }
        let key = ss_core::translation::load_api_key();
        if key.trim().is_empty() {
            return Ok(pull_error("missing_key", None));
        }
        match ss_core::translation::list_models(&base, &key).await {
            Ok(models) => {
                let mut map = JsonMap::new();
                map.insert("ok".to_owned(), JsonValue::Bool(true));
                map.insert(
                    "models".to_owned(),
                    JsonValue::Array(
                        models
                            .into_iter()
                            .map(JsonValue::String)
                            .collect::<Vec<_>>(),
                    ),
                );
                Ok(JsonValue::Object(map))
            }
            Err(error) => {
                let (kind, status) =
                    match error.downcast_ref::<ss_core::translation::ModelListError>() {
                        Some(ss_core::translation::ModelListError::MissingKey) => {
                            ("missing_key", None)
                        }
                        Some(ss_core::translation::ModelListError::Empty) => ("empty", None),
                        Some(ss_core::translation::ModelListError::Status(code)) => {
                            ("status", Some(u64::from(*code)))
                        }
                        Some(ss_core::translation::ModelListError::Request) | None => {
                            ("request", None)
                        }
                    };
                Ok(pull_error(kind, status))
            }
        }
    }
}

fn pull_error(kind: &str, status: Option<u64>) -> JsonValue {
    let mut map = JsonMap::new();
    map.insert("ok".to_owned(), JsonValue::Bool(false));
    map.insert("error".to_owned(), JsonValue::String(kind.to_owned()));
    if let Some(status) = status {
        map.insert("status".to_owned(), JsonValue::from(status));
    }
    JsonValue::Object(map)
}

#[cfg(test)]
mod tests {
    use super::SettingsUpdates;

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
                "skillstar-nsh-settings-updates-{}-{}",
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

    fn str_field(value: &nsh_core::JsonValue, key: &str) -> Option<String> {
        value.get(key).and_then(|item| item.as_str()).map(str::to_owned)
    }

    fn bool_field(value: &nsh_core::JsonValue, key: &str) -> Option<bool> {
        value.get(key).and_then(|item| item.as_bool())
    }

    /// skill_updates.rs parity: a missing file is manual/hourly, a save with
    /// an off-grade interval stores the hourly default (resolve_interval_
    /// minutes rides the facade), and every round trip keeps the mode.
    #[test]
    fn skill_updates_default_resolve_and_round_trip() {
        let _iso = IsolatedDataDir::new();
        let first = SettingsUpdates::load_skill_updates();
        assert_eq!(bool_field(&first, "auto_update"), Some(false));
        assert_eq!(
            first.get("interval_minutes").and_then(|v| v.as_u64()),
            Some(60)
        );

        SettingsUpdates::save_skill_updates(true, 360).unwrap();
        let saved = SettingsUpdates::load_skill_updates();
        assert_eq!(bool_field(&saved, "auto_update"), Some(true));
        assert_eq!(
            saved.get("interval_minutes").and_then(|v| v.as_u64()),
            Some(360)
        );

        // 7 is not one of the five grades: resolve_interval_minutes → 60.
        SettingsUpdates::save_skill_updates(true, 7).unwrap();
        let resolved = SettingsUpdates::load_skill_updates();
        assert_eq!(
            resolved.get("interval_minutes").and_then(|v| v.as_u64()),
            Some(60)
        );
    }

    /// translation.rs save_translation parity: a save from this page never
    /// wipes the drawer's per-skill description choices or the reader's
    /// style — both ride the disk copy — and the reloaded config echoes back.
    #[test]
    fn translation_save_keeps_disk_choices_and_reader_theme() {
        let _iso = IsolatedDataDir::new();
        // Seed the disk the way the drawer / reader would: a per-skill choice
        // plus a non-default reader style.
        let mut seeded = ss_core::translation::TranslationConfig::default();
        seeded.reader_theme = "wavy".into();
        seeded
            .description_choices
            .insert("demo".to_owned(), true);
        ss_core::translation::save_config(&seeded).unwrap();

        // The page's in-memory copy knows nothing about either field.
        let mut from_page = ss_core::translation::TranslationConfig::default();
        from_page.engine = ss_core::translation::Engine::Llm;
        from_page.translate_descriptions = true;
        from_page.target_lang = "ja".into();
        let input = serde_json::to_value(&from_page).unwrap();
        let saved = SettingsUpdates::save_translation(input).unwrap();

        assert_eq!(str_field(&saved, "engine").as_deref(), Some("llm"));
        assert_eq!(str_field(&saved, "target_lang").as_deref(), Some("ja"));
        assert_eq!(bool_field(&saved, "translate_descriptions"), Some(true));
        assert_eq!(str_field(&saved, "reader_theme").as_deref(), Some("wavy"));
        assert_eq!(
            saved
                .get("description_choices")
                .and_then(|v| v.get("demo"))
                .and_then(|v| v.as_bool()),
            Some(true),
            "the drawer's choice survives a settings save"
        );
    }

    /// The static catalog the language slider rides: five endonym rows,
    /// zh-CN first.
    #[test]
    fn language_catalog_keeps_its_order() {
        let languages = SettingsUpdates::translation_languages();
        let rows = languages.as_array().unwrap();
        assert_eq!(rows.len(), 5);
        assert_eq!(str_field(&rows[0], "code").as_deref(), Some("zh-CN"));
        assert_eq!(str_field(&rows[0], "label").as_deref(), Some("简体中文"));
        assert_eq!(str_field(&rows[4], "code").as_deref(), Some("ko"));
    }

    /// The hand-typed key round-trips through its own file (trimmed on the way
    /// in, so a trailing newline never reaches the bearer header), and an
    /// isolated data dir starts with none.
    #[test]
    fn typed_llm_key_round_trips() {
        let _iso = IsolatedDataDir::new();
        assert_eq!(SettingsUpdates::load_llm_key(), "");
        SettingsUpdates::save_llm_key("  sk-typed-1  ".to_owned()).unwrap();
        assert_eq!(SettingsUpdates::load_llm_key(), "sk-typed-1");
    }

    /// pull_llm_models' shape-only miss (never the network): a sandbox with no
    /// typed key resolves `missing_key` on the first poll, before any socket
    /// opens. (The `missing_base` guard only fires on a config the domain's
    /// normalization can no longer produce — the shipped default URL fills an
    /// emptied one — so its envelope is pinned by shape below instead.)
    #[test]
    fn pull_llm_models_misses_resolve_before_the_network() {
        let _iso = IsolatedDataDir::new();
        let waker = std::task::Waker::noop();
        let mut cx = std::task::Context::from_waker(waker);

        let mut no_key = std::pin::pin!(SettingsUpdates::pull_llm_models());
        let report = match no_key.as_mut().poll(&mut cx) {
            std::task::Poll::Ready(value) => value.expect("pull over an isolated dir"),
            std::task::Poll::Pending => unreachable!("a missing key resolves on first poll"),
        };
        assert_eq!(bool_field(&report, "ok"), Some(false));
        assert_eq!(str_field(&report, "error").as_deref(), Some("missing_key"));

        // The wire envelope every failure kind rides (the TSX side switch on it).
        let base = super::pull_error("missing_base", None);
        assert_eq!(bool_field(&base, "ok"), Some(false));
        assert_eq!(str_field(&base, "error").as_deref(), Some("missing_base"));
        assert!(base.get("status").is_none(), "no status for a shape miss");

        let status = super::pull_error("status", Some(429));
        assert_eq!(str_field(&status, "error").as_deref(), Some("status"));
        assert_eq!(status.get("status").and_then(|v| v.as_u64()), Some(429));
    }
}
