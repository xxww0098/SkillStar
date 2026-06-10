mod cli;
mod commands;
mod core;

use tracing::{error, warn};

use tauri::{Emitter, Manager};
use tauri_plugin_deep_link::DeepLinkExt;

pub(crate) const DEEP_LINK_SCHEME: &str = "skillstar";
pub(crate) const DEEP_LINK_EVENT: &str = "skillstar://deep-link";

#[derive(Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct DeepLinkPayload {
    url: String,
    scheme: String,
    host: Option<String>,
    path: String,
    query: Option<String>,
    fragment: Option<String>,
}

fn deep_link_payload(url: &url::Url) -> Option<DeepLinkPayload> {
    if url.scheme() != DEEP_LINK_SCHEME {
        warn!(target: "deep_link", "ignored unsupported scheme: {}", url.scheme());
        return None;
    }

    Some(DeepLinkPayload {
        url: url.to_string(),
        scheme: url.scheme().to_string(),
        host: url.host_str().map(ToString::to_string),
        path: url.path().to_string(),
        query: url.query().map(ToString::to_string),
        fragment: url.fragment().map(ToString::to_string),
    })
}

fn emit_deep_link(app: &tauri::AppHandle, url: url::Url) {
    let Some(payload) = deep_link_payload(&url) else {
        return;
    };

    if let Err(err) = app.emit(DEEP_LINK_EVENT, payload) {
        warn!(target: "deep_link", "failed to emit deep-link event: {err}");
    }
}

fn setup_deep_links(app: &mut tauri::App) {
    let app_handle = app.handle().clone();

    #[cfg(any(target_os = "windows", target_os = "linux"))]
    if let Err(err) = app.deep_link().register_all() {
        warn!(target: "deep_link", "failed to register configured desktop schemes: {err}");
    }

    match app.deep_link().get_current() {
        Ok(Some(urls)) => {
            for url in urls {
                emit_deep_link(&app_handle, url);
            }
        }
        Ok(None) => {}
        Err(err) => warn!(target: "deep_link", "failed to read current deep-link URLs: {err}"),
    }

    app.deep_link().on_open_url(move |event| {
        for url in event.urls() {
            emit_deep_link(&app_handle, url);
        }
    });
}

#[cfg(test)]
mod tests {
    use super::{DEEP_LINK_EVENT, DEEP_LINK_SCHEME, deep_link_payload};

    #[test]
    fn deep_link_payload_accepts_configured_scheme() {
        let url = url::Url::parse("skillstar://models/cloud-sync?mode=merge#provider").unwrap();
        let payload = deep_link_payload(&url).expect("skillstar scheme should be accepted");

        assert_eq!(DEEP_LINK_SCHEME, "skillstar");
        assert_eq!(DEEP_LINK_EVENT, "skillstar://deep-link");
        assert_eq!(payload.scheme, "skillstar");
        assert_eq!(payload.host.as_deref(), Some("models"));
        assert_eq!(payload.path, "/cloud-sync");
        assert_eq!(payload.query.as_deref(), Some("mode=merge"));
        assert_eq!(payload.fragment.as_deref(), Some("provider"));
    }

    #[test]
    fn deep_link_payload_rejects_other_schemes() {
        let url = url::Url::parse("https://skillstar.local/models").unwrap();

        assert!(deep_link_payload(&url).is_none());
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
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_deep_link::init());

    #[cfg(desktop)]
    {
        builder = builder
            .plugin(tauri_plugin_updater::Builder::new().build())
            .plugin(tauri_plugin_process::init());
    }

