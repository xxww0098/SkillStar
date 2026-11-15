//! Where each client keeps the credential it serves, as pure functions over an
//! injected environment.
//!
//! Nothing here reads the real home directory: PathRoots::from_env is the only
//! function that looks at the process environment, and
//! SKILLSTAR_TOOL_SYNC_HOME always wins over every client's own override, so a
//! test can never read or write a developer's real ~/.codex, ~/.claude or
//! Cursor database. The path table itself is host-independent: the tests
//! construct every platform variant.

use std::path::PathBuf;

use ss_core::infra::paths::{home_dir, tool_sync_home_override};

/// Which desktop layout the paths follow.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DesktopOs {
    Macos,
    Windows,
    Linux,
}

/// The OS this binary was built for.
pub fn current_desktop_os() -> DesktopOs {
    #[cfg(target_os = "macos")]
    {
        DesktopOs::Macos
    }
    #[cfg(target_os = "windows")]
    {
        DesktopOs::Windows
    }
    #[cfg(target_os = "linux")]
    {
        DesktopOs::Linux
    }
}

/// Everything the client paths are resolved against.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PathRoots {
    /// SKILLSTAR_TOOL_SYNC_HOME: the sandbox root that replaces the home
    /// directory for external tool config. Always wins.
    pub sandbox_home: Option<PathBuf>,
    /// The user's home directory.
    pub home: PathBuf,
    /// CODEX_HOME, when the Codex CLI was told to keep its state elsewhere.
    pub codex_home: Option<PathBuf>,
    /// CLAUDE_CONFIG_DIR, when Claude Code was told to keep its state elsewhere.
    pub claude_config_dir: Option<PathBuf>,
    /// GROK_HOME, when the Grok CLI was told to keep its state elsewhere.
    pub grok_home: Option<PathBuf>,
    /// APPDATA (Windows only; absence falls back to <home>/AppData/Roaming).
    pub app_data: Option<PathBuf>,
    /// XDG_CONFIG_HOME (Linux only; absence falls back to <home>/.config).
    pub xdg_config_home: Option<PathBuf>,
    /// Which platform layout to follow.
    pub os: DesktopOs,
}

impl PathRoots {
    /// Resolve from the process environment.
    pub fn from_env() -> Self {
        Self {
            sandbox_home: tool_sync_home_override(),
            home: home_dir(),
            codex_home: non_empty_env("CODEX_HOME"),
            claude_config_dir: non_empty_env("CLAUDE_CONFIG_DIR"),
            grok_home: non_empty_env("GROK_HOME"),
            app_data: non_empty_env("APPDATA"),
            xdg_config_home: non_empty_env("XDG_CONFIG_HOME"),
            os: current_desktop_os(),
        }
    }

    /// Roots for one platform and home, with no overrides at all.
    pub fn for_os(os: DesktopOs, home: impl Into<PathBuf>) -> Self {
        Self {
            sandbox_home: None,
            home: home.into(),
            codex_home: None,
            claude_config_dir: None,
            grok_home: None,
            app_data: None,
            xdg_config_home: None,
            os,
        }
    }

    /// The home directory client config is resolved under: the sandbox when it
    /// is set, the real home otherwise.
    fn tool_home(&self) -> &std::path::Path {
        self.sandbox_home.as_deref().unwrap_or(self.home.as_path())
    }
}

fn non_empty_env(name: &str) -> Option<PathBuf> {
    std::env::var_os(name)
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
}

/// The Codex CLI's home: sandbox, then CODEX_HOME, then ~/.codex.
pub fn codex_home(roots: &PathRoots) -> PathBuf {
    if roots.sandbox_home.is_some() {
        return roots.tool_home().join(".codex");
    }
    if let Some(codex_home) = &roots.codex_home {
        return codex_home.clone();
    }
    roots.home.join(".codex")
}

/// The file Codex reads its OAuth credential from.
pub fn codex_auth_path(roots: &PathRoots) -> PathBuf {
    codex_home(roots).join("auth.json")
}

