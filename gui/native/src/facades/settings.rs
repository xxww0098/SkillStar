//! Settings base facade — the settings page's own seam (`Settings` in TSX),
//! registered by the settings-base wave. Covers the base sections the base
//! worker owns: `gui_prefs.json` (language / background style / background
//! run) and `proxy.json`. The four parallel section slices never edit this
//! file — they land in their own pre-registered slots (`settings_updates.rs`,
//! `settings_network.rs`).
//!
//! `gui_prefs.json` lives in `ss-gpui/src/prefs.rs` on the GPUI side; this
//! crate cannot link ss-gpui, so the same file is read and written here with
//! the same schema and defaults (`zh-CN` / `current` / `false`), through
//! `ss_core::infra::paths::config_dir()` so `SKILLSTAR_DATA_DIR` overrides
//! keep working. Proxy reads and writes are direct calls into
//! `ss_core::config::proxy::{load_config, save_config}` — the very same
//! functions the GPUI settings page uses.

use nsh_core::prelude::*;
use nsh_core::JsonValue;

use nsh_core::element::__private::JsonMap;

/// The settings base seam (`Settings` in TSX).
#[module]
pub struct Settings;

/// The on-disk `gui_prefs.json` shape (prefs.rs `GuiPrefs`): three fields,
/// serde-defaulted. Mirrored field-for-field; adding a fourth field is a
/// product decision that starts in prefs.rs, not here.
struct GuiPrefs {
    language: String,
    background_style: String,
    background_run: bool,
}

impl Default for GuiPrefs {
    fn default() -> Self {
        Self {
            language: "zh-CN".into(),
            background_style: "current".into(),
            background_run: false,
        }
    }
}

fn prefs_path() -> std::path::PathBuf {
    ss_core::infra::paths::config_dir().join("gui_prefs.json")
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

    /// Read `gui_prefs.json` (`prefs::load` parity): missing file or
    /// malformed JSON falls back to the defaults, never an error — the
    /// settings page must always open. Read-only.
    pub fn load_prefs() -> JsonValue {
        let prefs = match std::fs::read_to_string(prefs_path()) {
            Ok(text) => serde_json::from_str::<serde_json::Value>(&text)
                .map(|raw| GuiPrefs {
                    language: raw
                        .get("language")
                        .and_then(|v| v.as_str())
                        .unwrap_or("zh-CN")
                        .to_owned(),
                    background_style: raw
                        .get("background_style")
                        .and_then(|v| v.as_str())
                        .unwrap_or("current")
                        .to_owned(),
                    background_run: raw
                        .get("background_run")
                        .and_then(|v| v.as_bool())
                        .unwrap_or(false),
                })
                .unwrap_or_default(),
            Err(_) => GuiPrefs::default(),
        };
        prefs_value(&prefs)
    }

    /// Persist the three prefs fields (`prefs::save` parity): pretty JSON,
    /// parent directory created on demand. The GPUI side keeps the file
    /// readable by the same schema, so both shells stay interchangeable.
    pub fn save_prefs(
        language: String,
        background_style: String,
        background_run: bool,
    ) -> Result<(), String> {
        let prefs = GuiPrefs {
            language,
            background_style,
            background_run,
        };
        let path = prefs_path();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|err| format!("{err:#}"))?;
        }
        let text = serde_json::to_string_pretty(&prefs_value(&prefs))
            .map(|mut text| {
                text.push('\n');
                text
            })
            .map_err(|err| format!("{err:#}"))?;
        std::fs::write(&path, text).map_err(|err| format!("{err:#}"))
    }

    /// Read `proxy.json` (`proxy::load_config` parity — the identical domain
    /// call the GPUI page makes). A missing file returns the defaults,
    /// including the default `NO_PROXY` bypass list. Read-only.
    pub fn load_proxy() -> JsonValue {
        let config = ss_core::config::proxy::load_config().unwrap_or_default();
        serde_json::to_value(&config).unwrap_or(JsonValue::Null)
    }

    /// Persist the proxy configuration (`proxy::save_config` parity, atomic
    /// write via `ss_core::infra::fs_ops`). Accepts the same JSON shape
    /// `load_proxy` returns; unknown `proxy_type` strings load back as
    /// `http` (the domain's loose parse).
    pub fn save_proxy(config: JsonValue) -> Result<(), String> {
        let parsed: ss_core::config::proxy::ProxyConfig =
            serde_json::from_value(config).map_err(|err| format!("{err:#}"))?;
        ss_core::config::proxy::save_config(&parsed).map_err(|err| format!("{err:#}"))
    }
}

#[cfg(test)]
mod tests {
    use super::{Settings, field_bool, field_str};

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
