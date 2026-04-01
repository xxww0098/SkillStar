mod cli;
mod commands;
mod core;

use std::sync::{
    Mutex,
    atomic::{AtomicBool, Ordering},
};
use tauri::{Emitter, Manager};

pub struct ExitControl {
    allow_exit: AtomicBool,
}

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

pub fn run_cli(args: Vec<String>) {
    cli::run(args);
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let mut builder = tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_dialog::init());

    #[cfg(desktop)]
    {
        builder = builder
            .plugin(tauri_plugin_updater::Builder::new().build())
            .plugin(tauri_plugin_process::init());
    }

    builder
        .manage(core::patrol::PatrolManager::new())
        .manage(TrayState::new(detect_system_lang()))
        .manage(ExitControl::new())
        .setup(|app| {
            if let Err(err) = core::marketplace_snapshot::initialize() {
                eprintln!("[marketplace_snapshot] init failed: {err}");
            }
            let app_handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                if let Err(err) =
                    core::marketplace_snapshot::refresh_startup_scopes_if_needed().await
                {
                    eprintln!("[marketplace_snapshot] startup refresh failed: {err}");
                }
                let _ = app_handle.emit("marketplace://ready", ());
            });
            setup_tray(app)?;
            Ok(())
        })
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                let app = window.app_handle();
                let patrol_manager = app.state::<core::patrol::PatrolManager>();
                let should_background = patrol_manager.status().enabled;

                if should_background {
                    // Prevent native close and hide the window to background.
                    api.prevent_close();
                    let _ = window.hide();
                    // Hide the Dock icon — keep only the tray icon visible.
                    #[cfg(target_os = "macos")]
                    {
                        let _ = app.set_activation_policy(tauri::ActivationPolicy::Accessory);
                    }
                    // Notify frontend so it can optionally start patrol.
                    let _ = window.emit("skillstar://window-hidden", ());
                } else {
                    api.prevent_close();
                    let exit_control = app.state::<ExitControl>();
                    exit_control.allow_next_exit();
                    app.exit(0);
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            commands::list_skills,
            commands::refresh_skill_updates,
            commands::install_skill,
            commands::uninstall_skill,
            commands::toggle_skill_for_agent,
            commands::update_skill,
            commands::marketplace::search_skills_sh,
            commands::marketplace::get_skills_sh_leaderboard,
            commands::marketplace::get_official_publishers,
            commands::marketplace::get_publisher_repos,
            commands::marketplace::get_publisher_repo_skills,
            commands::marketplace::get_marketplace_skill_details,
            commands::marketplace::resolve_skill_sources,
            commands::marketplace::ai_extract_search_keywords,
            commands::marketplace::ai_search_with_keywords,
            commands::marketplace::get_leaderboard_local,
            commands::marketplace::search_marketplace_local,
            commands::marketplace::get_publishers_local,
            commands::marketplace::get_publisher_repos_local,
            commands::marketplace::get_repo_skills_local,
            commands::marketplace::get_skill_detail_local,
            commands::marketplace::ai_search_marketplace_local,
            commands::marketplace::sync_marketplace_scope,
            commands::marketplace::get_marketplace_sync_states,
            commands::github::check_gh_installed,
            commands::github::check_gh_status,
            commands::github::publish_skill_to_github,
            commands::github::list_user_repos,
            commands::github::inspect_repo_folders,
            commands::read_skill_file_raw,
            commands::create_local_skill_from_content,
            commands::create_local_skill,
            commands::delete_local_skill,
            commands::migrate_local_skills,
            commands::list_skill_files,
            commands::agents::list_agent_profiles,
            commands::agents::toggle_agent_profile,
            commands::agents::unlink_all_skills_from_agent,
            commands::agents::batch_link_skills_to_agent,
            commands::agents::list_linked_skills,
            commands::agents::unlink_skill_from_agent,
            commands::projects::create_project_skills,
            commands::list_skill_groups,
            commands::create_skill_group,
            commands::update_skill_group,
            commands::delete_skill_group,
            commands::duplicate_skill_group,
            commands::deploy_skill_group,
            commands::read_skill_content,
            commands::update_skill_content,
            commands::get_proxy_config,
            commands::save_proxy_config,
            commands::projects::register_project,
            commands::projects::list_projects,
            commands::projects::get_project_skills,
            commands::projects::save_and_sync_project,
            commands::projects::save_project_skills_list,
            commands::projects::update_project_path,
            commands::projects::remove_project,
            commands::projects::scan_project_skills,
            commands::projects::rebuild_project_skills_from_disk,
            commands::projects::import_project_skills,
            commands::projects::detect_project_agents,
            commands::ai::get_ai_config,
            commands::ai::save_ai_config,
            commands::ai::ai_translate_skill,
            commands::ai::ai_translate_skill_stream,
            commands::ai::ai_translate_short_text,
            commands::ai::ai_translate_short_text_with_source,
            commands::ai::get_mymemory_usage_stats,
            commands::ai::ai_translate_short_text_stream,
            commands::ai::ai_translate_short_text_stream_with_source,
            commands::ai::ai_retranslate_short_text_with_source,
            commands::ai::ai_retranslate_short_text_stream_with_source,
            commands::ai::ai_summarize_skill,
            commands::ai::ai_summarize_skill_stream,
            commands::ai::ai_test_connection,
            commands::ai::ai_pick_skills,
            commands::ai::ai_security_scan,
            commands::ai::estimate_security_scan,
            commands::ai::cancel_security_scan,
            commands::ai::get_cached_scan_results,
            commands::ai::clear_security_scan_cache,
            commands::ai::list_security_scan_logs,
            commands::ai::get_security_scan_log_dir,
            commands::github::scan_github_repo,
            commands::github::install_from_scan,
            commands::github::list_repo_history,
            commands::github::get_repo_cache_info,
            commands::github::clean_repo_cache,
            commands::github::get_storage_overview,
            commands::github::clear_all_caches,
            commands::github::force_delete_installed_skills,
            commands::github::force_delete_repo_caches,
            commands::github::force_delete_app_config,
            commands::github::clean_broken_skills,
            commands::export_skill_bundle,
            commands::preview_skill_bundle,
            commands::import_skill_bundle,
            commands::export_multi_skill_bundle,
            commands::write_text_file,
            commands::read_text_file,
            commands::open_folder,
            commands::patrol::start_patrol,
            commands::patrol::stop_patrol,
            commands::patrol::get_patrol_status,
            commands::patrol::set_patrol_enabled,
            commands::patrol::app_quit,
            commands::patrol::set_dock_visible,
            update_tray_language,
        ])
        .build(tauri::generate_context!())
        .expect("error while building SkillStar")
        .run(|app_handle, event| {
            // Prevent the app from exiting when the last window is hidden.
            // This keeps the process alive for background patrol and tray icon.
            if let tauri::RunEvent::ExitRequested { api, .. } = &event {
                let exit_control = app_handle.state::<ExitControl>();
                if !exit_control.consume_allow_flag() {
                    api.prevent_exit();
                }
            }
        });
}

