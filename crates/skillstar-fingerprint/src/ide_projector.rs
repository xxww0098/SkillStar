//! Project a [`DeviceFingerprint`]'s [`IdeTelemetry`] onto a real IDE on disk.
//!
//! Each supported IDE is a VS Code fork; they all read the same four
//! `telemetry.*` fields out of `<userdata>/User/globalStorage/storage.json`
//! on launch. Writing a fingerprint into one of those files makes that IDE
//! present a fresh device identity to its analytics pipeline.
//!
//! Before the first write we copy the *current* `storage.json` (or, when
//! missing, just the telemetry slice) to
//! `~/.skillstar/fingerprints/baselines/<ide>.json` so [`IdeProjector::restore_baseline`]
//! can put the original device identity back.

use crate::telemetry::IdeTelemetry;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::PathBuf;
use thiserror::Error;

/// Errors raised by the projector layer.
#[derive(Debug, Error)]
pub enum ProjectorError {
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("json: {0}")]
    Json(#[from] serde_json::Error),
    #[error("IDE `{0}` is not installed on this machine")]
    NotInstalled(String),
    #[error("no baseline saved for `{0}` yet")]
    NoBaseline(String),
    #[error("config home unavailable: {0}")]
    ConfigHome(String),
    #[error("storage path missing for `{0}`")]
    MissingStorage(String),
}

/// What the UI needs to know about a projector before asking it to do anything.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SupportedIde {
    pub agent_id: String,
    pub display_name: String,
    pub installed: bool,
    pub storage_path: Option<String>,
    pub has_baseline: bool,
    /// Current telemetry currently on disk (best-effort; `None` if unreadable).
    pub current: Option<IdeTelemetry>,
}

/// Status flag for a single agent — used by the Tauri list endpoint.
#[derive(Debug, Clone, Copy)]
pub enum IdeStatus {
    Installed,
    NotInstalled,
}

/// Behaviour implemented by every IDE-specific projector.
pub trait IdeProjector: Send + Sync {
    /// Stable id, e.g. `"cursor"`. Matches SkillStar's AgentId conventions.
    fn agent_id(&self) -> &'static str;
    /// Human label, e.g. `"Cursor"`.
    fn display_name(&self) -> &'static str;

    /// Path to the storage.json this projector reads/writes.
    /// `None` when we can't even determine where to look (rare).
    fn storage_path(&self) -> Option<PathBuf>;

    fn is_installed(&self) -> bool {
        self.storage_path().map(|p| p.exists()).unwrap_or(false)
    }

    /// Path under SkillStar's data dir where the original telemetry slice
    /// is persisted. Defaults to `~/.skillstar/fingerprints/baselines/<id>.json`.
    fn baseline_path(&self) -> Result<PathBuf, ProjectorError> {
        let home = home_dir()?;
        Ok(home
            .join(".skillstar")
            .join("fingerprints")
            .join("baselines")
            .join(format!("{}.json", self.agent_id())))
    }

    /// `true` when [`baseline_path`] exists.
    fn has_baseline(&self) -> bool {
        self.baseline_path().map(|p| p.exists()).unwrap_or(false)
    }

    /// Read the IDE's current `telemetry.*` fields. Returns an empty
    /// telemetry when the file doesn't exist.
    fn read_current(&self) -> Result<IdeTelemetry, ProjectorError> {
        let Some(path) = self.storage_path() else {
            return Err(ProjectorError::MissingStorage(self.agent_id().to_string()));
        };
        if !path.exists() {
            return Ok(IdeTelemetry::default());
        }
        let bytes = std::fs::read(&path)?;
        if bytes.is_empty() {
            return Ok(IdeTelemetry::default());
        }
        let value: Value = serde_json::from_slice(&bytes)?;
        Ok(extract_telemetry(&value))
    }

    /// Save the IDE's current telemetry as a baseline (idempotent — only
    /// writes the first time it's called).
    fn backup_baseline(&self) -> Result<(), ProjectorError> {
        let baseline = self.baseline_path()?;
        if baseline.exists() {
            return Ok(()); // already snapshotted
        }
        if let Some(parent) = baseline.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let current = self.read_current()?;
        let json = serde_json::to_vec_pretty(&current)?;
        std::fs::write(&baseline, json)?;
        Ok(())
    }

    /// Overwrite the IDE's `telemetry.*` fields with `wanted`.
    ///
    /// `wanted`'s `None` entries are passed through (i.e. existing fields
    /// in storage.json are kept). Calls [`backup_baseline`] first so the
    /// original identity is recoverable.
    fn apply(&self, wanted: &IdeTelemetry) -> Result<(), ProjectorError> {
        let Some(path) = self.storage_path() else {
            return Err(ProjectorError::MissingStorage(self.agent_id().to_string()));
        };
        if !path.exists() {
            return Err(ProjectorError::NotInstalled(self.agent_id().to_string()));
        }
        self.backup_baseline()?;

        let bytes = std::fs::read(&path)?;
        let mut root: Value = if bytes.is_empty() {
            Value::Object(Default::default())
        } else {
            serde_json::from_slice(&bytes)?
        };
        inject_telemetry(&mut root, wanted);
        let serialized = serde_json::to_vec_pretty(&root)?;
        atomic_write(&path, &serialized)?;
        tracing::info!(
            target: "skillstar_fingerprint::projector",
            ide = self.agent_id(),
            "telemetry applied",
        );
        Ok(())
    }

