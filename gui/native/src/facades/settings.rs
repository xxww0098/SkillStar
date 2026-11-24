//! Settings base facade — the settings page's own seam (`Settings` in TSX),
//! registered by the settings-base wave. Covers the base sections the base
//! worker owns: `gui_prefs.json` (language / background style / background
//! run / the accounts page's auto-refresh cycle) and `proxy.json`. The four
//! parallel section slices never edit this file — they land in their own
//! pre-registered slots (`settings_updates.rs`, `settings_network.rs`).
//!
//! `gui_prefs.json` is owned here; the GPUI shell that used to co-own it is
//! gone (D-110). It is read and written through
//! `ss_core::infra::paths::config_dir()` so `SKILLSTAR_DATA_DIR` overrides
//! keep working, with the defaults `zh-CN` / `current` / `false` / no
//! cycle. Two pages write the same file, so every writer reads the map first
//! and replaces only its own keys. Proxy reads and writes are direct calls
//! into `ss_core::config::proxy::{load_config, save_config}`.

use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::time::SystemTime;

use nsh_core::prelude::*;
use nsh_core::JsonValue;

use nsh_core::element::__private::JsonMap;

/// The settings base seam (`Settings` in TSX).
#[module]
pub struct Settings;

/// The key the accounts page's staggered auto-refresh cycle lives under.
/// Missing key defaults to `DEFAULT_ACCOUNTS_AUTO_REFRESH_MS` (5 minutes); an
/// explicit `null` or `0` means the user turned auto-refresh off.
const ACCOUNTS_AUTO_REFRESH_KEY: &str = "accounts_auto_refresh_ms";
const DEFAULT_ACCOUNTS_AUTO_REFRESH_MS: u64 = 300_000;
const TRAY_QUOTA_ENABLED_KEY: &str = "tray_quota_enabled";
const LEGACY_TRAY_SHOW_PERCENT_KEY: &str = "tray_show_percent";

/// The on-disk `gui_prefs.json` shape: the three base fields the settings
/// page owns plus the accounts page's auto-refresh cycle. Every field is
/// defaulted on read, so a file written by an older build keeps loading.
struct GuiPrefs {
    language: String,
    background_style: String,
    background_run: bool,
    accounts_auto_refresh_ms: Option<u64>,
}

impl Default for GuiPrefs {
    fn default() -> Self {
        Self {
            language: "zh-CN".into(),
            background_style: "current".into(),
            background_run: false,
            accounts_auto_refresh_ms: Some(DEFAULT_ACCOUNTS_AUTO_REFRESH_MS),
        }
    }
}

fn prefs_path() -> PathBuf {
    ss_core::infra::paths::config_dir().join("gui_prefs.json")
}

type FileStamp = Option<(u64, Option<SystemTime>)>;

struct CachedPrefs {
    path: PathBuf,
    stamp: FileStamp,
    map: JsonMap<String, JsonValue>,
    pending_writes: u64,
}

static PREFS_CACHE: OnceLock<Mutex<Option<CachedPrefs>>> = OnceLock::new();

fn prefs_cache() -> &'static Mutex<Option<CachedPrefs>> {
    PREFS_CACHE.get_or_init(|| Mutex::new(None))
}

fn file_stamp(path: &Path) -> FileStamp {
    std::fs::metadata(path)
        .ok()
        .map(|meta| (meta.len(), meta.modified().ok()))
}

fn read_map_from_disk(path: &Path) -> JsonMap<String, JsonValue> {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|text| serde_json::from_str::<serde_json::Value>(&text).ok())
        .and_then(|raw| match raw {
            JsonValue::Object(map) => Some(map),
            _ => None,
        })
        .unwrap_or_default()
}

fn ensure_cached_map(guard: &mut Option<CachedPrefs>, path: &Path) -> JsonMap<String, JsonValue> {
    if let Some(cached) = guard.as_ref()
        && cached.path == path
    {
        if cached.pending_writes > 0 {
            return cached.map.clone();
        }
        let stamp = file_stamp(path);
        if stamp == cached.stamp {
            return cached.map.clone();
        }
    }
    let stamp = file_stamp(path);
    let map = read_map_from_disk(path);
    *guard = Some(CachedPrefs {
        path: path.to_path_buf(),
        stamp,
        map: map.clone(),
        pending_writes: 0,
    });
    map
}