/// Claude Code's config directory: sandbox, then CLAUDE_CONFIG_DIR, then
/// ~/.claude.
pub fn claude_config_dir(roots: &PathRoots) -> PathBuf {
    if roots.sandbox_home.is_some() {
        return roots.tool_home().join(".claude");
    }
    if let Some(config_dir) = &roots.claude_config_dir {
        return config_dir.clone();
    }
    roots.home.join(".claude")
}

/// The file Claude Code reads its OAuth credential from.
pub fn claude_credentials_path(roots: &PathRoots) -> PathBuf {
    claude_config_dir(roots).join(".credentials.json")
}

/// The Grok CLI's home: sandbox, then GROK_HOME, then ~/.grok.
///
/// The CLI honours GROK_HOME itself, so a user who moved it must be followed;
/// the test sandbox still wins over both, or a test would write into it.
pub fn grok_home(roots: &PathRoots) -> PathBuf {
    if roots.sandbox_home.is_some() {
        return roots.tool_home().join(".grok");
    }
    if let Some(grok_home) = &roots.grok_home {
        return grok_home.clone();
    }
    roots.home.join(".grok")
}

/// The file the Grok CLI reads its OAuth login from.
pub fn grok_auth_path(roots: &PathRoots) -> PathBuf {
    grok_home(roots).join("auth.json")
}

/// An Electron/VS Code-derived app's user-data directory under the platform's
/// app-data root, with the sandbox always winning.
fn ide_user_data_dir(roots: &PathRoots, app_dir: &str) -> PathBuf {
    match roots.os {
        DesktopOs::Macos => roots
            .tool_home()
            .join("Library")
            .join("Application Support")
            .join(app_dir),
        DesktopOs::Windows => match (&roots.sandbox_home, &roots.app_data) {
            (None, Some(app_data)) => app_data.join(app_dir),
            _ => roots
                .tool_home()
                .join("AppData")
                .join("Roaming")
                .join(app_dir),
        },
        DesktopOs::Linux => match (&roots.sandbox_home, &roots.xdg_config_home) {
            (None, Some(xdg)) => xdg.join(app_dir),
            _ => roots.tool_home().join(".config").join(app_dir),
        },
    }
}

/// Devin Desktop's state database.
///
/// The 2026-06 rebrand moved the Electron user-data directory from `Windsurf`
/// to `Devin`; an installation that has not taken the OTA update still keeps
/// the old directory, so an existing database wins over the new name and a
/// fresh machine gets the new name.
pub fn devin_desktop_state_db_path(roots: &PathRoots) -> PathBuf {
    let devin = ide_user_data_dir(roots, "Devin")
        .join("User")
        .join("globalStorage")
        .join("state.vscdb");
    let legacy = ide_user_data_dir(roots, "Windsurf")
        .join("User")
        .join("globalStorage")
        .join("state.vscdb");
    if devin.is_file() {
        return devin;
    }
    if legacy.is_file() {
        return legacy;
    }
    devin
}

/// Kiro's Electron user-data directory: sandbox, then the platform's app-data
/// root (the same layout every VS Code-derived app uses).
pub fn kiro_user_data_dir(roots: &PathRoots) -> PathBuf {
    ide_user_data_dir(roots, "Kiro")
}

/// Kiro's own profile file: the identity its window shows.
pub fn kiro_profile_path(roots: &PathRoots) -> PathBuf {
    kiro_user_data_dir(roots).join("profile.json")
}

/// The editor database Kiro caches its usage panel in.
pub fn kiro_state_db_path(roots: &PathRoots) -> PathBuf {
    kiro_user_data_dir(roots)
        .join("User")
        .join("globalStorage")
        .join("state.vscdb")
}

/// The AWS SSO cache Kiro keeps its login in (`~/.aws/sso/cache` on every OS).
///
/// Cockpit reads `kiro-auth-token.json` from here, not from the app-data
/// directory, so this is the file a switch has to own.
pub fn aws_sso_cache_dir(roots: &PathRoots) -> PathBuf {
    roots.tool_home().join(".aws").join("sso").join("cache")
}

/// The single global Kiro login file.
pub fn kiro_auth_token_path(roots: &PathRoots) -> PathBuf {
    aws_sso_cache_dir(roots).join("kiro-auth-token.json")
}

