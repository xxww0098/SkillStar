//! Supported desktop apps and the argv that actually isolates them.
//!
//! Cursor, Grok Bot, and Antigravity are verified. Other ids in
//! [`DesktopAppId`] are Pending: the launch shape is registered, but they are
//! not listed or started. Zed and GitHub Copilot are not variants.

use std::path::{Path, PathBuf};

use super::error::{
    CLAUDE_DESKTOP_REASON, GITHUB_COPILOT_REASON, InstanceError, ZED_INSTANCE_REASON,
};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// Desktop apps with a registered instance launch shape.
///
/// Claude Desktop and Zed are intentionally absent. Claude ignores
/// `--user-data-dir`. Zed has no such flag and keeps login in the global
/// keychain. GitHub Copilot is not an app.
///
/// Only `cursor`, `grok-bot`, and `antigravity` are verified and listed.
/// Every other variant is Pending. No live isolation run marked one Verified.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "kebab-case")]
#[ts(export, export_to = "DesktopAppId.ts")]
pub enum DesktopAppId {
    Cursor,
    GrokBot,
    Antigravity,
    #[serde(rename = "windsurf")]
    Windsurf,
    #[serde(rename = "kiro")]
    Kiro,
    #[serde(rename = "qoder")]
    Qoder,
    #[serde(rename = "codebuddy")]
    CodeBuddy,
    #[serde(rename = "codebuddy-cn")]
    CodeBuddyCn,
    #[serde(rename = "zcode")]
    ZCode,
    #[serde(rename = "trae")]
    Trae,
    #[serde(rename = "trae-solo")]
    TraeSolo,
    #[serde(rename = "trae-cn")]
    TraeCn,
    #[serde(rename = "trae-solo-cn")]
    TraeSoloCn,
}

/// Whether a registered app may appear in the picker and be launched.
///
/// Pending is the default for a candidate that has no isolation report.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InstanceCapability {
    Verified,
    Pending,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UserDataDirForm {
    /// `--user-data-dir <dir>` (two argv words).
    Separate,
    /// `--user-data-dir=<dir>` (one argv word). Antigravity drops the space form.
    Equals,
}

