//! Thin `#[tauri::command]` wrappers around `skillstar_app::shell_rc`.
//!
//! The real zshrc (de)serialization logic (idempotent write, atomic rename,
//! single backup) lives in the Tauri-agnostic `skillstar_app::shell_rc` module
//! so it stays testable and reusable outside of a Tauri command context. See
//! that module's docs for the full safety contract.

use skillstar_app::shell_rc::{ShellRcWriteResult, ensure_env_export, read_env_export};
use skillstar_core::infra::error::AppError;

/// Write `export <env_key>='<value>'` into `~/.zshrc` idempotently. Triggered
/// only by an explicit button in `CodexSettingsForm` (third_party auth mode).
#[tauri::command]
pub async fn write_codex_env_to_zshrc(
    env_key: String,
    value: String,
) -> Result<ShellRcWriteResult, AppError> {
    let home = skillstar_core::infra::paths::home_dir();
    ensure_env_export(&home, &env_key, &value)
}

/// Read the current value of `env_key` from `~/.zshrc`, if any. Powers the UI
/// "already written ✓" badge so the user knows whether a click is still needed.
#[tauri::command]
pub async fn read_codex_env_from_zshrc(env_key: String) -> Result<Option<String>, AppError> {
    let home = skillstar_core::infra::paths::home_dir();
    Ok(read_env_export(&home, &env_key))
}