/// ZCode's data root: sandbox, then the `dataBaseDir` its setting.json names
/// (a replacement home, so the root becomes `{dataBaseDir}/.zcode`), then ~/.zcode.
///
/// The default lives under the home on every platform: ZCode is not an
/// app-data application.
pub fn zcode_home(roots: &PathRoots) -> PathBuf {
    let default = roots.tool_home().join(".zcode");
    if roots.sandbox_home.is_some() {
        return default;
    }
    read_zcode_data_base_dir(&default)
        .map(|dir| dir.join(".zcode"))
        .unwrap_or(default)
}

/// ZCode's credentials file: every value in it is an `enc:v1` blob.
pub fn zcode_credentials_path(roots: &PathRoots) -> PathBuf {
    zcode_home(roots).join("v2").join("credentials.json")
}

/// ZCode's settings file (`v2/setting.json`): the provider family modes live here.
pub fn zcode_settings_path(roots: &PathRoots) -> PathBuf {
    zcode_home(roots).join("v2").join("setting.json")
}

/// ZCode's telemetry state: the official client's `deviceMid` sits here.
pub fn zcode_telemetry_path(roots: &PathRoots) -> PathBuf {
    zcode_home(roots).join("v2").join("telemetry-state.json")
}

/// `dataBaseDir` from a ZCode setting.json, when it is a non-empty string.
fn read_zcode_data_base_dir(home: &std::path::Path) -> Option<PathBuf> {
    let body = std::fs::read_to_string(home.join("v2").join("setting.json")).ok()?;
    let value: serde_json::Value = serde_json::from_str(&body).ok()?;
    let dir = value.get("dataBaseDir")?.as_str()?.trim();
    (!dir.is_empty()).then(|| PathBuf::from(dir))
}

/// Cursor's user-data directory: sandbox, then the platform's app-data root.
pub fn cursor_user_data_dir(roots: &PathRoots) -> PathBuf {
    ide_user_data_dir(roots, "Cursor")
}

/// The SQLite database Cursor keeps its session in.
pub fn cursor_state_db_path(roots: &PathRoots) -> PathBuf {
    cursor_user_data_dir(roots)
        .join("User")
        .join("globalStorage")
        .join("state.vscdb")
}

/// Antigravity's user-data directory: sandbox, then the platform's app-data
/// root. The IDE keeps its editor state there, the same layout VS Code uses.
pub fn antigravity_user_data_dir(roots: &PathRoots) -> PathBuf {
    ide_user_data_dir(roots, "Antigravity IDE")
}

/// The database Antigravity keeps its OAuth session in (the legacy IDE store).
pub fn antigravity_state_db_path(roots: &PathRoots) -> PathBuf {
    antigravity_user_data_dir(roots)
        .join("User")
        .join("globalStorage")
        .join("state.vscdb")
}

/// The Gemini home Antigravity's newer builds also read a token file from:
/// sandbox, then ~/.gemini.
pub fn antigravity_gemini_dir(roots: &PathRoots) -> PathBuf {
    roots.tool_home().join(".gemini")
}

/// Gemini CLI's config directory: sandbox, then ~/.gemini.
///
/// It shares the `~/.gemini` prefix with Antigravity but nothing else: Antigravity
/// writes its own token files there (`jetski-standalone-oauth-token`, the agy CLI
/// session), while the Gemini CLI reads exactly the two files below. Keeping the
/// two products' paths apart is what stops a switch of one from silently
/// rewriting the other's credential.
pub fn gemini_cli_dir(roots: &PathRoots) -> PathBuf {
    roots.tool_home().join(".gemini")
}

/// The OAuth tokens the Gemini CLI serves (the file it refreshes and re-reads).
pub fn gemini_cli_oauth_creds_path(roots: &PathRoots) -> PathBuf {
    gemini_cli_dir(roots).join("oauth_creds.json")
}

/// Which Google account the Gemini CLI believes its cached tokens belong to.
pub fn gemini_cli_accounts_path(roots: &PathRoots) -> PathBuf {
    gemini_cli_dir(roots).join("google_accounts.json")
}

