// ═══════════════════════════════════════════════════════════════════
//  App Shell: tray, window, and lifecycle management
// ═══════════════════════════════════════════════════════════════════

use std::sync::{
    Mutex,
    atomic::{AtomicBool, Ordering},
};

use tauri::{Emitter, Manager};

// ── Exit Control ──────────────────────────────────────────────────

/// Controls whether the app is allowed to exit on next close request.
/// Used by the patrol/background system to allow window close without exit.
pub struct ExitControl {
    allow_exit: AtomicBool,
}

impl Default for ExitControl {
    fn default() -> Self {
        Self::new()
    }
}

impl ExitControl {
    pub fn new() -> Self {
        Self {
            allow_exit: AtomicBool::new(false),
        }
    }

    pub fn allow_next_exit(&self) {
        self.allow_exit.store(true, Ordering::SeqCst);
    }

    pub fn consume_allow_flag(&self) -> bool {
        self.allow_exit.swap(false, Ordering::SeqCst)
    }
}

// ── Tray State ────────────────────────────────────────────────────

/// Holds the current UI language for the tray menu.
pub struct TrayState {
    lang: Mutex<String>,
}

impl TrayState {
    pub fn new(lang: impl Into<String>) -> Self {
        Self {
            lang: Mutex::new(lang.into()),
        }
    }

    pub fn lang(&self) -> String {
        self.lang
            .lock()
            .map(|lang| lang.clone())
            .unwrap_or_else(|_| detect_system_lang().to_string())
    }

    pub fn set_lang(&self, lang: String) {
        if let Ok(mut current_lang) = self.lang.lock() {
            *current_lang = lang;
        }
    }
}

// ── System Language Detection ─────────────────────────────────────

/// Detect system language at startup — "zh" prefix → Chinese, else English.
pub fn detect_system_lang() -> &'static str {
    let locale = sys_locale::get_locale().unwrap_or_default();
    if locale.starts_with("zh") {
        "zh-CN"
    } else {
        "en"
    }
}

// ── Tray Labels ───────────────────────────────────────────────────

/// Returns (show_label, toggle_patrol_label, quit_label) for the given language.
pub fn tray_labels(lang: &str, patrol_enabled: bool) -> (&'static str, &'static str, &'static str) {
    if lang.starts_with("zh") {
        (
            "显示窗口",
            if patrol_enabled {
                "停止后台检查"
            } else {
                "启动后台检查"
            },
            "退出",
        )
    } else {
        (
            "Show Window",
            if patrol_enabled {
                "Stop Background Check"
            } else {
                "Start Background Check"
            },
            "Quit",
        )
    }
}

// ── Tray Menu Building ────────────────────────────────────────────

fn build_tray_menu(
    app: &impl Manager<tauri::Wry>,
    lang: &str,
    patrol_enabled: bool,
) -> Result<tauri::menu::Menu<tauri::Wry>, Box<dyn std::error::Error>> {
    use tauri::menu::{CheckMenuItem, MenuBuilder, MenuItem, SubmenuBuilder};

    let (show_label, toggle_label, quit_label) = tray_labels(lang, patrol_enabled);
    let empty_label = if lang.starts_with("zh") {
        "空"
    } else {
        "Empty"
    };

    let show_i = MenuItem::with_id(app, "show", show_label, true, None::<&str>)?;
    let toggle_i = MenuItem::with_id(app, "toggle_patrol", toggle_label, true, None::<&str>)?;
    let quit_i = MenuItem::with_id(app, "quit", quit_label, true, None::<&str>)?;

    let store = crate::core::model_config::providers::read_store().unwrap_or_default();

    let mut menu_builder = MenuBuilder::new(app);

    for (app_id, display_name, app_providers) in [
        ("codex", "Codex", &store.codex),
        ("claude", "Claude", &store.claude),
    ] {
        let mut sub_builder = SubmenuBuilder::new(app, display_name);

        let mut sorted_providers: Vec<_> = app_providers.providers.values().collect();
        sorted_providers.sort_by_key(|p| p.sort_index.unwrap_or(u32::MAX));

        let mut has_items = false;

        if !sorted_providers.is_empty() {
            has_items = true;
            for provider in sorted_providers {
                let is_checked = app_providers.current.as_deref() == Some(provider.id.as_str());
                let id = format!("provider_{}_{}", app_id, provider.id);
                let check_item = CheckMenuItem::with_id(
                    app,
                    &id,
                    &provider.name,
                    true,
                    is_checked,
                    None::<&str>,
                )?;
                let _ = check_item.set_checked(is_checked);
                sub_builder = sub_builder.item(&check_item);
            }
        }

        if app_id == "codex" {
            let accounts = crate::core::model_config::codex_accounts::list_accounts()
                .into_iter()
                .filter(|a| a.auth_mode == "oauth" || a.auth_mode == "apikey")
                .collect::<Vec<_>>();

            if !accounts.is_empty() {
                if has_items {
                    sub_builder = sub_builder.separator();
                }
                has_items = true;
                let current_account_id =
                    crate::core::model_config::codex_accounts::get_current_account_id();
                for account in accounts {
                    let is_checked = current_account_id.as_deref() == Some(account.id.as_str());
                    let display_text = if account.email.contains('@') {
                        account
                            .email
                            .split('@')
                            .next()
                            .unwrap_or(&account.email)
                            .to_string()
                    } else {
                        account.email.clone()
                    };

                    let id = format!("account_{}_{}", app_id, account.id);
                    let check_item = CheckMenuItem::with_id(
                        app,
                        &id,
                        &display_text,
                        true,
                        is_checked,
                        None::<&str>,
                    )?;
                    let _ = check_item.set_checked(is_checked);
                    sub_builder = sub_builder.item(&check_item);
                }
            }
        }

        if !has_items {
            let empty_item = MenuItem::with_id(
                app,
                format!("empty_{}", app_id),
                empty_label,
                false,
                None::<&str>,
            )?;
            sub_builder = sub_builder.item(&empty_item);
        }
        let submenu = sub_builder.build()?;
        menu_builder = menu_builder.item(&submenu);
    }

    let menu = menu_builder
        .separator()
        .item(&show_i)
        .separator()
        .item(&toggle_i)
        .separator()
        .item(&quit_i)
        .build()?;

    Ok(menu)
}

