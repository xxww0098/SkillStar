mod cli;
mod commands;
mod core;

use std::sync::atomic::{AtomicBool, Ordering};
use tauri::{Emitter, Manager};

pub struct ExitControl {
    allow_exit: AtomicBool,
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
        .manage(ExitControl::new())
        .setup(|app| {
            setup_tray(app)?;
            Ok(())
        })
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                // Prevent native close and hide the window to background.
                api.prevent_close();
                let _ = window.hide();
                // Hide the Dock icon — keep only the tray icon visible.
                #[cfg(target_os = "macos")]
                {
                    let _ = window
                        .app_handle()
                        .set_activation_policy(tauri::ActivationPolicy::Accessory);
                }
                // Notify frontend so it can optionally start patrol.
                let _ = window.emit("skillstar://window-hidden", ());
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
            commands::marketplace::hydrate_marketplace_descriptions,
            commands::marketplace::get_publisher_repos,
            commands::marketplace::get_publisher_repo_skills,
            commands::marketplace::get_marketplace_skill_details,
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
            commands::ai::ai_translate_short_text_stream,
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

/// Returns (show_label, stop_patrol_label, quit_label) for the given language.
fn tray_labels(lang: &str) -> (&'static str, &'static str, &'static str) {
    if lang.starts_with("zh") {
        ("显示窗口", "停止后台检查", "退出")
    } else {
        ("Show Window", "Stop Background Check", "Quit")
    }
}

fn build_tray_menu(
    app: &impl Manager<tauri::Wry>,
    lang: &str,
) -> Result<tauri::menu::Menu<tauri::Wry>, Box<dyn std::error::Error>> {
    use tauri::menu::{MenuBuilder, MenuItem};

    let (show_label, stop_label, quit_label) = tray_labels(lang);

    let show_i = MenuItem::with_id(app, "show", show_label, true, None::<&str>)?;
    let stop_i = MenuItem::with_id(app, "stop_patrol", stop_label, true, None::<&str>)?;
    let quit_i = MenuItem::with_id(app, "quit", quit_label, true, None::<&str>)?;

    let menu = MenuBuilder::new(app)
        .item(&show_i)
        .separator()
        .item(&stop_i)
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

    let lang = detect_system_lang();
    let menu = build_tray_menu(app, lang)?;

    TrayIconBuilder::with_id("main-tray")
        .tooltip("SkillStar")
        .icon(app.default_window_icon().unwrap().clone())
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
            "stop_patrol" => {
                let manager = app.state::<core::patrol::PatrolManager>();
                tauri::async_runtime::block_on(manager.stop());
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
            "quit" => {
                let manager = app.state::<core::patrol::PatrolManager>();
                tauri::async_runtime::block_on(manager.stop());
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

// ── Tray language update command ────────────────────────────────────

#[tauri::command]
async fn update_tray_language(app: tauri::AppHandle, lang: String) -> Result<(), String> {
    let menu = build_tray_menu(&app, &lang).map_err(|e| e.to_string())?;
    let tray = app
        .tray_by_id("main-tray")
        .ok_or_else(|| "tray not found".to_string())?;
    tray.set_menu(Some(menu)).map_err(|e| e.to_string())?;
    Ok(())
}