#[cfg(test)]
mod tests {
    use super::{
        DesktopOs, PathRoots, antigravity_gemini_dir, antigravity_state_db_path,
        antigravity_user_data_dir, claude_config_dir, claude_credentials_path, codex_auth_path,
        codex_home, cursor_state_db_path, cursor_user_data_dir, gemini_cli_accounts_path,
        aws_sso_cache_dir, gemini_cli_oauth_creds_path, grok_auth_path, grok_home,
        devin_desktop_state_db_path, kiro_auth_token_path, kiro_profile_path, kiro_state_db_path,
        zcode_credentials_path, zcode_home, zcode_settings_path,
    };
    use std::path::PathBuf;

    fn roots(os: DesktopOs) -> PathRoots {
        PathRoots::for_os(os, PathBuf::from("/home/dev"))
    }

    #[test]
    fn codex_prefers_the_sandbox_then_its_own_override_then_home() {
        let mut plain = roots(DesktopOs::Macos);
        assert_eq!(codex_home(&plain), PathBuf::from("/home/dev/.codex"));
        assert_eq!(
            codex_auth_path(&plain),
            PathBuf::from("/home/dev/.codex/auth.json")
        );

        plain.codex_home = Some(PathBuf::from("/opt/codex"));
        assert_eq!(codex_home(&plain), PathBuf::from("/opt/codex"));
        assert_eq!(
            codex_auth_path(&plain),
            PathBuf::from("/opt/codex/auth.json")
        );

        // The test sandbox wins over CODEX_HOME: a developer who exported
        // CODEX_HOME must still never see a test write into it.
        plain.sandbox_home = Some(PathBuf::from("/sandbox"));
        assert_eq!(codex_home(&plain), PathBuf::from("/sandbox/.codex"));
    }

    #[test]
    fn claude_prefers_the_sandbox_then_its_own_override_then_home() {
        let mut plain = roots(DesktopOs::Linux);
        assert_eq!(
            claude_credentials_path(&plain),
            PathBuf::from("/home/dev/.claude/.credentials.json")
        );

        plain.claude_config_dir = Some(PathBuf::from("/opt/claude"));
        assert_eq!(
            claude_credentials_path(&plain),
            PathBuf::from("/opt/claude/.credentials.json")
        );

        plain.sandbox_home = Some(PathBuf::from("/sandbox"));
        assert_eq!(claude_config_dir(&plain), PathBuf::from("/sandbox/.claude"));
    }

    #[test]
    fn cursor_follows_each_platform_app_data_root() {
        assert_eq!(
            cursor_state_db_path(&roots(DesktopOs::Macos)),
            PathBuf::from("/home/dev/Library/Application Support/Cursor/User/globalStorage/state.vscdb")
        );

        let mut windows = roots(DesktopOs::Windows);
        assert_eq!(
            cursor_user_data_dir(&windows),
            PathBuf::from("/home/dev/AppData/Roaming/Cursor")
        );
        windows.app_data = Some(PathBuf::from("C:/Users/dev/AppData/Roaming"));
        assert_eq!(
            cursor_user_data_dir(&windows),
            PathBuf::from("C:/Users/dev/AppData/Roaming/Cursor")
        );

        let mut linux = roots(DesktopOs::Linux);
        assert_eq!(
            cursor_user_data_dir(&linux),
            PathBuf::from("/home/dev/.config/Cursor")
        );
        linux.xdg_config_home = Some(PathBuf::from("/xdg"));
        assert_eq!(cursor_user_data_dir(&linux), PathBuf::from("/xdg/Cursor"));
        assert_eq!(
            cursor_state_db_path(&linux),
            PathBuf::from("/xdg/Cursor/User/globalStorage/state.vscdb")
        );
    }

