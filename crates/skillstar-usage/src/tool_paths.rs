//! Well-known on-disk paths for IDE / CLI credential stores (default install only).

use std::path::PathBuf;

use skillstar_core::infra::paths::home_dir;

const TOOL_SYNC_HOME_ENV: &str = "SKILLSTAR_TOOL_SYNC_HOME";

pub fn is_tool_sync_sandboxed() -> bool {
    std::env::var_os(TOOL_SYNC_HOME_ENV).is_some_and(|value| !value.is_empty())
}

fn tool_config_home() -> PathBuf {
    std::env::var_os(TOOL_SYNC_HOME_ENV)
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(home_dir)
}

pub fn codex_auth_path() -> PathBuf {
    home_dir().join(".codex").join("auth.json")
}

pub fn antigravity_user_data_dir() -> Option<PathBuf> {
    #[cfg(target_os = "windows")]
    {
        if is_tool_sync_sandboxed() {
            return Some(
                tool_config_home()
                    .join("AppData")
                    .join("Roaming")
                    .join("Antigravity IDE"),
            );
        }
        let appdata = std::env::var("APPDATA").ok()?;
        return Some(PathBuf::from(appdata).join("Antigravity IDE"));
    }
    #[cfg(target_os = "macos")]
    {
        return Some(
            tool_config_home()
                .join("Library")
                .join("Application Support")
                .join("Antigravity IDE"),
        );
    }
    #[cfg(target_os = "linux")]
    {
        if is_tool_sync_sandboxed() {
            return Some(tool_config_home().join(".config").join("Antigravity IDE"));
        }
        if let Ok(xdg) = std::env::var("XDG_CONFIG_HOME") {
            let trimmed = xdg.trim();
            if !trimmed.is_empty() {
                return Some(PathBuf::from(trimmed).join("Antigravity IDE"));
            }
        }
        return Some(home_dir().join(".config").join("Antigravity IDE"));
    }
    #[allow(unreachable_code)]
    None
}

/// Return the credential-store mode advertised by the installed Antigravity
/// desktop version when it can be determined without touching credentials.
/// Version 2.0 and newer use the official system credential store; older
/// desktop builds use the legacy `state.vscdb` row.
pub fn antigravity_prefers_system_credentials() -> Option<bool> {
    #[cfg(target_os = "macos")]
    {
        if is_tool_sync_sandboxed() {
            return None;
        }
        let plist =
            std::fs::read_to_string("/Applications/Antigravity.app/Contents/Info.plist").ok()?;
        let version = plist
            .split_once("<key>CFBundleShortVersionString</key>")?
            .1
            .split_once("<string>")?
            .1
            .split_once("</string>")?
            .0
            .trim();
        let major = version.split('.').next()?.parse::<u64>().ok()?;
        return Some(major >= 2);
    }
    #[allow(unreachable_code)]
    None
}

pub fn cursor_user_data_dir() -> Option<PathBuf> {
    #[cfg(target_os = "windows")]
    {
        if std::env::var_os(TOOL_SYNC_HOME_ENV).is_some() {
            return Some(
                tool_config_home()
                    .join("AppData")
                    .join("Roaming")
                    .join("Cursor"),
            );
        }
        let appdata = std::env::var("APPDATA").ok()?;
        return Some(PathBuf::from(appdata).join("Cursor"));
    }
    #[cfg(target_os = "macos")]
    {
        return Some(
            tool_config_home()
                .join("Library")
                .join("Application Support")
                .join("Cursor"),
        );
    }
    #[cfg(target_os = "linux")]
    {
        if std::env::var_os(TOOL_SYNC_HOME_ENV).is_none()
            && let Ok(xdg) = std::env::var("XDG_CONFIG_HOME")
            && !xdg.trim().is_empty()
        {
            return Some(PathBuf::from(xdg).join("Cursor"));
        }
        return Some(tool_config_home().join(".config").join("Cursor"));
    }
    #[allow(unreachable_code)]
    None
}

pub fn cursor_state_db_path() -> Option<PathBuf> {
    cursor_user_data_dir().map(|root| root.join("User").join("globalStorage").join("state.vscdb"))
}

pub fn antigravity_state_db_path() -> Option<PathBuf> {
    antigravity_user_data_dir()
        .map(|root| root.join("User").join("globalStorage").join("state.vscdb"))
}