    /// Put the original telemetry (captured by [`backup_baseline`]) back on disk.
    fn restore_baseline(&self) -> Result<(), ProjectorError> {
        let baseline = self.baseline_path()?;
        if !baseline.exists() {
            return Err(ProjectorError::NoBaseline(self.agent_id().to_string()));
        }
        let raw = std::fs::read(&baseline)?;
        let original: IdeTelemetry = serde_json::from_slice(&raw)?;
        self.apply(&original)?;
        tracing::info!(
            target: "skillstar_fingerprint::projector",
            ide = self.agent_id(),
            "telemetry restored from baseline",
        );
        Ok(())
    }

    /// One-call summary used by the Tauri list endpoint.
    fn summary(&self) -> SupportedIde {
        SupportedIde {
            agent_id: self.agent_id().to_string(),
            display_name: self.display_name().to_string(),
            installed: self.is_installed(),
            storage_path: self.storage_path().map(|p| p.to_string_lossy().to_string()),
            has_baseline: self.has_baseline(),
            current: self.read_current().ok(),
        }
    }
}

// ── VS Code-fork projector ───────────────────────────────────────────

/// Generic projector for any VS Code-style IDE that stores telemetry
/// inside `<userdata>/User/globalStorage/storage.json`. Four variants
/// ship out of the box: Cursor / Windsurf / Kiro / Antigravity IDE.
pub struct VsCodeForkProjector {
    pub agent_id: &'static str,
    pub display_name: &'static str,
    /// Folder name under the OS user-data root (e.g. `"Cursor"`).
    pub macos_dir: &'static str,
    pub windows_dir: &'static str,
    pub linux_dir: &'static str,
}

impl VsCodeForkProjector {
    /// All supported VS Code-fork IDEs in display order.
    pub const fn all() -> &'static [Self] {
        &[
            Self {
                agent_id: "cursor",
                display_name: "Cursor",
                macos_dir: "Cursor",
                windows_dir: "Cursor",
                linux_dir: "Cursor",
            },
            Self {
                agent_id: "windsurf",
                display_name: "Windsurf",
                macos_dir: "Windsurf",
                windows_dir: "Windsurf",
                linux_dir: "Windsurf",
            },
            Self {
                agent_id: "kiro",
                display_name: "Kiro",
                macos_dir: "Kiro",
                windows_dir: "Kiro",
                linux_dir: "Kiro",
            },
            Self {
                agent_id: "antigravity",
                display_name: "Antigravity IDE",
                macos_dir: "Antigravity IDE",
                windows_dir: "Antigravity IDE",
                linux_dir: "Antigravity IDE",
            },
        ]
    }

    fn user_data_dir(&self) -> Option<PathBuf> {
        let home = home_dir().ok()?;
        #[cfg(target_os = "macos")]
        return Some(
            home.join("Library/Application Support")
                .join(self.macos_dir),
        );
        #[cfg(target_os = "windows")]
        return std::env::var_os("APPDATA")
            .map(|appdata| PathBuf::from(appdata).join(self.windows_dir));
        #[cfg(target_os = "linux")]
        return Some(home.join(".config").join(self.linux_dir));
        #[allow(unreachable_code)]
        {
            let _ = home;
            None
        }
    }
}

impl IdeProjector for VsCodeForkProjector {
    fn agent_id(&self) -> &'static str {
        self.agent_id
    }
    fn display_name(&self) -> &'static str {
        self.display_name
    }
    fn storage_path(&self) -> Option<PathBuf> {
        Some(
            self.user_data_dir()?
                .join("User")
                .join("globalStorage")
                .join("storage.json"),
        )
    }
}

// ── helpers ──────────────────────────────────────────────────────────

fn home_dir() -> Result<PathBuf, ProjectorError> {
    if let Ok(v) = std::env::var("SKILLSTAR_HOME")
        && !v.is_empty()
    {
        return Ok(PathBuf::from(v));
    }
    if let Ok(v) = std::env::var("HOME")
        && !v.is_empty()
    {
        return Ok(PathBuf::from(v));
    }
    if let Ok(v) = std::env::var("USERPROFILE")
        && !v.is_empty()
    {
        return Ok(PathBuf::from(v));
    }
    Err(ProjectorError::ConfigHome(
        "neither $HOME nor %USERPROFILE% is set".to_string(),
    ))
}

