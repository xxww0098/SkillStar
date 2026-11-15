//! Settings network facade — the GitHub-mirror / network-doctor slice owns
//! this file (`SettingsNetwork` in TSX; GPUI sources
//! `crates/ss-gpui/src/settings/gh_mirror.rs` and `settings/network_doctor.rs`,
//! TSX slots `src/pages/settings/sections/ghMirror.tsx` and
//! `sections/networkDoctor.tsx`). Pre-registered empty by the settings-base
//! wave so the slice never touches a shared registry — it only adds methods
//! inside this file. Do not edit `settings.rs` (the base seam) or the
//! `lib.rs` / `facades/mod.rs` registration.
//!
//! Domain seams this slice wires (already dependencies of this crate):
//! - `ss_core::config::github_mirror::{load_config, save_config,
//!   builtin_presets, CUSTOM_ENTRY_ID}` — the ordered fallback chain, the
//!   enable switch and the custom-URL field (gh_mirror.rs save_gh_mirror
//!   parity; a fuzzy find renders `GitHubMirrorConfig::order`).
//! - `ss_core::config::network_doctor::{diagnose_network, NetworkDiagnosis,
//!   NetworkHostCheck}` — the run button's async diagnosis (network_doctor.rs
//!   run_diagnosis parity; `diagnose_network` is a future, so the facade
//!   method is `async` and the TSX side rides the bridge-task wave's
//!   cancel/progress primitives if it needs them).
//!
//! Boundary notes:
//! - `save_mirror_config` is a thin domain passthrough: the domain
//!   normalizes `order`/`preset_id` on every write (missing ids appended,
//!   unknown ids dropped). The GPUI UI trims the custom URL before saving
//!   (gh_mirror.rs `save_gh_mirror`); the nsh TSX state does the same trim,
//!   so the facade keeps whatever it is handed verbatim.
//! - `test_mirror` and `diagnose_network` touch the real network
//!   (`probe_http_client`, TCP probes). Unit tests must not call them for
//!   real — the wire shape is asserted over the stub below.
//!
//! Test discipline (wave-1 lesson): any test touching the
//! `SKILLSTAR_DATA_DIR` sandbox borrows the shared `TEST_ENV_LOCK` from
//! `facades/mod.rs` for the whole sandbox lifetime — never a module-private
//! lock, never `--test-threads=1`. `diagnose_network` touches the network —
//! unit tests must not call it for real; assert the wire shape over a stub.

use nsh_core::prelude::*;
use nsh_core::JsonValue;

use ss_core::config::github_mirror::{self, GitHubMirrorConfig};
use ss_core::config::network_doctor::{self, NetworkDiagnosis};

/// The GitHub-mirror + network-doctor settings seam (`SettingsNetwork` in TSX).
#[module]
pub struct SettingsNetwork;

/// `NetworkDiagnosis` → wire JSON (network_doctor.rs renders these fields;
/// serde keeps the snake_case names, and the TSX DTO mirrors them
/// field-for-field). Lives here — not in the domain — so the doctor's wire
/// shape stays this file's contract (the slot header's "wire serialization
/// in this file" rule).
fn diagnosis_wire(diagnosis: &NetworkDiagnosis) -> JsonValue {
    serde_json::to_value(diagnosis).unwrap_or(JsonValue::Null)
}

#[module]
impl SettingsNetwork {
    /// Read the GitHub-mirror config (`github_mirror::load_config` parity —
    /// the identical domain call the GPUI page makes at construction).
    /// Missing file → defaults (disabled, `ghproxy_vip` leading a complete
    /// order); a stored order is normalized complete on read. Never errors:
    /// a failed read degrades to the defaults so the page always opens.
    /// Read-only.
    pub fn load_mirror_config() -> JsonValue {
        let config = github_mirror::load_config().unwrap_or_default();
        serde_json::to_value(&config).unwrap_or(JsonValue::Null)
    }

    /// The builtin mirror presets (`github_mirror::builtin_presets`
    /// parity), declaration order — both the `order` fallback and the
    /// id→name/url lookup for the settings rows. The custom-entry id
    /// (`CUSTOM_ENTRY_ID`, "custom") is a stable constant mirrored TSX-side,
    /// never a preset row. Read-only.
    pub fn mirror_presets() -> JsonValue {
        serde_json::to_value(github_mirror::builtin_presets())
            .unwrap_or(JsonValue::Array(Vec::new()))
    }