    builder
        .manage(core::patrol::PatrolManager::new())
        .manage(core::app_shell::TrayState::new(
            core::app_shell::detect_system_lang(),
        ))
        .manage(core::app_shell::ExitControl::new())
        .manage(commands::updater::PendingUpdate::new())
        .manage(commands::models_commands::ProvidersWriteLock::new())
        .manage(commands::mcp_commands::McpWriteLock::new())
        .setup(|app| {
            // Migrate v1 flat layout → v2 categorised layout (idempotent)
            skillstar_core::infra::migration::migrate_legacy_paths();

            if let Err(err) = core::marketplace::initialize_local_snapshot() {
                error!(target: "marketplace_snapshot", "init failed: {err}");
            }
            setup_deep_links(app);

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

            core::app_shell::setup_tray(app)?;
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
                    let exit_control = app.state::<core::app_shell::ExitControl>();
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
            commands::marketplace::list_marketplace_skills_local,
            commands::marketplace::search_marketplace_local,
            commands::marketplace::get_publishers_local,
            commands::marketplace::get_publisher_repos_local,
            commands::marketplace::get_repo_skills_local,
            commands::marketplace::get_skill_detail_local,
            commands::marketplace::ai_search_marketplace_local,
            commands::marketplace::sync_marketplace_scope,
            commands::marketplace::get_marketplace_sync_states,
            commands::marketplace::list_curated_registries,
            commands::marketplace::upsert_curated_registry,
            commands::marketplace::list_marketplace_source_observations,
            commands::marketplace::list_known_marketplace_sources,
            commands::marketplace::upsert_marketplace_rating_summary,
            commands::marketplace::list_marketplace_rating_summaries,
            commands::marketplace::upsert_marketplace_category,
            commands::marketplace::list_marketplace_categories,
            commands::marketplace::assign_marketplace_skill_categories,
            commands::marketplace::list_marketplace_skill_categories,
            commands::marketplace::upsert_marketplace_tag,
            commands::marketplace::list_marketplace_tags,
            commands::marketplace::assign_marketplace_skill_tags,
            commands::marketplace::list_marketplace_skill_tags,
            commands::marketplace::upsert_marketplace_review,
            commands::marketplace::list_marketplace_reviews,
            commands::marketplace::upsert_marketplace_update_notification,
            commands::marketplace::list_marketplace_update_notifications,
            commands::marketplace::list_marketplace_update_notifications_for_skill,
            commands::marketplace::dismiss_marketplace_update_notification,
            commands::marketplace::search_marketplace_packs,
            commands::marketplace::list_marketplace_packs,
            commands::mcp_marketplace::list_mcp_market_servers_local,
            commands::mcp_marketplace::search_mcp_market_local,
            commands::mcp_marketplace::get_mcp_market_server_detail_local,
            commands::mcp_marketplace::sync_mcp_market_scope,
            commands::mcp_marketplace::get_mcp_market_sync_states,
            commands::mcp_marketplace::mcp_market_entry_to_draft,
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
            commands::install_from_share_code,
            commands::adopt_local_folder,
            commands::get_skill_deploy_status,
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
            commands::ai::summarize::ai_summarize_skill,
            commands::ai::summarize::ai_summarize_skill_stream,
            commands::ai::summarize::ai_test_connection,
            commands::ai::summarize::ai_pick_skills,
            commands::ai::translate::ai_translate_skill,
            commands::ai::translate::ai_translate_skill_stream,
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
            commands::models_commands::get_providers_store,
            commands::models_commands::get_app_providers,
            commands::models_commands::get_provider_presets,
            commands::models_commands::get_provider_presets_flat,
            commands::models_commands::test_endpoints_latency,
            commands::models_commands::set_app_ai_provider_ref,
            commands::models_commands::clear_app_ai_provider_ref,
            commands::models_commands::list_tool_config_files,
            commands::models_commands::read_tool_config_file,
            commands::models_commands::write_tool_config_file,
            commands::models_commands::format_tool_config_file,
            commands::models_commands::push_provider_to_tool_config,
            commands::models_commands::create_provider,
            commands::models_commands::create_provider_from_preset,
            commands::models_commands::update_provider,
            commands::models_commands::delete_provider,
            commands::models_commands::switch_active_provider,
            commands::models_commands::test_provider_latency,
            commands::models_commands::test_all_providers_latency,
            commands::models_commands::get_tool_config_targets,
            commands::models_commands::sync_provider_to_tool,
            commands::models_commands::sync_provider_to_all_tools,
            // Flat store commands (v2)
            commands::models_commands::get_providers_flat,
            commands::models_commands::create_provider_flat,
            commands::models_commands::update_provider_flat,
            commands::models_commands::delete_provider_flat,
            commands::models_commands::reorder_providers,
            commands::models_commands::activate_tool,
            commands::models_commands::deactivate_tool,
            commands::models_commands::update_tool_settings,
            commands::models_commands::get_tool_activations,
            // Model discovery, balance, and tool detection commands
            commands::models_commands::fetch_provider_models,
            commands::models_commands::fetch_provider_model_catalog,
            commands::models_commands::query_provider_balance,
            commands::models_commands::detect_tool_installation,
            commands::models_commands::detect_env_conflicts,
            commands::models_commands::detect_provider_conflicts,
            commands::models_commands::resync_tool,
            commands::models_commands::test_provider_connection,
            commands::mcp_commands::list_mcp_servers,
            commands::mcp_commands::mcp_tool_statuses,
            commands::mcp_commands::create_mcp_server,
            commands::mcp_commands::update_mcp_server,
            commands::mcp_commands::delete_mcp_server,
            commands::mcp_commands::set_mcp_tool_enabled,
            commands::mcp_commands::sync_mcp_server,
            commands::mcp_commands::sync_all_mcp,
            commands::mcp_commands::import_mcp_from_tool,
            commands::mcp_commands::reorder_mcp_servers,
            commands::mcp_commands::get_mcp_presets,
            commands::updater::check_app_update,
            commands::updater::download_and_install_update,
            commands::updater::restart_after_update,
            commands::usage_commands::list_usage_catalog,
            commands::usage_commands::list_subscriptions,
            commands::usage_commands::create_subscription,
            commands::usage_commands::update_subscription,
            commands::usage_commands::delete_subscription,
            commands::usage_commands::reorder_subscriptions,
            commands::usage_commands::refresh_subscription_usage,
            commands::usage_commands::refresh_all_subscriptions,
            commands::usage_commands::get_subscription_alerts,
            commands::usage_commands::dismiss_subscription_alert,
            commands::usage_commands::get_usage_summary,
            commands::usage_commands::start_oauth_login,
            commands::usage_commands::await_oauth_completion,
            commands::usage_commands::submit_oauth_callback,
            commands::usage_commands::cancel_oauth_login,
            // Multi-account (Phase 7)
            commands::usage_commands::get_active_subscriptions,
            commands::usage_commands::set_active_subscription,
            commands::usage_commands::clear_active_subscription,
            commands::usage_commands::import_subscription_from_local,
            commands::usage_commands::get_subscription_api_key,
            // Fingerprint management (Phase 4)
            commands::fingerprints::list_fingerprints,
            commands::fingerprints::get_fingerprint,
            commands::fingerprints::list_fingerprint_presets,
            commands::fingerprints::create_fingerprint_from_preset,
            commands::fingerprints::update_fingerprint,
            commands::fingerprints::delete_fingerprint,
            commands::fingerprints::set_active_fingerprint,
            // IDE projector (Phase 6)
            commands::fingerprints::list_supported_ides,
            commands::fingerprints::apply_fingerprint_to_ide,
            commands::fingerprints::restore_ide_baseline,
            update_tray_language,
        ])
        .build(tauri::generate_context!())
        .expect("error while building SkillStar")
        .run(|app_handle, event| {
            // Prevent the app from exiting when the last window is hidden.
            // This keeps the process alive for background patrol and tray icon.
            if let tauri::RunEvent::ExitRequested { api, .. } = &event {
                let exit_control = app_handle.state::<core::app_shell::ExitControl>();
                if !exit_control.consume_allow_flag() {
                    api.prevent_exit();
                }
            }
        });
}

// ── Tray language update command ────────────────────────────────────

#[tauri::command]
async fn update_tray_language(app: tauri::AppHandle, lang: String) -> Result<(), String> {
    app.state::<core::app_shell::TrayState>().set_lang(lang);
    core::app_shell::refresh_tray_menu(&app)
}