/// Extract telemetry fields from a parsed `storage.json` Value.
///
/// VS Code historically wrote *both* nested (`telemetry.machineId` inside
/// `telemetry`) and flat-keyed (`"telemetry.machineId"` at the root)
/// versions. We try nested first, then flat.
fn extract_telemetry(value: &Value) -> IdeTelemetry {
    let pick = |key: &str| -> Option<String> {
        if let Some(obj) = value.get("telemetry").and_then(|v| v.as_object())
            && let Some(v) = obj.get(key).and_then(|v| v.as_str())
        {
            return Some(v.to_string());
        }
        value
            .get(format!("telemetry.{key}"))
            .and_then(|v| v.as_str())
            .map(String::from)
    };
    IdeTelemetry {
        machine_id: pick("machineId"),
        mac_machine_id: pick("macMachineId"),
        dev_device_id: pick("devDeviceId"),
        sqm_id: pick("sqmId"),
        service_machine_id: value
            .get("storage.serviceMachineId")
            .and_then(|v| v.as_str())
            .map(String::from),
        installation_id: pick("installationId"),
    }
}

/// Write whichever telemetry fields are `Some(_)` into the parsed value
/// (both nested under `telemetry` and at flat `telemetry.X` keys to satisfy
/// older readers).
fn inject_telemetry(value: &mut Value, t: &IdeTelemetry) {
    // Ensure top-level is an object.
    if !value.is_object() {
        *value = Value::Object(Default::default());
    }
    let obj = value.as_object_mut().unwrap();

    // Ensure nested `telemetry` exists.
    if !obj.get("telemetry").map(|v| v.is_object()).unwrap_or(false) {
        obj.insert("telemetry".to_string(), Value::Object(Default::default()));
    }
    let nested = obj
        .get_mut("telemetry")
        .and_then(|v| v.as_object_mut())
        .expect("telemetry was just ensured to be an object");

    let set = |obj: &mut serde_json::Map<String, Value>, key: &str, val: &Option<String>| {
        if let Some(v) = val {
            obj.insert(key.to_string(), Value::String(v.clone()));
        }
    };
    set(nested, "machineId", &t.machine_id);
    set(nested, "macMachineId", &t.mac_machine_id);
    set(nested, "devDeviceId", &t.dev_device_id);
    set(nested, "sqmId", &t.sqm_id);
    if let Some(v) = &t.installation_id {
        nested.insert("installationId".into(), Value::String(v.clone()));
    }

    // Flat keys (legacy).
    let flat_set = |obj: &mut serde_json::Map<String, Value>, key: &str, val: &Option<String>| {
        if let Some(v) = val {
            obj.insert(format!("telemetry.{key}"), Value::String(v.clone()));
        }
    };
    flat_set(obj, "machineId", &t.machine_id);
    flat_set(obj, "macMachineId", &t.mac_machine_id);
    flat_set(obj, "devDeviceId", &t.dev_device_id);
    flat_set(obj, "sqmId", &t.sqm_id);

    if let Some(v) = &t.service_machine_id {
        obj.insert("storage.serviceMachineId".into(), Value::String(v.clone()));
    }
}

fn atomic_write(path: &std::path::Path, bytes: &[u8]) -> Result<(), ProjectorError> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, bytes)?;
    std::fs::rename(&tmp, path)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    /// Concrete projector that writes to a tmpdir so we don't touch real IDEs.
    struct FakeProjector {
        path: PathBuf,
    }
    impl IdeProjector for FakeProjector {
        fn agent_id(&self) -> &'static str {
            "fake"
        }
        fn display_name(&self) -> &'static str {
            "Fake"
        }
        fn storage_path(&self) -> Option<PathBuf> {
            Some(self.path.clone())
        }
    }

    #[test]
    fn apply_and_restore_roundtrip() {
        let dir = tempdir().unwrap();
        // Force baselines to land in dir too. `set_var` is `unsafe` on the
        // current edition because env vars are process-wide; in test code
        // we accept that risk.
        unsafe {
            std::env::set_var("SKILLSTAR_HOME", dir.path());
        }

        let storage = dir.path().join("storage.json");
        std::fs::write(
            &storage,
            br#"{"telemetry":{"machineId":"orig-machine","macMachineId":"orig-mac","devDeviceId":"orig-dev","sqmId":"{ORIG-SQM}"}}"#,
        )
        .unwrap();

        let p = FakeProjector {
            path: storage.clone(),
        };

        // Apply new identity.
        let wanted = IdeTelemetry::generate();
        p.apply(&wanted).unwrap();
        let after = p.read_current().unwrap();
        assert_eq!(after.machine_id, wanted.machine_id);
        assert!(p.has_baseline());

        // Restore — should put orig-* back.
        p.restore_baseline().unwrap();
        let restored = p.read_current().unwrap();
        assert_eq!(restored.machine_id.as_deref(), Some("orig-machine"));

        unsafe {
            std::env::remove_var("SKILLSTAR_HOME");
        }
    }

    #[test]
    fn extract_handles_flat_keys_only() {
        let v: Value = serde_json::from_str(
            r#"{"telemetry.machineId":"flat-only","telemetry.macMachineId":"flat-mac"}"#,
        )
        .unwrap();
        let t = extract_telemetry(&v);
        assert_eq!(t.machine_id.as_deref(), Some("flat-only"));
        assert_eq!(t.mac_machine_id.as_deref(), Some("flat-mac"));
    }
}