    #[test]
    fn antigravity_follows_each_platform_app_data_root() {
        assert_eq!(
            antigravity_state_db_path(&roots(DesktopOs::Macos)),
            PathBuf::from(
                "/home/dev/Library/Application Support/Antigravity IDE/User/globalStorage/state.vscdb"
            )
        );
        assert_eq!(
            antigravity_gemini_dir(&roots(DesktopOs::Macos)),
            PathBuf::from("/home/dev/.gemini")
        );

        let mut windows = roots(DesktopOs::Windows);
        assert_eq!(
            antigravity_user_data_dir(&windows),
            PathBuf::from("/home/dev/AppData/Roaming/Antigravity IDE")
        );
        windows.app_data = Some(PathBuf::from("C:/Users/dev/AppData/Roaming"));
        assert_eq!(
            antigravity_user_data_dir(&windows),
            PathBuf::from("C:/Users/dev/AppData/Roaming/Antigravity IDE")
        );

        let mut linux = roots(DesktopOs::Linux);
        assert_eq!(
            antigravity_user_data_dir(&linux),
            PathBuf::from("/home/dev/.config/Antigravity IDE")
        );
        linux.xdg_config_home = Some(PathBuf::from("/xdg"));
        assert_eq!(
            antigravity_state_db_path(&linux),
            PathBuf::from("/xdg/Antigravity IDE/User/globalStorage/state.vscdb")
        );
        assert_eq!(
            antigravity_gemini_dir(&linux),
            PathBuf::from("/home/dev/.gemini"),
            "the Gemini home is not a config root; it always sits under the home"
        );
    }

    #[test]
    fn devin_desktop_prefers_an_existing_database_and_then_the_new_name() {
        let temp = tempfile::tempdir().expect("temp");
        let mut roots = PathRoots::for_os(DesktopOs::Macos, temp.path().join("home"));
        let devin = temp
            .path()
            .join("home/Library/Application Support/Devin/User/globalStorage/state.vscdb");
        let legacy = temp
            .path()
            .join("home/Library/Application Support/Windsurf/User/globalStorage/state.vscdb");
        // 两个都没有：用新名字（不猜）。
        assert_eq!(devin_desktop_state_db_path(&roots), devin);

        // 只有旧目录（没升 OTA）：用旧的。
        std::fs::create_dir_all(legacy.parent().expect("dir")).expect("dir");
        std::fs::write(&legacy, b"").expect("legacy db");
        assert_eq!(devin_desktop_state_db_path(&roots), legacy);

        // 两个都在：新的优先。
        std::fs::create_dir_all(devin.parent().expect("dir")).expect("dir");
        std::fs::write(&devin, b"").expect("devin db");
        assert_eq!(devin_desktop_state_db_path(&roots), devin);

        // 沙箱压住一切。
        roots.sandbox_home = Some(temp.path().join("sandbox"));
        assert!(
            devin_desktop_state_db_path(&roots).starts_with(temp.path().join("sandbox")),
            "the sandbox must win"
        );
    }