/// The file's top level as a map. A missing file, malformed JSON and a
/// non-object root all read as an empty map, so each reader falls back to its
/// own default: neither the settings page nor the accounts page may fail to
/// open because of a broken prefs file.
fn read_raw() -> JsonMap<String, JsonValue> {
    let path = prefs_path();
    let mut guard = prefs_cache()
        .lock()
        .expect("gui_prefs cache lock poisoned");
    ensure_cached_map(&mut guard, &path)
}

fn write_file_atomic(path: &Path, bytes: &[u8]) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|err| format!("{err:#}"))?;
    }
    ss_core::infra::fs_ops::atomic_write(path, bytes).map_err(|err| format!("{err:#}"))
}

#[cfg(not(test))]
fn prefs_writer_tx() -> &'static std::sync::mpsc::Sender<(PathBuf, Vec<u8>)> {
    static TX: OnceLock<std::sync::mpsc::Sender<(PathBuf, Vec<u8>)>> = OnceLock::new();
    TX.get_or_init(|| {
        let (tx, rx) = std::sync::mpsc::channel::<(PathBuf, Vec<u8>)>();
        let _ = std::thread::Builder::new()
            .name("ss-gui-prefs-writer".to_string())
            .spawn(move || {
                while let Ok((mut path, mut bytes)) = rx.recv() {
                    let mut flushed_count: u64 = 1;
                    // Coalesce burst saves to the latest snapshot before touching disk.
                    while let Ok((next_path, next_bytes)) = rx.try_recv() {
                        if next_path == path {
                            bytes = next_bytes;
                            flushed_count += 1;
                        } else {
                            flush_one_prefs_write(&path, &bytes, flushed_count);
                            path = next_path;
                            bytes = next_bytes;
                            flushed_count = 1;
                        }
                    }
                    flush_one_prefs_write(&path, &bytes, flushed_count);
                }
            });
        tx
    })
}

#[cfg(not(test))]
fn flush_one_prefs_write(path: &Path, bytes: &[u8], flushed_count: u64) {
    if let Err(err) = write_file_atomic(path, bytes) {
        eprintln!("settings: failed to persist {}: {err}", path.display());
    }
    let stamp = file_stamp(path);
    let mut guard = prefs_cache()
        .lock()
        .expect("gui_prefs cache lock poisoned");
    if let Some(cached) = guard.as_mut()
        && cached.path == path
    {
        cached.pending_writes = cached.pending_writes.saturating_sub(flushed_count);
        if cached.pending_writes == 0 {
            cached.stamp = stamp;
        }
    }
}