    /// Persist the mirror configuration (`github_mirror::save_config`
    /// parity — atomic write via `ss_core::infra::fs_ops`, plus the
    /// `github_health` reset so a reconfigured dead mirror is retried).
    /// Accepts the same JSON shape `load_mirror_config` returns; the domain
    /// re-derives `preset_id` from `order[0]` and appends missing entries.
    pub fn save_mirror_config(config: JsonValue) -> Result<(), String> {
        let parsed: GitHubMirrorConfig =
            serde_json::from_value(config).map_err(|err| format!("{err:#}"))?;
        github_mirror::save_config(&parsed).map_err(|err| format!("{err:#}"))
    }

    /// Probe one mirror through a tiny public raw file
    /// (`github_mirror::test_mirror` parity, gh_mirror.rs `gh_test_button`).
    /// Resolves the latency in ms; a non-2xx or unexpected body rejects
    /// (and records a circuit-breaker failure). Hits the real network —
    /// never call from unit tests.
    pub async fn test_mirror(url: String) -> Result<u64, String> {
        github_mirror::test_mirror(&url)
            .await
            .map_err(|err| format!("{err:#}"))
    }

    /// One-shot network diagnosis (`network_doctor::diagnose_network`
    /// parity, network_doctor.rs `run_diagnosis`): probes the proxy
    /// listener, direct GitHub/API/skills.sh and each configured mirror.
    /// Resolves the `NetworkDiagnosis` wire shape (`proxy_enabled`,
    /// `proxy_type`, `checks[]` with `id/label/url/status/latency_ms/detail`,
    /// `recommendations[]` machine keys — the UI maps them through i18n).
    /// Hits the real network — never call from unit tests.
    pub async fn diagnose_network() -> Result<JsonValue, String> {
        let diagnosis = network_doctor::diagnose_network()
            .await
            .map_err(|err| format!("{err:#}"))?;
        Ok(diagnosis_wire(&diagnosis))
    }
}

#[cfg(test)]
mod tests {
    use super::{SettingsNetwork, diagnosis_wire};
    use nsh_core::JsonValue;
    use ss_core::config::network_doctor::{NetworkDiagnosis, NetworkHostCheck};

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
                "skillstar-nsh-settings-network-{}-{}",
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

