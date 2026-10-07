//! GUI-local preferences (language, background style, background-run).
//! Ponytail store — one JSON file under `~/.skillstar/config/` so it
//! participates in the existing backup layout. Tauri parity: React used
//! `localStorage`; here we use a real file so the CLI/`skillstar` binary
//! could read it too if needed.

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct GuiPrefs {
    /// `"zh-CN"` or `"en"` — matches `src/i18n` language codes.
    pub language: String,
    /// `"current"` or `"paper"` — matches `src/lib/backgroundStyle`.
    pub background_style: String,
    /// Whether the window can hide to tray on close instead of quitting.
    pub background_run: bool,
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

fn prefs_path() -> PathBuf {
    ss_core::infra::paths::config_dir().join("gui_prefs.json")
}

pub fn load() -> GuiPrefs {
    let path = prefs_path();
    let Ok(text) = std::fs::read_to_string(&path) else {
        return GuiPrefs::default();
    };
    serde_json::from_str(&text).unwrap_or_default()
}

pub fn save(prefs: &GuiPrefs) -> Result<()> {
    let path = prefs_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&path, serde_json::to_string_pretty(prefs)?)?;
    Ok(())
}