/// Update one slice of `gui_prefs.json` atomically while keeping sibling keys intact.
fn mutate_raw(mutate: impl FnOnce(&mut JsonMap<String, JsonValue>)) -> Result<(), String> {
    let path = prefs_path();
    let mut guard = prefs_cache()
        .lock()
        .expect("gui_prefs cache lock poisoned");
    let mut raw = ensure_cached_map(&mut guard, &path);
    mutate(&mut raw);
    let mut text = serde_json::to_string_pretty(&JsonValue::Object(raw.clone()))
        .map_err(|err| format!("{err:#}"))?;
    text.push('\n');
    let bytes = text.into_bytes();
    let entry = guard.get_or_insert_with(|| CachedPrefs {
        path: path.clone(),
        stamp: None,
        map: JsonMap::new(),
        pending_writes: 0,
    });
    entry.path = path.clone();
    entry.map = raw;

    #[cfg(test)]
    {
        drop(guard);
        write_file_atomic(&path, &bytes)?;
        let stamp = file_stamp(&path);
        let mut guard = prefs_cache()
            .lock()
            .expect("gui_prefs cache lock poisoned");
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
        // Send inside the cache mutex so mpsc FIFO order matches in-memory mutation
        // order and concurrent callers cannot overwrite newer bytes with older ones;
        // the unbounded channel send itself never blocks.
        let send_res = prefs_writer_tx().send((path.clone(), bytes.clone()));
        drop(guard);
        if send_res.is_err() {
            let result = write_file_atomic(&path, &bytes);
            let stamp = file_stamp(&path);
            let mut guard = prefs_cache()
                .lock()
                .expect("gui_prefs cache lock poisoned");
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

/// The parsed prefs, defaults filled in (so `Default` stays the one place the
/// fallbacks are written down).
fn read_prefs() -> GuiPrefs {
    let raw = read_raw();
    let fallback = GuiPrefs::default();
    let accounts_auto_refresh_ms = match raw.get(ACCOUNTS_AUTO_REFRESH_KEY) {
        None => fallback.accounts_auto_refresh_ms,
        Some(JsonValue::Null) => None,
        Some(v) => v.as_u64().filter(|&ms| ms > 0),
    };
    GuiPrefs {
        language: raw
            .get("language")
            .and_then(|v| v.as_str())
            .map(str::to_owned)
            .unwrap_or(fallback.language),
        background_style: raw
            .get("background_style")
            .and_then(|v| v.as_str())
            .map(str::to_owned)
            .unwrap_or(fallback.background_style),
        background_run: raw
            .get("background_run")
            .and_then(|v| v.as_bool())
            .unwrap_or(fallback.background_run),
        accounts_auto_refresh_ms,
    }
}

fn prefs_value(prefs: &GuiPrefs) -> JsonValue {
    let mut map = JsonMap::new();
    map.insert("language".to_owned(), JsonValue::String(prefs.language.clone()));
    map.insert(
        "background_style".to_owned(),
        JsonValue::String(prefs.background_style.clone()),
    );
    map.insert(
        "background_run".to_owned(),
        JsonValue::Bool(prefs.background_run),
    );
    JsonValue::Object(map)
}

struct CachedProxy {
    path: PathBuf,
    stamp: FileStamp,
    config: ss_core::config::proxy::ProxyConfig,
    pending_writes: u64,
}

static PROXY_CACHE: OnceLock<Mutex<Option<CachedProxy>>> = OnceLock::new();

fn proxy_cache() -> &'static Mutex<Option<CachedProxy>> {
    PROXY_CACHE.get_or_init(|| Mutex::new(None))
}

fn read_proxy_cached() -> ss_core::config::proxy::ProxyConfig {
    let path = ss_core::infra::paths::proxy_config_path();
    let mut guard = proxy_cache().lock().expect("proxy cache lock poisoned");
    if let Some(cached) = guard.as_ref()
        && cached.path == path
        && (cached.pending_writes > 0 || file_stamp(&path) == cached.stamp)
    {
        return cached.config.clone();
    }
    let stamp = file_stamp(&path);
    let config = ss_core::config::proxy::load_config().unwrap_or_default();
    *guard = Some(CachedProxy {
        path,
        stamp,
        config: config.clone(),
        pending_writes: 0,
    });
    config
}

#[cfg(not(test))]
fn proxy_writer_tx() -> &'static std::sync::mpsc::Sender<(PathBuf, Vec<u8>)> {
    static TX: OnceLock<std::sync::mpsc::Sender<(PathBuf, Vec<u8>)>> = OnceLock::new();
    TX.get_or_init(|| {
        let (tx, rx) = std::sync::mpsc::channel::<(PathBuf, Vec<u8>)>();
        let _ = std::thread::Builder::new()
            .name("ss-gui-proxy-writer".to_string())
            .spawn(move || {
                while let Ok((mut path, mut bytes)) = rx.recv() {
                    let mut flushed_count: u64 = 1;
                    while let Ok((next_path, next_bytes)) = rx.try_recv() {
                        if next_path == path {
                            bytes = next_bytes;
                            flushed_count += 1;
                        } else {
                            flush_one_proxy_write(&path, &bytes, flushed_count);
                            path = next_path;
                            bytes = next_bytes;
                            flushed_count = 1;
                        }
                    }
                    flush_one_proxy_write(&path, &bytes, flushed_count);
                }
            });
        tx
    })
}

#[cfg(not(test))]
fn flush_one_proxy_write(path: &Path, bytes: &[u8], flushed_count: u64) {
    if let Err(err) = write_file_atomic(path, bytes) {
        eprintln!("settings: failed to persist {}: {err}", path.display());
    }
    let stamp = file_stamp(path);
    let mut guard = proxy_cache().lock().expect("proxy cache lock poisoned");
    if let Some(cached) = guard.as_mut()
        && cached.path == path
    {
        cached.pending_writes = cached.pending_writes.saturating_sub(flushed_count);
        if cached.pending_writes == 0 {
            cached.stamp = stamp;
        }
    }
}

fn save_proxy_cached(parsed: ss_core::config::proxy::ProxyConfig) -> Result<(), String> {
    let path = ss_core::infra::paths::proxy_config_path();
    let bytes = serde_json::to_string_pretty(&parsed)
        .map_err(|err| format!("{err:#}"))?
        .into_bytes();
    let mut guard = proxy_cache().lock().expect("proxy cache lock poisoned");
    let entry = guard.get_or_insert_with(|| CachedProxy {
        path: path.clone(),
        stamp: None,
        config: parsed.clone(),
        pending_writes: 0,
    });
    entry.path = path.clone();
    entry.config = parsed;

    #[cfg(test)]
    {
        drop(guard);
        write_file_atomic(&path, &bytes)?;
        let stamp = file_stamp(&path);
        let mut guard = proxy_cache().lock().expect("proxy cache lock poisoned");
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
        let send_res = proxy_writer_tx().send((path.clone(), bytes.clone()));
        drop(guard);
        if send_res.is_err() {
            let result = write_file_atomic(&path, &bytes);
            let stamp = file_stamp(&path);
            let mut guard = proxy_cache().lock().expect("proxy cache lock poisoned");
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

#[cfg(test)]
fn field_str(value: &JsonValue, key: &str) -> Option<String> {
    value.get(key).and_then(|item| item.as_str()).map(str::to_owned)
}

#[cfg(test)]
fn field_bool(value: &JsonValue, key: &str) -> Option<bool> {
    value.get(key).and_then(|item| item.as_bool())
}

#[module]
impl Settings {
    /// Inline `data:image/png;base64,…` URI for the 32px sidebar brand mark.
    /// Pure, synchronous, backed by the embedded `assets/icon.png`.
    pub fn brand_icon_data_uri() -> String {
        crate::brand_icon_data_uri().to_owned()
    }

    /// Inline `data:image/svg+xml;base64,…` URI for the monochrome menu-bar /
    /// status-bar template icon (`assets/tray-icon.svg`). Pure, synchronous.
    pub fn tray_icon_data_uri() -> String {
        crate::tray_icon_data_uri().to_owned()
    }

    /// Inline `data:image/svg+xml;base64,…` URI for an agent brand icon.
    /// When `light` is true, monochrome dark-ink SVGs (`#e8eef8`) are
    /// recolored to paper foreground (`#18181b`). Pure, synchronous.
    pub fn agent_icon_data_uri(id: String, light: bool) -> String {
        crate::agent_icons::agent_icon_data_uri(&id, light)
    }

    /// Pure in-memory lookup for an already-loaded GitHub owner avatar `data:image/…` URI.
    pub fn peek_github_avatar_data_uri(owner: String) -> Option<String> {
        crate::repo_avatars::peek_github_avatar_data_uri(&owner)
    }

    /// Load a GitHub owner avatar `data:image/…` URI from `cache_dir()/avatars/{owner}.png`,
    /// fetching via `probe_http_client` on cache miss.
    pub async fn load_github_avatar_data_uri(owner: String) -> Option<String> {
        crate::repo_avatars::load_github_avatar_data_uri(&owner).await
    }

    /// Read `gui_prefs.json`: missing file or malformed JSON falls back to
    /// the defaults, never an error — the settings page must always open.
    /// Returns the three base fields this page renders; the accounts page's
    /// cycle has its own reader (`load_accounts_auto_refresh`). Read-only.
    pub fn load_prefs() -> JsonValue {
        prefs_value(&read_prefs())
    }

    /// Persist the three base prefs fields: pretty JSON, parent directory
    /// created on demand. Read-modify-write, because the accounts page saves
    /// into the same file — its `accounts_auto_refresh_ms` must survive a
    /// write from this page.
    pub fn save_prefs(
        language: String,
        background_style: String,
        background_run: bool,
    ) -> Result<(), String> {
        mutate_raw(|raw| {
            raw.insert("language".to_owned(), JsonValue::String(language));
            raw.insert(
                "background_style".to_owned(),
                JsonValue::String(background_style),
            );
            raw.insert("background_run".to_owned(), JsonValue::Bool(background_run));
        })
    }

    /// The accounts page's staggered auto-refresh cycle in ms (`null` = off;
    /// a file without the key defaults to 5 minutes). Read-only.
    pub fn load_accounts_auto_refresh() -> Option<u64> {
        read_prefs().accounts_auto_refresh_ms
    }

    /// Persist that cycle. `None` stores explicit `null` so "off" survives
    /// restarts instead of falling back to the 5-minute default. Read-modify-write,
    /// same discipline as `save_prefs`: the settings page's three fields stay untouched.
    pub fn save_accounts_auto_refresh(ms: Option<u64>) -> Result<(), String> {
        mutate_raw(|raw| match ms {
            Some(ms) => {
                raw.insert(
                    ACCOUNTS_AUTO_REFRESH_KEY.to_owned(),
                    JsonValue::Number(ms.into()),
                );
            }
            None => {
                raw.insert(ACCOUNTS_AUTO_REFRESH_KEY.to_owned(), JsonValue::Null);
            }
        })
    }

    /// Read the menu-bar / system-tray quota popover preference from
    /// `gui_prefs.json` (`enabled` defaults to `true`).
    pub fn load_tray_quota_prefs() -> JsonValue {
        let raw = read_raw();
        let enabled = raw
            .get(TRAY_QUOTA_ENABLED_KEY)
            .and_then(|v| v.as_bool())
            .unwrap_or(true);
        let mut map = JsonMap::new();
        map.insert("enabled".to_owned(), JsonValue::Bool(enabled));
        JsonValue::Object(map)
    }

    /// Persist the menu-bar / system-tray quota popover preference into
    /// `gui_prefs.json` without touching base prefs or auto-refresh cycle.
    pub fn save_tray_quota_prefs(enabled: bool) -> Result<(), String> {
        mutate_raw(|raw| {
            raw.insert(TRAY_QUOTA_ENABLED_KEY.to_owned(), JsonValue::Bool(enabled));
            raw.remove(LEGACY_TRAY_SHOW_PERCENT_KEY);
        })
    }

    /// Wait for the next mouse-down outside the tray quota popover on macOS.
    /// Resolves `true` when an outside click occurs, `false` when cancelled
    /// or on platforms where native window blur handles dismissal.
    pub async fn wait_popover_outside_click() -> bool {
        crate::popover_dismiss::wait_outside_click().await
    }

    /// Cancel any pending `wait_popover_outside_click` watcher and uninstall
    /// its AppKit event monitors.
    pub fn cancel_popover_outside_click() {
        crate::popover_dismiss::cancel_outside_click();
    }

    /// Read `proxy.json` (`proxy::load_config` parity — the identical domain
    /// call the GPUI page makes). A missing file returns the defaults,
    /// including the default `NO_PROXY` bypass list. Backed by in-memory cache.
    pub fn load_proxy() -> JsonValue {
        let config = read_proxy_cached();
        serde_json::to_value(&config).unwrap_or(JsonValue::Null)
    }

    /// Persist the proxy configuration (`proxy::save_config` parity, atomic
    /// write via `ss_core::infra::fs_ops` on a background writer thread).
    /// Accepts the same JSON shape `load_proxy` returns; unknown `proxy_type`
    /// strings load back as `http` (the domain's loose parse).
    pub fn save_proxy(config: JsonValue) -> Result<(), String> {
        let parsed: ss_core::config::proxy::ProxyConfig =
            serde_json::from_value(config).map_err(|err| format!("{err:#}"))?;
        save_proxy_cached(parsed)
    }
}

#[cfg(test)]
mod tests {
    use super::{
        ACCOUNTS_AUTO_REFRESH_KEY, DEFAULT_ACCOUNTS_AUTO_REFRESH_MS, Settings, field_bool,
        field_str,
    };

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
                "skillstar-nsh-settings-{}-{}",
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

    /// prefs.rs load parity: no file means the three defaults, and a
    /// malformed file degrades the same way instead of failing the page.
    #[test]
    fn missing_or_broken_prefs_load_the_defaults() {
        let _iso = IsolatedDataDir::new();
        let first = Settings::load_prefs();
        assert_eq!(field_str(&first, "language").as_deref(), Some("zh-CN"));
        assert_eq!(
            field_str(&first, "background_style").as_deref(),
            Some("current")
        );
        assert_eq!(field_bool(&first, "background_run"), Some(false));

        let path = ss_core::infra::paths::config_dir().join("gui_prefs.json");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, "{not json").unwrap();
        let broken = Settings::load_prefs();
        assert_eq!(field_str(&broken, "language").as_deref(), Some("zh-CN"));
        assert_eq!(field_bool(&broken, "background_run"), Some(false));
    }

    /// prefs.rs save parity: all three fields survive the round trip, and
    /// the file lands inside the sandbox's config dir (never real $HOME).
    #[test]
    fn prefs_round_trip_keeps_every_field() {
        let _iso = IsolatedDataDir::new();
        Settings::save_prefs("en".into(), "paper".into(), true).unwrap();
        let loaded = Settings::load_prefs();
        assert_eq!(field_str(&loaded, "language").as_deref(), Some("en"));
        assert_eq!(
            field_str(&loaded, "background_style").as_deref(),
            Some("paper")
        );
        assert_eq!(field_bool(&loaded, "background_run"), Some(true));

        let path = ss_core::infra::paths::config_dir().join("gui_prefs.json");
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(text.contains("\"background_run\": true"), "pretty JSON on disk");
    }

    /// The accounts page's cycle shares `gui_prefs.json` with the settings
    /// page: missing key defaults to 5 minutes, explicit values round-trip
    /// and survive a base-prefs save, and explicit "off" persists as `null`.
    #[test]
    fn accounts_auto_refresh_survives_a_base_prefs_save() {
        let _iso = IsolatedDataDir::new();
        assert_eq!(
            Settings::load_accounts_auto_refresh(),
            Some(DEFAULT_ACCOUNTS_AUTO_REFRESH_MS)
        );

        Settings::save_accounts_auto_refresh(Some(60_000)).unwrap();
        assert_eq!(Settings::load_accounts_auto_refresh(), Some(60_000));

        // 设置页只写它自己的三个字段：账号页的选择必须留在同一份文件里。
        Settings::save_prefs("en".into(), "paper".into(), true).unwrap();
        assert_eq!(Settings::load_accounts_auto_refresh(), Some(60_000));
        assert_eq!(
            field_str(&Settings::load_prefs(), "language").as_deref(),
            Some("en")
        );

        // 显式关闭写入 null，重启后仍保持关闭而非回退到默认 5 分钟；基础字段原样保留。
        Settings::save_accounts_auto_refresh(None).unwrap();
        assert_eq!(Settings::load_accounts_auto_refresh(), None);
        Settings::save_prefs("en".into(), "paper".into(), false).unwrap();
        assert_eq!(Settings::load_accounts_auto_refresh(), None);

        let tray_defaults = Settings::load_tray_quota_prefs();
        assert_eq!(field_bool(&tray_defaults, "enabled"), Some(true));
        Settings::save_tray_quota_prefs(false).unwrap();
        let tray_saved = Settings::load_tray_quota_prefs();
        assert_eq!(field_bool(&tray_saved, "enabled"), Some(false));
        let text =
            std::fs::read_to_string(ss_core::infra::paths::config_dir().join("gui_prefs.json"))
                .unwrap();
        assert!(
            text.contains(&format!("\"{ACCOUNTS_AUTO_REFRESH_KEY}\": null")),
            "off persists explicit null: {text}"
        );
        assert!(
            text.contains("\"language\": \"en\""),
            "base fields stay: {text}"
        );
    }

    /// proxy.rs parity through the same domain calls the GPUI page makes:
    /// defaults on a missing file, full round trip on a written one, and
    /// `socks5h` stays distinct from `socks5` across the wire.
    #[test]
    fn proxy_round_trips_through_the_domain_config() {
        let _iso = IsolatedDataDir::new();
        let defaults = Settings::load_proxy();
        assert_eq!(field_bool(&defaults, "enabled"), Some(false));
        assert_eq!(field_str(&defaults, "proxy_type").as_deref(), Some("http"));
        assert_eq!(defaults.get("port").and_then(|v| v.as_u64()), Some(7897));
        assert!(
            field_str(&defaults, "bypass").unwrap().contains(".deepseek.com"),
            "the default NO_PROXY list ships with a missing file"
        );

        let mut config = defaults.clone();
        if let Some(map) = config.as_object_mut() {
            map.insert("enabled".to_owned(), nsh_core::JsonValue::Bool(true));
            map.insert(
                "proxy_type".to_owned(),
                nsh_core::JsonValue::String("socks5h".into()),
            );
            map.insert(
                "host".to_owned(),
                nsh_core::JsonValue::String("127.0.0.1".into()),
            );
            map.insert("port".to_owned(), nsh_core::JsonValue::from(1080u64));
            map.insert(
                "username".to_owned(),
                nsh_core::JsonValue::String("alice".into()),
            );
        }
        Settings::save_proxy(config).unwrap();
        let saved = Settings::load_proxy();
        assert_eq!(field_bool(&saved, "enabled"), Some(true));
        assert_eq!(field_str(&saved, "proxy_type").as_deref(), Some("socks5h"));
        assert_eq!(field_str(&saved, "host").as_deref(), Some("127.0.0.1"));
        assert_eq!(saved.get("port").and_then(|v| v.as_u64()), Some(1080));
        assert_eq!(field_str(&saved, "username").as_deref(), Some("alice"));
    }
}
