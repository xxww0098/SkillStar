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
    use tauri::menu::{MenuBuilder, MenuItem};

    let (show_label, toggle_label, quit_label) = tray_labels(lang, patrol_enabled);

    let show_i = MenuItem::with_id(app, "show", show_label, true, None::<&str>)?;
    let toggle_i = MenuItem::with_id(app, "toggle_patrol", toggle_label, true, None::<&str>)?;
    let quit_i = MenuItem::with_id(app, "quit", quit_label, true, None::<&str>)?;

    let menu = MenuBuilder::new(app)
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
