//! Well-known on-disk paths for IDE / CLI credential stores (default install only).

use std::path::{Path, PathBuf};

use skillstar_core::infra::paths::home_dir;

use crate::trae_platform::TraePlatformKind;

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

/// Host OS for IDE config roots. The path table test constructs every variant;
/// production only constructs the cfg-selected host.
#[cfg_attr(not(test), allow(dead_code))]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum DesktopOs {
    Macos,
    Windows,
    Linux,
}

fn current_desktop_os() -> Option<DesktopOs> {
    #[cfg(target_os = "macos")]
    {
        return Some(DesktopOs::Macos);
    }
    #[cfg(target_os = "windows")]
    {
        return Some(DesktopOs::Windows);
    }
    #[cfg(target_os = "linux")]
    {
        return Some(DesktopOs::Linux);
    }
    #[allow(unreachable_code)]
    None
}

/// IDE user-data root. Sandbox (`SKILLSTAR_TOOL_SYNC_HOME`) wins over
/// `APPDATA` / `XDG_CONFIG_HOME` / the real home, same as Antigravity.
fn ide_user_data_dir(os: DesktopOs, app_dir: &str) -> Option<PathBuf> {
    match os {
        DesktopOs::Macos => Some(
            tool_config_home()
                .join("Library")
                .join("Application Support")
                .join(app_dir),
        ),
        DesktopOs::Windows => {
            if is_tool_sync_sandboxed() {
                return Some(
                    tool_config_home()
                        .join("AppData")
                        .join("Roaming")
                        .join(app_dir),
                );
            }
            let appdata = std::env::var("APPDATA").ok()?;
            let trimmed = appdata.trim();
            if trimmed.is_empty() {
                return None;
            }
            Some(PathBuf::from(trimmed).join(app_dir))
        }
        DesktopOs::Linux => {
            if !is_tool_sync_sandboxed()
                && let Ok(xdg) = std::env::var("XDG_CONFIG_HOME")
            {
                let trimmed = xdg.trim();
                if !trimmed.is_empty() {
                    return Some(PathBuf::from(trimmed).join(app_dir));
                }
            }
            Some(tool_config_home().join(".config").join(app_dir))
        }
    }
}

fn global_state_db(os: DesktopOs, app_dir: &str) -> Option<PathBuf> {
    ide_user_data_dir(os, app_dir)
        .map(|root| root.join("User").join("globalStorage").join("state.vscdb"))
}

pub fn windsurf_state_db_path() -> Option<PathBuf> {
    global_state_db(current_desktop_os()?, "Windsurf")
}

/// Kiro's Electron user-data directory (`…/Kiro`), not the AWS cache.
pub fn kiro_data_dir() -> Option<PathBuf> {
    ide_user_data_dir(current_desktop_os()?, "Kiro")
}

/// `~/.aws/sso/cache` on every OS. Cockpit reads `kiro-auth-token.json` here,
/// not under the Kiro user-data directory.
pub fn aws_sso_cache_dir() -> PathBuf {
    tool_config_home().join(".aws").join("sso").join("cache")
}

/// First existing candidate wins, same order as cockpit
/// `ensure_state_db_path_for_user_data_dir`. Does not create or copy a database.
pub fn qoder_state_db_path() -> Option<PathBuf> {
    let root = ide_user_data_dir(current_desktop_os()?, "Qoder")?;
    Some(resolve_qoder_state_db(&root))
}

fn resolve_qoder_state_db(root: &Path) -> PathBuf {
    let preferred = root.join("User").join("globalStorage").join("state.vscdb");
    let candidates = [
        preferred.clone(),
        root.join("globalStorage").join("state.vscdb"),
        root.join("state.vscdb"),
    ];
    candidates
        .into_iter()
        .find(|path| path.exists())
        .unwrap_or(preferred)
}

pub fn trae_storage_path_for(platform: TraePlatformKind) -> Option<PathBuf> {
    ide_user_data_dir(current_desktop_os()?, platform.app_support_dir_name())
        .map(|root| root.join("User").join("globalStorage").join("storage.json"))
}

pub fn codebuddy_state_db_path() -> Option<PathBuf> {
    global_state_db(current_desktop_os()?, "CodeBuddy")
}

pub fn codebuddy_cn_state_db_path() -> Option<PathBuf> {
    global_state_db(current_desktop_os()?, "CodeBuddy CN")
}

/// ZCode data root. Default is `~/.zcode`.
///
/// Cockpit reads `dataBaseDir` from `~/.zcode/v2/setting.json` (no trailing
/// "s") and treats that value as a replacement home: the root becomes
/// `{dataBaseDir}/.zcode`.
pub fn zcode_home() -> PathBuf {
    let default_root = tool_config_home().join(".zcode");
    overridden_zcode_root(&default_root.join("v2").join(ZCODE_SETTINGS_FILE))
        .unwrap_or(default_root)
}