    #[test]
    fn zcode_follows_its_own_setting_json_and_the_sandbox_wins() {
        let temp = tempfile::tempdir().expect("temp");
        let mut roots = PathRoots::for_os(DesktopOs::Macos, temp.path().join("home"));
        assert_eq!(zcode_home(&roots), temp.path().join("home/.zcode"));
        assert_eq!(
            zcode_credentials_path(&roots),
            temp.path().join("home/.zcode/v2/credentials.json")
        );

        // dataBaseDir 是一个"替代 home"，所以根变成 {dataBaseDir}/.zcode。
        let base = temp.path().join("elsewhere");
        let setting = temp.path().join("home/.zcode/v2/setting.json");
        std::fs::create_dir_all(setting.parent().expect("dir")).expect("dir");
        std::fs::write(
            &setting,
            format!(r#"{{"dataBaseDir":"{}"}}"#, base.display()),
        )
        .expect("setting");
        assert_eq!(zcode_home(&roots), base.join(".zcode"));
        assert_eq!(
            zcode_settings_path(&roots),
            base.join(".zcode/v2/setting.json")
        );

        // 沙箱压过 dataBaseDir：测试不许被 JSON 拽出沙箱。
        roots.sandbox_home = Some(temp.path().join("sandbox"));
        assert_eq!(
            zcode_home(&roots),
            temp.path().join("sandbox/.zcode")
        );
    }

    #[test]
    fn kiro_keeps_its_login_in_the_aws_cache_and_its_profile_in_app_data() {
        let macos = roots(DesktopOs::Macos);
        assert_eq!(
            kiro_auth_token_path(&macos),
            PathBuf::from("/home/dev/.aws/sso/cache/kiro-auth-token.json")
        );
        assert_eq!(
            aws_sso_cache_dir(&macos),
            PathBuf::from("/home/dev/.aws/sso/cache")
        );
        assert_eq!(
            kiro_profile_path(&macos),
            PathBuf::from("/home/dev/Library/Application Support/Kiro/profile.json")
        );
        assert_eq!(
            kiro_state_db_path(&macos),
            PathBuf::from(
                "/home/dev/Library/Application Support/Kiro/User/globalStorage/state.vscdb"
            )
        );

        let mut sandboxed = roots(DesktopOs::Windows);
        sandboxed.sandbox_home = Some(PathBuf::from("/sandbox"));
        assert_eq!(
            kiro_auth_token_path(&sandboxed),
            PathBuf::from("/sandbox/.aws/sso/cache/kiro-auth-token.json")
        );
        assert_eq!(
            kiro_profile_path(&sandboxed),
            PathBuf::from("/sandbox/AppData/Roaming/Kiro/profile.json")
        );
    }

    #[test]
    fn grok_prefers_the_sandbox_then_grok_home_then_the_dot_grok_dir() {
        let mut roots = roots(DesktopOs::Macos);
        assert_eq!(grok_home(&roots), PathBuf::from("/home/dev/.grok"));
        assert_eq!(
            grok_auth_path(&roots),
            PathBuf::from("/home/dev/.grok/auth.json")
        );

        roots.grok_home = Some(PathBuf::from("/opt/grok"));
        assert_eq!(grok_auth_path(&roots), PathBuf::from("/opt/grok/auth.json"));

        // 沙箱压过 GROK_HOME：导出了 GROK_HOME 的开发者也不该被测试写进去。
        roots.sandbox_home = Some(PathBuf::from("/sandbox"));
        assert_eq!(grok_auth_path(&roots), PathBuf::from("/sandbox/.grok/auth.json"));
    }

    #[test]
    fn gemini_cli_keeps_its_two_files_under_the_gemini_home() {
        // 同一前缀，不同产品：Gemini CLI 只认下面这两个文件。
        let macos = roots(DesktopOs::Macos);
        assert_eq!(
            gemini_cli_oauth_creds_path(&macos),
            PathBuf::from("/home/dev/.gemini/oauth_creds.json")
        );
        assert_eq!(
            gemini_cli_accounts_path(&macos),
            PathBuf::from("/home/dev/.gemini/google_accounts.json")
        );
        assert_eq!(
            gemini_cli_oauth_creds_path(&macos).parent(),
            Some(antigravity_gemini_dir(&macos).as_path())
        );

        let mut sandboxed = roots(DesktopOs::Linux);
        sandboxed.sandbox_home = Some(PathBuf::from("/sandbox"));
        assert_eq!(
            gemini_cli_oauth_creds_path(&sandboxed),
            PathBuf::from("/sandbox/.gemini/oauth_creds.json")
        );
    }

    #[test]
    fn the_sandbox_root_wins_on_every_platform() {
        for os in [DesktopOs::Macos, DesktopOs::Windows, DesktopOs::Linux] {
            let mut sandboxed = roots(os);
            sandboxed.sandbox_home = Some(PathBuf::from("/sandbox"));
            sandboxed.app_data = Some(PathBuf::from("/real-app-data"));
            sandboxed.xdg_config_home = Some(PathBuf::from("/real-xdg"));
            sandboxed.codex_home = Some(PathBuf::from("/real-codex"));
            sandboxed.claude_config_dir = Some(PathBuf::from("/real-claude"));
            sandboxed.grok_home = Some(PathBuf::from("/real-grok"));
            for path in [
                codex_auth_path(&sandboxed),
                claude_credentials_path(&sandboxed),
                cursor_state_db_path(&sandboxed),
                antigravity_state_db_path(&sandboxed),
                antigravity_gemini_dir(&sandboxed),
                grok_auth_path(&sandboxed),
                gemini_cli_oauth_creds_path(&sandboxed),
                kiro_auth_token_path(&sandboxed),
                kiro_profile_path(&sandboxed),
                zcode_credentials_path(&sandboxed),
            ] {
                assert!(
                    path.starts_with("/sandbox"),
                    "{os:?} escaped the sandbox: {}",
                    path.display()
                );
            }
        }
    }
}
