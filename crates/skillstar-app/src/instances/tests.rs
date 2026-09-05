use super::apps::{DesktopAppId, open_argv};
use super::error::{CLAUDE_DESKTOP_REASON, InstanceError};
use super::process::{cmdline_uses_user_data_dir, parse_ps_line};
use super::{create_instance, list_desktop_apps, list_instances};
use crate::test_support::{ENV_LOCK, EnvGuard};
use std::path::Path;
use tempfile::TempDir;

#[test]
fn parse_rejects_claude_desktop() {
    for id in ["claude", "claude-desktop", "Claude", "Claude.app"] {
        let err = DesktopAppId::parse(id).expect_err(id);
        match err {
            InstanceError::UnsupportedApp(reason) => {
                assert_eq!(reason, CLAUDE_DESKTOP_REASON);
                assert!(reason.contains("~/Library/Application Support/Claude"));
                assert!(reason.contains("--user-data-dir"));
            }
            other => panic!("expected UnsupportedApp, got {other}"),
        }
    }
}

#[test]
fn parse_rejects_catalog_bindings() {
    let xai = DesktopAppId::parse("xai").expect_err("xai");
    assert!(xai.to_string().contains("grok-bot"));
    let anthropic = DesktopAppId::parse("anthropic").expect_err("anthropic");
    assert!(anthropic.to_string().contains("Claude Desktop"));
}

#[test]
fn grok_bot_is_not_xai() {
    let app = DesktopAppId::parse("grok-bot").unwrap();
    assert_eq!(app.as_str(), "grok-bot");
    assert_eq!(app.catalog_id(), None);
    assert_eq!(app.launch_spec().macos_app_name, "Grok Bot.app");
}

#[test]
fn list_desktop_apps_is_the_three_working_apps() {
    let apps = list_desktop_apps();
    let ids: Vec<_> = apps.iter().map(|a| a.id).collect();
    assert_eq!(
        ids,
        vec![
            DesktopAppId::Cursor,
            DesktopAppId::GrokBot,
            DesktopAppId::Antigravity
        ]
    );
    assert!(apps.iter().all(|app| app.id.catalog_id() != Some("xai")));
    assert!(
        apps.iter()
            .all(|app| app.id.catalog_id() != Some("anthropic"))
    );
    assert!(DesktopAppId::parse("claude").is_err());
}

#[test]
fn antigravity_argv_uses_equals_form() {
    let dir = Path::new("/tmp/skillstar-instances/antigravity/abc");
    let argv = open_argv(DesktopAppId::Antigravity, dir);
    assert_eq!(
        argv,
        vec![
            "/usr/bin/open",
            "-n",
            "-a",
            "Antigravity.app",
            "--args",
            "--user-data-dir=/tmp/skillstar-instances/antigravity/abc",
            "--new-window",
        ]
    );
    assert!(
        !argv.windows(2).any(
            |w| w[0] == "--user-data-dir" && w[1] == "/tmp/skillstar-instances/antigravity/abc"
        )
    );
}

#[test]
fn cursor_and_grok_bot_use_separate_form() {
    let dir = Path::new("/tmp/skillstar-instances/cursor/abc");
    let cursor = open_argv(DesktopAppId::Cursor, dir);
    assert_eq!(
        &cursor[5..],
        [
            "--user-data-dir",
            "/tmp/skillstar-instances/cursor/abc",
            "--new-window"
        ]
    );

    let grok = open_argv(
        DesktopAppId::GrokBot,
        Path::new("/tmp/skillstar-instances/grok-bot/xyz"),
    );
    assert_eq!(
        &grok[3..],
        [
            "Grok Bot.app",
            "--args",
            "--user-data-dir",
            "/tmp/skillstar-instances/grok-bot/xyz"
        ]
    );
    assert!(!grok.iter().any(|arg| arg == "--new-window"));
}

#[test]
fn cmdline_match_accepts_both_forms_and_rejects_prefix() {
    let dir = Path::new("/tmp/inst/a");
    assert!(cmdline_uses_user_data_dir(
        "Cursor --user-data-dir /tmp/inst/a --new-window",
        dir
    ));
    assert!(cmdline_uses_user_data_dir(
        "Antigravity --user-data-dir=/tmp/inst/a --new-window",
        dir
    ));
    assert!(cmdline_uses_user_data_dir(
        r#"Antigravity --user-data-dir="/tmp/inst/a" --new-window"#,
        dir
    ));
    assert!(!cmdline_uses_user_data_dir(
        "Antigravity --user-data-dir=/tmp/inst/ab --new-window",
        dir
    ));
    assert!(!cmdline_uses_user_data_dir(
        "Cursor --user-data-dir /tmp/inst/ab --new-window",
        dir
    ));
    assert!(!cmdline_uses_user_data_dir(
        "Cursor --user-data-dir /tmp/other",
        dir
    ));
}

#[test]
fn parse_ps_line_reads_pid_and_command() {
    let (pid, cmd) = parse_ps_line(
        "  4321 /Applications/Cursor.app/Contents/MacOS/Cursor --user-data-dir /tmp/a",
    )
    .unwrap();
    assert_eq!(pid, 4321);
    assert!(cmd.contains("--user-data-dir"));
    assert!(parse_ps_line("   0 kernel_task").is_none());
}

#[tokio::test]
async fn create_instance_uses_skillstar_instances_layout() {
    let _lock = ENV_LOCK.lock().await;
    let temp = TempDir::new().unwrap();
    let _guard = EnvGuard::set(&[("SKILLSTAR_DATA_DIR", temp.path())]);

    let created = create_instance("cursor", "Work".into()).unwrap();
    let expected = temp
        .path()
        .join("instances")
        .join("cursor")
        .join(&created.id);
    assert_eq!(created.user_data_dir, expected.to_string_lossy());
    assert!(expected.is_dir());
    assert_eq!(created.app, DesktopAppId::Cursor);
    assert_eq!(created.name, "Work");
    assert!(!created.running);

    let listed = list_instances("cursor").unwrap();
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].id, created.id);
}

#[tokio::test]
async fn create_rejects_claude_and_does_not_write_a_profile() {
    let _lock = ENV_LOCK.lock().await;
    let temp = TempDir::new().unwrap();
    let _guard = EnvGuard::set(&[("SKILLSTAR_DATA_DIR", temp.path())]);

    let err = create_instance("claude-desktop", "Nope".into()).unwrap_err();
    assert!(err.to_string().contains("Claude Desktop"));
    assert!(!temp.path().join("instances").exists());
}

#[tokio::test]
#[cfg(not(target_os = "macos"))]
async fn start_is_macos_only() {
    let _lock = ENV_LOCK.lock().await;
    let temp = TempDir::new().unwrap();
    let _guard = EnvGuard::set(&[("SKILLSTAR_DATA_DIR", temp.path())]);

    let created = create_instance("grok-bot", "Bot".into()).unwrap();
    let err = start_instance(&created.id).unwrap_err();
    assert!(err.to_string().contains("macOS"));
}