const ZCODE_SETTINGS_FILE: &str = "setting.json";

fn overridden_zcode_root(settings: &Path) -> Option<PathBuf> {
    let content = std::fs::read_to_string(settings).ok()?;
    let value: serde_json::Value = serde_json::from_str(&content).ok()?;
    let base = value.get("dataBaseDir")?.as_str()?.trim();
    if base.is_empty() {
        return None;
    }
    Some(PathBuf::from(base).join(".zcode"))
}

#[cfg(test)]
mod tests {
    use super::*;

    const STATE_DB: &[&str] = &["User", "globalStorage", "state.vscdb"];
    const TRAE_STORAGE: &[&str] = &["User", "globalStorage", "storage.json"];
    const OSES: [DesktopOs; 3] = [DesktopOs::Macos, DesktopOs::Windows, DesktopOs::Linux];

    struct EnvGuard {
        _lock: std::sync::MutexGuard<'static, ()>,
        tool_sync: Option<std::ffi::OsString>,
        appdata: Option<std::ffi::OsString>,
        xdg: Option<std::ffi::OsString>,
    }

    impl EnvGuard {
        fn sandbox(path: &Path) -> Self {
            let lock = crate::test_env_lock()
                .lock()
                .unwrap_or_else(|error| error.into_inner());
            let tool_sync = std::env::var_os("SKILLSTAR_TOOL_SYNC_HOME");
            let appdata = std::env::var_os("APPDATA");
            let xdg = std::env::var_os("XDG_CONFIG_HOME");
            // SAFETY: serialized by the crate-wide test_env_lock.
            unsafe {
                std::env::set_var("SKILLSTAR_TOOL_SYNC_HOME", path);
                std::env::set_var("APPDATA", path.join("poison-appdata"));
                std::env::set_var("XDG_CONFIG_HOME", path.join("poison-xdg"));
            }
            Self {
                _lock: lock,
                tool_sync,
                appdata,
                xdg,
            }
        }
    }

    impl Drop for EnvGuard {
        fn drop(&mut self) {
            restore("SKILLSTAR_TOOL_SYNC_HOME", self.tool_sync.as_deref());
            restore("APPDATA", self.appdata.as_deref());
            restore("XDG_CONFIG_HOME", self.xdg.as_deref());
        }
    }

    fn restore(key: &str, prev: Option<&std::ffi::OsStr>) {
        // SAFETY: caller holds the crate-wide test_env_lock.
        unsafe {
            match prev {
                Some(value) => std::env::set_var(key, value),
                None => std::env::remove_var(key),
            }
        }
    }

    fn expect(root: &Path, os: DesktopOs, app_dir: &str, tail: &[&str]) -> PathBuf {
        let base = match os {
            DesktopOs::Macos => root
                .join("Library")
                .join("Application Support")
                .join(app_dir),
            DesktopOs::Windows => root.join("AppData").join("Roaming").join(app_dir),
            DesktopOs::Linux => root.join(".config").join(app_dir),
        };
        tail.iter().fold(base, |path, part| path.join(part))
    }

    fn resolved(os: DesktopOs, app_dir: &str, tail: &[&str]) -> PathBuf {
        let mut path = ide_user_data_dir(os, app_dir).expect("ide dir");
        for part in tail {
            path.push(part);
        }
        path
    }