// ── System Tray ─────────────────────────────────────────────────────

/// Returns (show_label, toggle_patrol_label, quit_label) for the given language.
fn tray_labels(lang: &str, patrol_enabled: bool) -> (&'static str, &'static str, &'static str) {
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

/// Detect system language at startup — "zh" prefix → Chinese, else English.
fn detect_system_lang() -> &'static str {
    let locale = sys_locale::get_locale().unwrap_or_default();
    if locale.starts_with("zh") {
        "zh-CN"
    } else {
        "en"
    }
}

fn setup_tray(app: &tauri::App) -> Result<(), Box<dyn std::error::Error>> {
    use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};

    let lang = app.state::<TrayState>().lang();
    let patrol_enabled = app.state::<core::patrol::PatrolManager>().status().enabled;
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
                let manager = app.state::<core::patrol::PatrolManager>();
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
                let manager = app.state::<core::patrol::PatrolManager>();
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

pub(crate) fn refresh_tray_menu(app: &tauri::AppHandle) -> Result<(), String> {
    let lang = app.state::<TrayState>().lang();
    let patrol_enabled = app.state::<core::patrol::PatrolManager>().status().enabled;

    let menu = build_tray_menu(app, &lang, patrol_enabled).map_err(|e| e.to_string())?;
    let tray = app
        .tray_by_id("main-tray")
        .ok_or_else(|| "tray not found".to_string())?;
    tray.set_menu(Some(menu)).map_err(|e| e.to_string())?;
    Ok(())
}

// ── Tray language update command ────────────────────────────────────

#[tauri::command]
async fn update_tray_language(app: tauri::AppHandle, lang: String) -> Result<(), String> {
    app.state::<TrayState>().set_lang(lang);
    refresh_tray_menu(&app)
}
