mod cli;
mod commands;
mod core;

use tracing::error;

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

pub fn run_cli(args: Vec<String>) {
    cli::run(args);
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // Initialize tracing subscriber — defaults to INFO, override with RUST_LOG env.
    // Set SKILLSTAR_LOG_JSON=1 for structured JSON output (production debugging).
    let env_filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info"));

    if std::env::var("SKILLSTAR_LOG_JSON").is_ok() {
        tracing_subscriber::fmt()
            .with_env_filter(env_filter)
            .json()
            .with_target(true)
            .with_span_events(tracing_subscriber::fmt::format::FmtSpan::CLOSE)
            .init();
    } else {
        tracing_subscriber::fmt()
            .with_env_filter(env_filter)
            .with_target(true)
            .without_time()
            .init();
    }

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
        .manage(commands::updater::PendingUpdate::new())
        .setup(|app| {
            // Migrate v1 flat layout → v2 categorised layout (idempotent)
            core::infra::migration::migrate_legacy_paths();

            if let Err(err) = core::marketplace::initialize_local_snapshot() {
                error!(target: "marketplace_snapshot", "init failed: {err}");
            }
            // Run translation cache LRU cleanup (once per process, non-blocking)
            core::ai::translation_cache::startup_cleanup();

            let app_handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                if let Err(err) = core::marketplace::refresh_local_snapshot_startup_scopes().await {
                    error!(target: "marketplace_snapshot", "startup refresh failed: {err}");
                }
                let _ = app_handle.emit("marketplace://ready", ());
            });

            // ── Windows: force webview resize to fix WebView2 bounds desync ──
            // WebView2 can initialize with stale/wrong bounds, causing content
            // to render only in the top half of the window. After a short delay
            // we read the actual window inner_size and force-set it back, which
            // triggers WebView2 to recalculate its layout.
            #[cfg(target_os = "windows")]
            {
                if let Some(window) = app.get_webview_window("main") {
                    let win = window.clone();
                    tauri::async_runtime::spawn(async move {
                        // Give WebView2 time to finish initialization
                        tokio::time::sleep(std::time::Duration::from_millis(150)).await;
                        if let Ok(size) = win.inner_size() {
                            // Force a 1px resize then restore — triggers WM_SIZE
                            let adjusted =
                                tauri::PhysicalSize::new(size.width.saturating_sub(1), size.height);
                            let _ = win.set_size(tauri::Size::Physical(adjusted));
                            tokio::time::sleep(std::time::Duration::from_millis(16)).await;
                            let _ = win.set_size(tauri::Size::Physical(size));
                        }
                    });
                }
            }

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
            commands::marketplace::search_marketplace_packs,
            commands::marketplace::list_marketplace_packs,
            commands::github::check_gh_installed,
            commands::github::check_gh_status,
            commands::github::check_git_status,
            commands::github::check_developer_mode,
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
            commands::agents::batch_remove_skills_from_all_agents,
            commands::agents::add_custom_agent_profile,
            commands::agents::remove_custom_agent_profile,
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
            commands::get_github_mirror_config,
            commands::save_github_mirror_config,
            commands::get_github_mirror_presets,
            commands::test_github_mirror,
            commands::projects::register_project,
            commands::projects::list_projects,
            commands::projects::get_project_skills,
            commands::projects::save_and_sync_project,
            commands::projects::save_project_skills_list,
            commands::projects::update_project_path,
            commands::projects::remove_project,
            commands::projects::scan_project_skills,
            commands::projects::refresh_stale_project_copies,
            commands::projects::rebuild_project_skills_from_disk,
            commands::projects::import_project_skills,
            commands::projects::detect_project_agents,
            commands::ai::get_ai_config,
            commands::ai::save_ai_config,
            commands::ai::get_translation_api_config,
            commands::ai::save_translation_api_config,
            commands::ai::get_translation_settings,
            commands::ai::save_translation_settings,
            commands::ai::get_translation_readiness,
            commands::ai::test_translation_provider,
            commands::ai::translate::ai_translate_skill,
            commands::ai::translate::ai_translate_skill_stream,
            commands::ai::translate::ai_translate_short_text_stream_with_source,
            commands::ai::translate::ai_retranslate_short_text_stream_with_source,
            commands::ai::summarize::ai_summarize_skill,
            commands::ai::summarize::ai_summarize_skill_stream,
            commands::ai::summarize::ai_test_connection,
            commands::ai::summarize::ai_pick_skills,
            commands::ai::translate::ai_batch_process_skills,
            commands::ai::translate::check_pending_batch_translate,
            commands::ai::scan::ai_security_scan,
            commands::ai::scan::estimate_security_scan,
            commands::ai::scan::cancel_security_scan,
            commands::ai::scan::get_cached_scan_results,
            commands::ai::scan::clear_security_scan_cache,
            commands::ai::scan::list_security_scan_logs,
            commands::ai::scan::get_security_scan_log_dir,
            commands::ai::scan::get_security_scan_policy,
            commands::ai::scan::save_security_scan_policy,
            commands::ai::scan::export_security_scan_sarif,
            commands::ai::scan::export_security_scan_report,
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
            commands::github::install_pack_from_url,
            commands::github::list_installed_packs,
            commands::github::remove_installed_pack,
            commands::github::get_pack_doctor,
            commands::github::check_new_repo_skills,
            commands::github::dismiss_new_skill,
            commands::github::get_dismissed_new_skills,
            commands::github::dismiss_new_skills_batch,
            commands::export_skill_bundle,
            commands::preview_skill_bundle,
            commands::import_skill_bundle,
            commands::export_multi_skill_bundle,
            commands::preview_multi_skill_bundle,
            commands::import_multi_skill_bundle,
            commands::write_text_file,
            commands::read_text_file,
            commands::open_external_url,
            commands::open_folder,
            commands::patrol::start_patrol,
            commands::patrol::stop_patrol,
            commands::patrol::get_patrol_status,
            commands::patrol::set_patrol_enabled,
            commands::patrol::app_quit,
            commands::patrol::set_dock_visible,
            commands::acp::get_acp_config,
            commands::acp::save_acp_config,
            commands::launch::get_launch_config,
            commands::launch::save_launch_config,
            commands::launch::delete_launch_config,
            commands::launch::deploy_launch,
            commands::launch::list_agent_clis,
            commands::updater::check_app_update,
            commands::updater::download_and_install_update,
            commands::updater::restart_after_update,
            commands::models::get_model_config_status,
            commands::models::get_claude_model_config,
            commands::models::save_claude_model_config,
            commands::models::get_codex_model_config,
            commands::models::save_codex_model_config,
            commands::models::get_codex_auth,
            commands::models::save_codex_auth,
            commands::models::get_codex_auth_status,
            commands::models::codex_oauth_start,
            commands::models::codex_oauth_complete,
            commands::models::codex_oauth_cancel,
            commands::models::codex_oauth_submit_callback,
            commands::models::gemini_oauth_start,
            commands::models::gemini_oauth_complete,
            commands::models::gemini_oauth_cancel,
            commands::models::gemini_oauth_submit_callback,
            commands::models::gemini_oauth_is_configured,
            commands::models::list_codex_accounts,
            commands::models::get_current_codex_account_id,
            commands::models::switch_codex_account,
            commands::models::delete_codex_account,
            commands::models::refresh_codex_quota,
            commands::models::refresh_all_codex_quotas,
            commands::models::add_codex_api_key_account,
            commands::models::refresh_gemini_quota,
            commands::models::get_opencode_model_config,
            commands::models::save_opencode_model_config,
            commands::models::set_claude_setting,
            commands::models::set_codex_setting,
            commands::models::set_opencode_setting,
            commands::models::test_model_endpoints,
            commands::models::get_opencode_cli_models,
            commands::models::get_opencode_auth_providers,
            commands::models::add_opencode_auth_provider,
            commands::models::remove_opencode_auth_provider,
            commands::models::open_opencode_config_dir,
            commands::models::open_opencode_auth_dir,
            commands::models::get_model_providers,
            commands::models::switch_model_provider,
            commands::models::add_model_provider,
            commands::models::update_model_provider,
            commands::models::delete_model_provider,
            commands::models::reorder_model_providers,
            commands::models::fetch_endpoint_models,
            commands::models::read_model_config_text,
            commands::models::write_model_config_text,
            commands::models::format_model_config_text,
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

    let store = core::model_config::providers::read_store().unwrap_or_default();

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
            let accounts = core::model_config::codex_accounts::list_accounts()
                .into_iter()
                .filter(|a| a.auth_mode == "oauth" || a.auth_mode == "apikey")
                .collect::<Vec<_>>();

            if !accounts.is_empty() {
                if has_items {
                    sub_builder = sub_builder.separator();
                }
                has_items = true;
                let current_account_id =
                    core::model_config::codex_accounts::get_current_account_id();
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
            id if id.starts_with("provider_") => {
                let parts: Vec<&str> = id.splitn(3, '_').collect();
                if parts.len() == 3 {
                    let app_id = parts[1];
                    let provider_id = parts[2];
                    if let Err(e) =
                        core::model_config::providers::switch_provider(app_id, provider_id)
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
                        match crate::commands::models::switch_codex_account(
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