    #[test]
    fn sandboxed_path_table_covers_every_app_on_every_os() {
        let dir = tempfile::tempdir().expect("tempdir");
        let sandbox = dir.path();
        let _guard = EnvGuard::sandbox(sandbox);
        let real_home = home_dir();
        assert_ne!(sandbox, real_home.as_path());

        let ide_rows: &[(&str, &str, &[&str])] = &[
            ("windsurf", "Windsurf", STATE_DB),
            ("kiro", "Kiro", &[]),
            ("qoder", "Qoder", STATE_DB),
            ("codebuddy", "CodeBuddy", STATE_DB),
            ("codebuddy-cn", "CodeBuddy CN", STATE_DB),
            ("trae", "Trae", TRAE_STORAGE),
            ("trae-solo", "TRAE SOLO", TRAE_STORAGE),
            ("trae-cn", "Trae CN", TRAE_STORAGE),
            ("trae-solo-cn", "TRAE SOLO CN", TRAE_STORAGE),
        ];

        for os in OSES {
            for (_name, app_dir, tail) in ide_rows {
                let path = resolved(os, app_dir, tail);
                let expected = expect(sandbox, os, app_dir, tail);
                assert_eq!(path, expected, "{app_dir} on {os:?}");
                assert!(path.starts_with(sandbox), "{path:?}");
                assert_ne!(path, expect(&real_home, os, app_dir, tail));
                assert!(!path.starts_with(sandbox.join("poison-appdata")));
                assert!(!path.starts_with(sandbox.join("poison-xdg")));
            }
            let cache = aws_sso_cache_dir();
            assert_eq!(cache, sandbox.join(".aws").join("sso").join("cache"));
            assert_ne!(
                cache,
                real_home.join(".aws").join("sso").join("cache"),
                "aws cache on {os:?}"
            );
            assert_eq!(zcode_home(), sandbox.join(".zcode"));
            assert_ne!(zcode_home(), real_home.join(".zcode"));
        }

        let host = current_desktop_os().expect("desktop os");
        assert_eq!(
            windsurf_state_db_path(),
            Some(expect(sandbox, host, "Windsurf", STATE_DB))
        );
        assert_eq!(kiro_data_dir(), Some(expect(sandbox, host, "Kiro", &[])));
        assert_eq!(
            qoder_state_db_path(),
            Some(expect(sandbox, host, "Qoder", STATE_DB))
        );
        assert_eq!(
            codebuddy_state_db_path(),
            Some(expect(sandbox, host, "CodeBuddy", STATE_DB))
        );
        assert_eq!(
            codebuddy_cn_state_db_path(),
            Some(expect(sandbox, host, "CodeBuddy CN", STATE_DB))
        );
        for kind in TraePlatformKind::ALL {
            assert_eq!(kind.app_support_dir_name(), kind.display_name());
            assert_eq!(
                trae_storage_path_for(kind),
                Some(expect(
                    sandbox,
                    host,
                    kind.app_support_dir_name(),
                    TRAE_STORAGE
                ))
            );
        }
    }

    #[test]
    fn qoder_state_db_prefers_the_first_existing_candidate() {
        let dir = tempfile::tempdir().expect("tempdir");
        let root = dir.path();
        let preferred = root.join("User").join("globalStorage").join("state.vscdb");
        let middle = root.join("globalStorage").join("state.vscdb");
        let flat = root.join("state.vscdb");

        assert_eq!(resolve_qoder_state_db(root), preferred);

        std::fs::write(&flat, b"flat").expect("flat");
        assert_eq!(resolve_qoder_state_db(root), flat);

        std::fs::create_dir_all(middle.parent().expect("parent")).expect("mkdir");
        std::fs::write(&middle, b"mid").expect("mid");
        assert_eq!(resolve_qoder_state_db(root), middle);

        std::fs::create_dir_all(preferred.parent().expect("parent")).expect("mkdir");
        std::fs::write(&preferred, b"pref").expect("pref");
        assert_eq!(resolve_qoder_state_db(root), preferred);

        let _guard = EnvGuard::sandbox(root);
        let host = current_desktop_os().expect("desktop os");
        let user_data = ide_user_data_dir(host, "Qoder").expect("qoder dir");
        let alternate = user_data.join("globalStorage").join("state.vscdb");
        std::fs::create_dir_all(alternate.parent().expect("parent")).expect("mkdir");
        std::fs::write(&alternate, b"alt").expect("alt");
        assert_eq!(qoder_state_db_path().expect("public path"), alternate);
    }

    #[test]
    fn zcode_home_prefers_setting_json_database_dir() {
        let dir = tempfile::tempdir().expect("tempdir");
        let sandbox = dir.path();
        let _guard = EnvGuard::sandbox(sandbox);
        assert_eq!(zcode_home(), sandbox.join(".zcode"));

        let v2 = sandbox.join(".zcode").join("v2");
        std::fs::create_dir_all(&v2).expect("mkdir");
        let override_root = sandbox.join("override");
        std::fs::write(
            v2.join("settings.json"),
            serde_json::to_string(&serde_json::json!({
                "dataBaseDir": override_root.to_string_lossy()
            }))
            .expect("json"),
        )
        .expect("wrong filename");
        assert_eq!(
            zcode_home(),
            sandbox.join(".zcode"),
            "settings.json is not cockpit's filename"
        );

        std::fs::write(v2.join("setting.json"), r#"{"dataBaseDir":"   "}"#).expect("blank");
        assert_eq!(zcode_home(), sandbox.join(".zcode"));

        std::fs::write(v2.join("setting.json"), "{not json").expect("invalid");
        assert_eq!(zcode_home(), sandbox.join(".zcode"));

        std::fs::write(
            v2.join("setting.json"),
            serde_json::to_string(&serde_json::json!({
                "dataBaseDir": override_root.to_string_lossy()
            }))
            .expect("json"),
        )
        .expect("override");
        assert_eq!(zcode_home(), override_root.join(".zcode"));
        assert_ne!(zcode_home(), home_dir().join(".zcode"));
    }
}