/// How a managed instance is isolated from the default profile.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LaunchMode {
    OpenArgs {
        user_data_dir_form: UserDataDirForm,
        extra_fixed_args: &'static [&'static str],
    },
    /// ZCode. `open -a` cannot pass these variables, and the main process
    /// does not take `--user-data-dir`. Helper processes still show
    /// `--user-data-dir={root}/electron`.
    EnvSpawn(EnvSpawn),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EnvSpawn {
    /// Joined onto the instance root. PID matching uses this directory.
    pub electron_subdir: &'static str,
    pub bindings: &'static [EnvBinding],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EnvBinding {
    pub key: &'static str,
    pub value: EnvTemplate,
    /// Cockpit sets `HOME` / `USERPROFILE` only off Windows. ZCode 3.3.4
    /// fails Electron init on Windows when those two are redirected.
    pub unix_only: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EnvTemplate {
    /// `{instance_root}/{suffix}`.
    UnderRoot(&'static str),
    /// `ZCode [{instance name}]`.
    ZCodeWindowTitle,
    /// Real-home credential secret. This registry does not compute it.
    RealHomeCredential,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EnvAssignment {
    pub key: &'static str,
    pub value: EnvAssignmentValue,
    pub unix_only: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EnvAssignmentValue {
    Path(String),
    Literal(String),
    RealHomeCredential,
}

#[derive(Debug, Clone, Copy)]
pub struct LaunchSpec {
    pub macos_app_name: &'static str,
    pub mode: LaunchMode,
}

const ZCODE_BINDINGS: &[EnvBinding] = &[
    EnvBinding {
        key: "ZCODE_DESKTOP_USER_DATA_DIR",
        value: EnvTemplate::UnderRoot("electron"),
        unix_only: false,
    },
    EnvBinding {
        key: "ZCODE_DESKTOP_SESSION_DATA_DIR",
        value: EnvTemplate::UnderRoot("electron/session"),
        unix_only: false,
    },
    EnvBinding {
        key: "ZCODE_DATA_BASE_DIR",
        value: EnvTemplate::UnderRoot("data"),
        unix_only: false,
    },
    EnvBinding {
        key: "ZCODE_DESKTOP_HOME_DIR",
        value: EnvTemplate::UnderRoot("data"),
        unix_only: false,
    },
    EnvBinding {
        key: "ZCODE_CREDENTIAL_SECRET",
        value: EnvTemplate::RealHomeCredential,
        unix_only: false,
    },
    EnvBinding {
        key: "ZCODE_DESKTOP_APPLICATION_NAME",
        value: EnvTemplate::ZCodeWindowTitle,
        unix_only: false,
    },
    EnvBinding {
        key: "HOME",
        value: EnvTemplate::UnderRoot("data"),
        unix_only: true,
    },
    EnvBinding {
        key: "USERPROFILE",
        value: EnvTemplate::UnderRoot("data"),
        unix_only: true,
    },
];

const ZCODE_ENV_SPAWN: EnvSpawn = EnvSpawn {
    electron_subdir: "electron",
    bindings: ZCODE_BINDINGS,
};

#[derive(Clone, Copy)]
struct AppMeta {
    as_str: &'static str,
    display_name: &'static str,
    catalog_id: Option<&'static str>,
    capability: InstanceCapability,
    macos_app_name: &'static str,
    mode: LaunchMode,
}

fn open_args(form: UserDataDirForm, extra_fixed_args: &'static [&'static str]) -> LaunchMode {
    LaunchMode::OpenArgs {
        user_data_dir_form: form,
        extra_fixed_args,
    }
}

fn pending(
    id: &'static str,
    display_name: &'static str,
    macos_app_name: &'static str,
    mode: LaunchMode,
) -> AppMeta {
    AppMeta {
        as_str: id,
        display_name,
        catalog_id: Some(id),
        capability: InstanceCapability::Pending,
        macos_app_name,
        mode,
    }
}

impl DesktopAppId {
    /// Registered ids. Pending candidates are included; Zed is not.
    pub fn candidates() -> [Self; 13] {
        [
            Self::Cursor,
            Self::GrokBot,
            Self::Antigravity,
            Self::Windsurf,
            Self::Kiro,
            Self::Qoder,
            Self::CodeBuddy,
            Self::CodeBuddyCn,
            Self::ZCode,
            Self::Trae,
            Self::TraeSolo,
            Self::TraeCn,
            Self::TraeSoloCn,
        ]
    }

    fn meta(self) -> AppMeta {
        match self {
            Self::Cursor => AppMeta {
                as_str: "cursor",
                display_name: "Cursor",
                catalog_id: Some("cursor"),
                capability: InstanceCapability::Verified,
                macos_app_name: "Cursor.app",
                mode: open_args(UserDataDirForm::Separate, &["--new-window"]),
            },
            Self::GrokBot => AppMeta {
                as_str: "grok-bot",
                display_name: "Grok Bot",
                catalog_id: None,
                capability: InstanceCapability::Verified,
                macos_app_name: "Grok Bot.app",
                mode: open_args(UserDataDirForm::Separate, &[]),
            },
            Self::Antigravity => AppMeta {
                as_str: "antigravity",
                display_name: "Antigravity",
                catalog_id: Some("antigravity"),
                capability: InstanceCapability::Verified,
                macos_app_name: "Antigravity.app",
                mode: open_args(UserDataDirForm::Equals, &["--new-window"]),
            },
            // Cockpit probes `Devin.app` before `Windsurf.app`. The bundle
            // name is not verified; isolation stays Pending either way.
            Self::Windsurf => pending(
                "windsurf",
                "Windsurf",
                "Windsurf.app",
                open_args(UserDataDirForm::Separate, &["--new-window"]),
            ),
            Self::Kiro => pending(
                "kiro",
                "Kiro",
                "Kiro.app",
                open_args(UserDataDirForm::Separate, &["--new-window"]),
            ),
            Self::Qoder => pending(
                "qoder",
                "Qoder",
                "Qoder.app",
                open_args(UserDataDirForm::Separate, &["--new-window"]),
            ),
            Self::CodeBuddy => pending(
                "codebuddy",
                "CodeBuddy",
                "CodeBuddy.app",
                open_args(UserDataDirForm::Separate, &["--new-window"]),
            ),
            Self::CodeBuddyCn => pending(
                "codebuddy-cn",
                "CodeBuddy CN",
                "CodeBuddy CN.app",
                open_args(UserDataDirForm::Separate, &["--new-window"]),
            ),
            Self::ZCode => pending(
                "zcode",
                "ZCode",
                "ZCode.app",
                LaunchMode::EnvSpawn(ZCODE_ENV_SPAWN),
            ),
            Self::Trae => pending(
                "trae",
                "Trae",
                "Trae.app",
                open_args(UserDataDirForm::Separate, &["--new-window"]),
            ),
            Self::TraeSolo => pending(
                "trae-solo",
                "TRAE SOLO",
                "TRAE SOLO.app",
                open_args(UserDataDirForm::Separate, &["--new-window"]),
            ),
            Self::TraeCn => pending(
                "trae-cn",
                "Trae CN",
                "Trae CN.app",
                open_args(UserDataDirForm::Separate, &["--new-window"]),
            ),
            Self::TraeSoloCn => pending(
                "trae-solo-cn",
                "TRAE SOLO CN",
                "TRAE SOLO CN.app",
                open_args(UserDataDirForm::Separate, &["--new-window"]),
            ),
        }
    }

    pub fn as_str(self) -> &'static str {
        self.meta().as_str
    }

    pub fn display_name(self) -> &'static str {
        self.meta().display_name
    }

    /// Usage catalog this app's instances attach to in the UI, if any.
    ///
    /// Grok Bot has no catalog: do not bind it to `xai`.
    pub fn catalog_id(self) -> Option<&'static str> {
        self.meta().catalog_id
    }

    pub fn capability(self) -> InstanceCapability {
        self.meta().capability
    }

    pub fn launch_spec(self) -> LaunchSpec {
        let meta = self.meta();
        LaunchSpec {
            macos_app_name: meta.macos_app_name,
            mode: meta.mode,
        }
    }

    /// Pending apps parse, but create and start refuse them.
    pub fn ensure_launchable(self) -> Result<Self, InstanceError> {
        match self.capability() {
            InstanceCapability::Verified => Ok(self),
            InstanceCapability::Pending => Err(InstanceError::UnsupportedApp(format!(
                "{} 多开仍是 Pending：尚未做隔离实证，不能创建或启动实例。",
                self.display_name()
            ))),
        }
    }

    /// Directory a running process must mention for this instance.
    ///
    /// Chromium apps use the instance root. ZCode helpers use `{root}/electron`.
    pub fn process_match_dir(self, instance_root: &Path) -> PathBuf {
        match self.launch_spec().mode {
            LaunchMode::OpenArgs { .. } => instance_root.to_path_buf(),
            LaunchMode::EnvSpawn(spec) => instance_root.join(spec.electron_subdir),
        }
    }

    /// Materialized env for [`LaunchMode::EnvSpawn`]. `None` for `open -a` apps.
    ///
    /// `RealHomeCredential` is not filled in. This does not read the home directory.
    pub fn env_assignments(
        self,
        instance_root: &Path,
        instance_name: &str,
    ) -> Option<Vec<EnvAssignment>> {
        let LaunchMode::EnvSpawn(spec) = self.launch_spec().mode else {
            return None;
        };
        Some(
            spec.bindings
                .iter()
                .map(|binding| EnvAssignment {
                    key: binding.key,
                    unix_only: binding.unix_only,
                    value: match binding.value {
                        EnvTemplate::UnderRoot(suffix) => EnvAssignmentValue::Path(
                            instance_root.join(suffix).to_string_lossy().into_owned(),
                        ),
                        EnvTemplate::ZCodeWindowTitle => {
                            EnvAssignmentValue::Literal(format!("ZCode [{instance_name}]"))
                        }
                        EnvTemplate::RealHomeCredential => EnvAssignmentValue::RealHomeCredential,
                    },
                })
                .collect(),
        )
    }

    pub fn parse(raw: &str) -> Result<Self, InstanceError> {
        let raw = raw.trim();
        if let Some(app) = Self::candidates()
            .into_iter()
            .find(|app| app.as_str() == raw)
        {
            return Ok(app);
        }
        match raw {
            "claude" | "claude-desktop" | "Claude" | "Claude.app" => Err(
                InstanceError::UnsupportedApp(CLAUDE_DESKTOP_REASON.to_string()),
            ),
            "zed" | "Zed" | "Zed.app" => Err(InstanceError::UnsupportedApp(
                ZED_INSTANCE_REASON.to_string(),
            )),
            "github-copilot" => Err(InstanceError::UnsupportedApp(
                GITHUB_COPILOT_REASON.to_string(),
            )),
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
///
/// `None` for [`LaunchMode::EnvSpawn`]: that launch has no `--user-data-dir` flag.
pub fn open_argv(app: DesktopAppId, user_data_dir: &Path) -> Option<Vec<String>> {
    let spec = app.launch_spec();
    let LaunchMode::OpenArgs {
        user_data_dir_form,
        extra_fixed_args,
    } = spec.mode
    else {
        return None;
    };
    let dir = user_data_dir.to_string_lossy();
    let mut argv = vec![
        "/usr/bin/open".to_string(),
        "-n".to_string(),
        "-a".to_string(),
        spec.macos_app_name.to_string(),
        "--args".to_string(),
    ];
    match user_data_dir_form {
        UserDataDirForm::Equals => argv.push(format!("--user-data-dir={dir}")),
        UserDataDirForm::Separate => {
            argv.push("--user-data-dir".to_string());
            argv.push(dir.into_owned());
        }
    }
    argv.extend(extra_fixed_args.iter().map(|arg| (*arg).to_string()));
    Some(argv)
}

/// Apps shown in the instance picker. Pending candidates are omitted.
pub fn all_apps() -> Vec<DesktopAppId> {
    DesktopAppId::candidates()
        .into_iter()
        .filter(|app| app.capability() == InstanceCapability::Verified)
        .collect()
}
