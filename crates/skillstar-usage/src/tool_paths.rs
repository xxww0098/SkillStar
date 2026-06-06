//! Well-known on-disk paths for IDE / CLI credential stores (default install only).

use std::path::PathBuf;

use skillstar_core::infra::paths::home_dir;

pub fn codex_auth_path() -> PathBuf {
    home_dir().join(".codex").join("auth.json")
}

pub fn antigravity_user_data_dir() -> Option<PathBuf> {
    #[cfg(target_os = "windows")]
    {
        let appdata = std::env::var("APPDATA").ok()?;
        return Some(PathBuf::from(appdata).join("Antigravity IDE"));
    }
    #[cfg(target_os = "macos")]
    {
        return Some(
            home_dir()
                .join("Library")
                .join("Application Support")
                .join("Antigravity IDE"),
        );
    }
    #[cfg(target_os = "linux")]
    {
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

pub fn antigravity_state_db_path() -> Option<PathBuf> {
    antigravity_user_data_dir()
        .map(|root| root.join("User").join("globalStorage").join("state.vscdb"))
}

pub fn qoder_user_data_dir() -> PathBuf {
    #[cfg(target_os = "macos")]
    {
        home_dir().join("Library/Application Support/Qoder")
    }
    #[cfg(target_os = "windows")]
    {
        let appdata = std::env::var("APPDATA").unwrap_or_else(|_| String::new());
        return PathBuf::from(appdata).join("Qoder");
    }
    #[cfg(target_os = "linux")]
    {
        return home_dir().join(".config/Qoder");
    }
}

pub fn qoder_state_db_path() -> PathBuf {
    qoder_user_data_dir()
        .join("User")
        .join("globalStorage")
        .join("state.vscdb")
}

pub fn qoder_machine_token_path() -> PathBuf {
    qoder_user_data_dir()
        .join("SharedClientCache")
        .join("cache")
        .join("machine_token.json")
}