    fn str_field<'a>(value: &'a JsonValue, key: &str) -> Option<&'a str> {
        value.get(key).and_then(|item| item.as_str())
    }

    /// gh_mirror.rs `gh_mirror_order` fallback contract: no file on disk
    /// loads the *raw* domain default — `order` stays null and the TSX side
    /// falls back to the builtin-preset id order (exactly what
    /// `SettingsPage::gh_mirror_order` does in GPUI). Never an error.
    #[test]
    fn mirror_defaults_load_the_raw_domain_default() {
        let _iso = IsolatedDataDir::new();
        let config = SettingsNetwork::load_mirror_config();
        assert_eq!(config.get("enabled").and_then(|item| item.as_bool()), Some(false));
        assert_eq!(str_field(&config, "preset_id"), Some("ghproxy_vip"));
        assert_eq!(
            config.get("custom_url").map(|item| item.is_null()),
            Some(true)
        );
        assert_eq!(
            config.get("order").map(|item| item.is_null()),
            Some(true),
            "a missing file is not normalized — the UI derives the order from the presets"
        );
    }

    /// save_gh_mirror parity through the domain write: the stored order
    /// round-trips (first entry is the selection, missing presets appended)
    /// and `preset_id` is re-derived — the UI keeps only `order`.
    #[test]
    fn mirror_save_round_trips_the_user_order() {
        let _iso = IsolatedDataDir::new();
        let config: JsonValue = serde_json::json!({
            "enabled": true,
            "preset_id": null,
            "custom_url": "https://mirror.example",
            "order": ["gh_proxy_com", "custom", "ghproxy_vip"],
        });
        SettingsNetwork::save_mirror_config(config).unwrap();

        let loaded = SettingsNetwork::load_mirror_config();
        assert_eq!(loaded.get("enabled").and_then(|item| item.as_bool()), Some(true));
        assert_eq!(
            str_field(&loaded, "preset_id"),
            Some("gh_proxy_com"),
            "preset_id is order[0] unless it is the custom entry"
        );
        assert_eq!(
            str_field(&loaded, "custom_url"),
            Some("https://mirror.example"),
            "the facade passes the URL through verbatim (trim is TS-side)"
        );
        let order: Vec<String> = loaded
            .get("order")
            .and_then(|item| item.as_array())
            .map(|ids| {
                ids.iter()
                    .filter_map(|id| id.as_str().map(str::to_owned))
                    .collect()
            })
            .unwrap_or_default();
        assert_eq!(
            &order[..3],
            &["gh_proxy_com", "custom", "ghproxy_vip"],
            "the user's relative order is authoritative"
        );
        assert_eq!(order.len(), 5, "omitted presets are appended");
    }

    /// A non-object payload is rejected instead of silently writing garbage.
    #[test]
    fn mirror_save_rejects_a_wrong_shape_payload() {
        SettingsNetwork::save_mirror_config(JsonValue::Bool(true))
            .err()
            .expect("bool is not a GitHubMirrorConfig");
    }

    /// The preset wire: four builtin hosts, https with a trailing slash,
    /// and never the custom-entry id (that row is UI-only).
    #[test]
    fn mirror_presets_wire_lists_the_builtin_hosts() {
        let presets = SettingsNetwork::mirror_presets();
        let rows = presets
            .as_array()
            .expect("presets serialize as an array");
        assert_eq!(rows.len(), 4);
        for row in rows {
            let id = str_field(row, "id").unwrap_or_default();
            assert_ne!(id, "custom");
            assert!(str_field(row, "url").unwrap_or_default().starts_with("https://"));
            assert!(str_field(row, "url").unwrap_or_default().ends_with('/'));
            assert!(!str_field(row, "name").unwrap_or_default().is_empty());
            assert_eq!(row.get("supports_clone").and_then(|v| v.as_bool()), Some(true));
        }
    }

    /// The doctor's wire shape over a stub (the slot's test discipline:
    /// `diagnose_network` hits the real network, so its serialization is
    /// asserted on a hand-built `NetworkDiagnosis`, not a live call).
    #[test]
    fn diagnosis_wire_shape_over_a_stub() {
        let stub = NetworkDiagnosis {
            proxy_enabled: true,
            proxy_type: Some("socks5".into()),
            checks: vec![
                NetworkHostCheck {
                    id: "github".into(),
                    label: "GitHub".into(),
                    url: "https://github.com/".into(),
                    status: "ok".into(),
                    latency_ms: Some(42),
                    detail: Some("HTTP 200".into()),
                },
                NetworkHostCheck {
                    id: "proxy".into(),
                    label: "Proxy".into(),
                    url: String::new(),
                    status: "skip".into(),
                    latency_ms: None,
                    detail: Some("proxy disabled".into()),
                },
            ],
            recommendations: vec!["enable_github_mirrors".into()],
        };
        let wire = diagnosis_wire(&stub);
        assert_eq!(wire.get("proxy_enabled").and_then(|v| v.as_bool()), Some(true));
        assert_eq!(str_field(&wire, "proxy_type"), Some("socks5"));
        let checks = wire
            .get("checks")
            .and_then(|v| v.as_array())
            .expect("checks array");
        assert_eq!(checks.len(), 2);
        let first = &checks[0];
        assert_eq!(str_field(first, "id"), Some("github"));
        assert_eq!(str_field(first, "status"), Some("ok"));
        assert_eq!(first.get("latency_ms").and_then(|v| v.as_u64()), Some(42));
        assert_eq!(str_field(first, "detail"), Some("HTTP 200"));
        let second = &checks[1];
        assert_eq!(str_field(second, "status"), Some("skip"));
        assert!(
            second.get("latency_ms").map(|v| v.is_null()).unwrap_or(false),
            "an absent latency serializes as JSON null"
        );
        let recs = wire
            .get("recommendations")
            .and_then(|v| v.as_array())
            .expect("recommendations array");
        assert_eq!(recs[0].as_str(), Some("enable_github_mirrors"));
    }
}
