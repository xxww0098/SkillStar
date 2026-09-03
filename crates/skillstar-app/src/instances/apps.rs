//! Supported desktop apps and the argv that actually isolates them.

use super::error::{CLAUDE_DESKTOP_REASON, InstanceError};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// Desktop apps SkillStar will launch with an isolated Chromium profile.
///
/// Claude Desktop is intentionally absent: it ignores `--user-data-dir`.
/// Catalog `xai` is also absent: that is the Grok CLI, not Grok Bot.app.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "kebab-case")]
#[ts(export, export_to = "DesktopAppId.ts")]
pub enum DesktopAppId {
    Cursor,
    GrokBot,
    Antigravity,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UserDataDirForm {
    /// `--user-data-dir <dir>` (Cursor, Grok Bot).
    Separate,
    /// `--user-data-dir=<dir>` (Antigravity; space-separated form is dropped).
    Equals,
}

#[derive(Debug, Clone, Copy)]
pub struct LaunchSpec {
    pub macos_app_name: &'static str,
    pub user_data_dir_form: UserDataDirForm,
    pub extra_fixed_args: &'static [&'static str],
}

impl DesktopAppId {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Cursor => "cursor",
            Self::GrokBot => "grok-bot",
            Self::Antigravity => "antigravity",
        }
    }

    pub fn display_name(self) -> &'static str {
        match self {
            Self::Cursor => "Cursor",
            Self::GrokBot => "Grok Bot",
            Self::Antigravity => "Antigravity",
        }
    }

    /// Usage catalog this app's instances attach to in the UI, if any.
    ///
    /// Grok Bot has no catalog: do not bind it to `xai`.
    pub fn catalog_id(self) -> Option<&'static str> {
        match self {
            Self::Cursor => Some("cursor"),
            Self::GrokBot => None,
            Self::Antigravity => Some("antigravity"),
        }
    }

    pub fn launch_spec(self) -> LaunchSpec {
        match self {
            Self::Cursor => LaunchSpec {
                macos_app_name: "Cursor.app",
                user_data_dir_form: UserDataDirForm::Separate,
                extra_fixed_args: &["--new-window"],
            },
            Self::GrokBot => LaunchSpec {
                macos_app_name: "Grok Bot.app",
                user_data_dir_form: UserDataDirForm::Separate,
                extra_fixed_args: &[],
            },
            Self::Antigravity => LaunchSpec {
                macos_app_name: "Antigravity.app",
                user_data_dir_form: UserDataDirForm::Equals,
                extra_fixed_args: &["--new-window"],
            },
        }
    }

    pub fn parse(raw: &str) -> Result<Self, InstanceError> {
        match raw.trim() {
            "cursor" => Ok(Self::Cursor),
            "grok-bot" => Ok(Self::GrokBot),
            "antigravity" => Ok(Self::Antigravity),
            "claude" | "claude-desktop" | "Claude" | "Claude.app" => Err(
                InstanceError::UnsupportedApp(CLAUDE_DESKTOP_REASON.to_string()),
            ),
            "anthropic" => Err(InstanceError::UnsupportedApp(
                "不能把 Claude 桌面多开绑到 anthropic 额度卡。Claude Desktop 不支持 profile 隔离。"
                    .to_string(),
            )),
            "xai" | "grok" => Err(InstanceError::UnsupportedApp(
                "不能把 Grok Bot 桌面多开绑到 xai CLI。请使用 grok-bot。".to_string(),
            )),
            other => Err(InstanceError::UnsupportedApp(format!(
                "未知的桌面应用：{other}"
            ))),
        }
    }
}

/// `/usr/bin/open -n -a <App.app> --args …` plus the app-specific user-data-dir form.
pub fn open_argv(app: DesktopAppId, user_data_dir: &std::path::Path) -> Vec<String> {
    let spec = app.launch_spec();
    let dir = user_data_dir.to_string_lossy();
    let mut argv = vec![
        "/usr/bin/open".to_string(),
        "-n".to_string(),
        "-a".to_string(),
        spec.macos_app_name.to_string(),
        "--args".to_string(),
    ];
    match spec.user_data_dir_form {
        UserDataDirForm::Equals => argv.push(format!("--user-data-dir={dir}")),
        UserDataDirForm::Separate => {
            argv.push("--user-data-dir".to_string());
            argv.push(dir.into_owned());
        }
    }
    argv.extend(spec.extra_fixed_args.iter().map(|s| (*s).to_string()));
    argv
}

pub fn all_apps() -> [DesktopAppId; 3] {
    [
        DesktopAppId::Cursor,
        DesktopAppId::GrokBot,
        DesktopAppId::Antigravity,
    ]
}
