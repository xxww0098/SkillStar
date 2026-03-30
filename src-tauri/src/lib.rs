mod cli;
mod commands;
mod core;

use tauri::Manager;

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
        .setup(|app| {
            setup_tray(app)?;
            setup_patrol_auto_resume(app.handle().clone());
            Ok(())
        })
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                // If patrol is running, hide window instead of quitting
                let manager = window.state::<core::patrol::PatrolManager>();
                let is_running = tauri::async_runtime::block_on(manager.is_running());
                if is_running {
                    api.prevent_close();
                    let _ = window.hide();
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
            commands::projects::update_project_path,
            commands::projects::remove_project,
            commands::projects::scan_project_skills,
            commands::projects::import_project_skills,
            commands::ai::get_ai_config,
            commands::ai::save_ai_config,
            commands::ai::ai_translate_skill,
            commands::ai::ai_translate_skill_stream,
            commands::ai::ai_translate_short_text_stream,
            commands::ai::ai_summarize_skill,
            commands::ai::ai_summarize_skill_stream,
            commands::ai::ai_test_connection,
            commands::ai::ai_pick_skills,
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
            commands::patrol::start_patrol,
            commands::patrol::stop_patrol,
            commands::patrol::get_patrol_status,
            commands::patrol::get_patrol_config,
            commands::patrol::save_patrol_config,
            commands::patrol::set_dock_visible,
        ])
        .run(tauri::generate_context!())
        .expect("error while running SkillStar");
}

// ── System Tray ─────────────────────────────────────────────────────

fn setup_tray(app: &tauri::App) -> Result<(), Box<dyn std::error::Error>> {
    use tauri::menu::{MenuBuilder, MenuItem};
    use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};

    let show_i = MenuItem::with_id(app, "show", "显示窗口 / Show", true, None::<&str>)?;
    let stop_i = MenuItem::with_id(app, "stop_patrol", "退出隐遁 / Stop Patrol", true, None::<&str>)?;
    let quit_i = MenuItem::with_id(app, "quit", "退出 / Quit", true, None::<&str>)?;

    let menu = MenuBuilder::new(app)
        .item(&show_i)
        .separator()
        .item(&stop_i)
        .separator()
        .item(&quit_i)
        .build()?;

    TrayIconBuilder::with_id("main-tray")
        .tooltip("SkillStar")
        .icon(app.default_window_icon().unwrap().clone())
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "show" => {
                if let Some(window) = app.get_webview_window("main") {
                    let _ = window.show();
                    let _ = window.unminimize();
                    let _ = window.set_focus();
                }
            }
            "stop_patrol" => {
                let manager = app.state::<core::patrol::PatrolManager>();
                tauri::async_runtime::block_on(manager.stop());
                if let Some(window) = app.get_webview_window("main") {
                    let _ = window.show();
                    let _ = window.unminimize();
                    let _ = window.set_focus();
                }
            }
            "quit" => {
                let manager = app.state::<core::patrol::PatrolManager>();
                tauri::async_runtime::block_on(manager.stop());
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

// ── Patrol Auto-Resume ──────────────────────────────────────────────

fn setup_patrol_auto_resume(app: tauri::AppHandle) {
    let config = core::patrol::load_config();
    if config.enabled {
        let interval = config.interval_secs;
        tauri::async_runtime::spawn(async move {
            let manager = app.state::<core::patrol::PatrolManager>();
            if let Err(e) = manager.start(app.clone(), interval).await {
                eprintln!("[patrol] Auto-resume failed: {}", e);
            }
        });
    }
}