// ── Tray Setup ────────────────────────────────────────────────────

pub fn setup_tray(app: &tauri::App) -> Result<(), Box<dyn std::error::Error>> {
    use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};

    let lang = app.state::<TrayState>().lang();
    let patrol_enabled = app
        .state::<crate::core::patrol::PatrolManager>()
        .status()
        .enabled;
    let menu = build_tray_menu(app, &lang, patrol_enabled)?;

    TrayIconBuilder::with_id("main-tray")
        .tooltip("SkillStar")
        .icon(
            app.default_window_icon()
                .expect("SkillStar must have a default window icon")
                .clone(),
        )
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "show" => {
                // Restore Dock icon before showing.
                #[cfg(target_os = "macos")]
                {
                    let _ = app.set_activation_policy(tauri::ActivationPolicy::Regular);
                }
                if let Some(window) = app.get_webview_window("main") {
                    let _ = window.show();
                    let _ = window.unminimize();
                    let _ = window.set_focus();
                }
            }
            "toggle_patrol" => {
                let manager = app.state::<crate::core::patrol::PatrolManager>();
                let status = manager.status();

                if status.enabled {
                    manager.stop();
                } else {
                    let _ = manager.start(app.clone(), status.interval_secs);
                }

                let _ = app.emit("patrol://enabled-changed", !status.enabled);
                let _ = refresh_tray_menu(app);
            }
            "quit" => {
                let manager = app.state::<crate::core::patrol::PatrolManager>();
                manager.stop();
                let exit_control = app.state::<ExitControl>();
                exit_control.allow_next_exit();
                app.exit(0);
            }
            id if id.starts_with("provider_") => {
                let parts: Vec<&str> = id.splitn(3, '_').collect();
                if parts.len() == 3 {
                    let app_id = parts[1];
                    let provider_id = parts[2];
                    if let Err(e) =
                        crate::core::model_config::providers::switch_provider(app_id, provider_id)
                    {
                        tracing::error!("Failed to switch provider from tray: {}", e);
                    } else {
                        let _ = app.emit(
                            "model-config://switched",
                            serde_json::json!({
                                "appId": app_id,
                                "providerId": provider_id
                            }),
                        );
                        if let Err(e) = refresh_tray_menu(app) {
                            tracing::error!("Failed to refresh tray menu: {}", e);
                        }
                    }
                }
            }
            id if id.starts_with("account_") => {
                let parts: Vec<&str> = id.splitn(3, '_').collect();
                if parts.len() == 3 {
                    let account_id = parts[2].to_string();
                    let app_handle = app.clone();
                    tauri::async_runtime::spawn(async move {
                        match crate::commands::oauth_commands::switch_codex_account(
                            app_handle.clone(),
                            account_id.clone(),
                        )
                        .await
                        {
                            Ok(_) => {
                                let _ = app_handle.emit(
                                    "codex-account://switched",
                                    serde_json::json!({
                                        "accountId": account_id
                                    }),
                                );
                            }
                            Err(e) => {
                                tracing::error!("Failed to switch codex account from tray: {}", e);
                            }
                        }
                    });
                }
            }
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                let app = tray.app_handle();
                // Restore Dock icon before showing.
                #[cfg(target_os = "macos")]
                {
                    let _ = app.set_activation_policy(tauri::ActivationPolicy::Regular);
                }
                if let Some(window) = app.get_webview_window("main") {
                    let _ = window.show();
                    let _ = window.unminimize();
                    let _ = window.set_focus();
                }
            }
        })
        .build(app)?;

    Ok(())
}

// ── Tray Menu Refresh ──────────────────────────────────────────────

pub fn refresh_tray_menu(app: &tauri::AppHandle) -> Result<(), String> {
    let lang = app.state::<TrayState>().lang();
    let patrol_enabled = app
        .state::<crate::core::patrol::PatrolManager>()
        .status()
        .enabled;

    let menu = build_tray_menu(app, &lang, patrol_enabled).map_err(|e| e.to_string())?;
    let tray = app
        .tray_by_id("main-tray")
        .ok_or_else(|| "tray not found".to_string())?;
    tray.set_menu(Some(menu)).map_err(|e| e.to_string())?;
    Ok(())
}
