use super::apps::{
    DesktopAppId, EnvAssignmentValue, InstanceCapability, UserDataDirForm, open_argv,
};
use super::error::{
    CLAUDE_DESKTOP_REASON, GITHUB_COPILOT_REASON, InstanceError, ZED_INSTANCE_REASON,
};
use super::process::{cmdline_uses_user_data_dir, parse_ps_line};
#[cfg(not(target_os = "macos"))]
use super::start_instance;
use super::{create_instance, list_desktop_apps, list_instances};
use crate::test_support::EnvGuard;
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
fn legacy_windsurf_app_id_still_parses_onto_devin_desktop() {
    assert_eq!(
        DesktopAppId::parse("windsurf").unwrap(),
        DesktopAppId::DevinDesktop
    );
    let stored: DesktopAppId = serde_json::from_str("\"windsurf\"").unwrap();
    assert_eq!(stored, DesktopAppId::DevinDesktop);
    assert_eq!(
        serde_json::to_string(&DesktopAppId::DevinDesktop).unwrap(),
        "\"devin-desktop\""
    );
}

#[test]
fn parse_rejects_catalog_bindings() {
    let xai = DesktopAppId::parse("xai").expect_err("xai");
    assert!(xai.to_string().contains("没有独立 IDE"));
    let anthropic = DesktopAppId::parse("anthropic").expect_err("anthropic");
    assert!(anthropic.to_string().contains("Claude Desktop"));
}

#[test]
fn list_desktop_apps_is_the_verified_ide_apps() {
    let apps = list_desktop_apps();
    let ids: Vec<_> = apps.iter().map(|a| a.id).collect();
    assert_eq!(ids, vec![DesktopAppId::Cursor, DesktopAppId::Antigravity]);
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
    let argv = open_argv(DesktopAppId::Antigravity, dir).expect("chromium argv");
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
fn cursor_uses_separate_form() {
    let dir = Path::new("/tmp/skillstar-instances/cursor/abc");
    let cursor = open_argv(DesktopAppId::Cursor, dir).expect("chromium argv");
    assert_eq!(
        &cursor[5..],
        [
            "--user-data-dir",
            "/tmp/skillstar-instances/cursor/abc",
            "--new-window"
        ]
    );
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
    let temp = TempDir::new().unwrap();
    let _guard = EnvGuard::set(&[("SKILLSTAR_DATA_DIR", temp.path())]);

    let created = create_instance("cursor", "Work".into()).unwrap();
    let expected = temp
        .path()
        .join("data")
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
    let temp = TempDir::new().unwrap();
    let _guard = EnvGuard::set(&[("SKILLSTAR_DATA_DIR", temp.path())]);

    let err = create_instance("claude-desktop", "Nope".into()).unwrap_err();
    assert!(err.to_string().contains("Claude Desktop"));
    assert!(!temp.path().join("instances").exists());
}

#[tokio::test]
#[cfg(not(target_os = "macos"))]
async fn start_is_macos_only() {
    let temp = TempDir::new().unwrap();
    let _guard = EnvGuard::set(&[("SKILLSTAR_DATA_DIR", temp.path())]);

    let created = create_instance("cursor", "Work".into()).unwrap();
    let err = start_instance(&created.id).unwrap_err();
    assert!(err.to_string().contains("macOS"));
}

#[test]
fn parse_rejects_zed_like_claude_desktop() {
    for id in ["zed", "Zed", "Zed.app"] {
        let err = DesktopAppId::parse(id).expect_err(id);
        match err {
            InstanceError::UnsupportedApp(reason) => {
                assert_eq!(reason, ZED_INSTANCE_REASON);
                assert!(reason.contains("--user-data-dir"));
                assert!(reason.contains("钥匙串"));
            }
            other => panic!("expected UnsupportedApp, got {other}"),
        }
    }
    assert!(
        DesktopAppId::candidates()
            .into_iter()
            .all(|app| app.as_str() != "zed")
    );
}

#[test]
fn parse_rejects_github_copilot_without_a_vscode_injector() {
    let err = DesktopAppId::parse("github-copilot").expect_err("copilot");
    match err {
        InstanceError::UnsupportedApp(reason) => {
            assert_eq!(reason, GITHUB_COPILOT_REASON);
            assert!(reason.contains("不是独立桌面应用"));
            assert!(reason.contains("VS Code"));
        }
        other => panic!("expected UnsupportedApp, got {other}"),
    }
}

#[test]
fn candidates_roundtrip_their_ids() {
    assert_eq!(
        DesktopAppId::candidates().map(|app| app.as_str()),
        ["cursor", "antigravity", "devin-desktop", "kiro", "zcode"]
    );
    for app in DesktopAppId::candidates() {
        assert_eq!(DesktopAppId::parse(app.as_str()).unwrap(), app);
    }
}

#[test]
fn pending_candidates_parse_but_stay_out_of_the_picker() {
    let pending = ["devin-desktop", "kiro", "zcode"];
    let listed: Vec<_> = list_desktop_apps().into_iter().map(|app| app.id).collect();
    for id in pending {
        let app = DesktopAppId::parse(id).unwrap();
        assert_eq!(app.as_str(), id);
        assert_eq!(app.capability(), InstanceCapability::Pending);
        assert_eq!(app.catalog_id(), Some(id));
        assert!(!listed.contains(&app), "{id} must not be listed");
        assert_eq!(serde_json::to_string(&app).unwrap(), format!("\"{id}\""));
    }
    for app in [DesktopAppId::Cursor, DesktopAppId::Antigravity] {
        assert_eq!(app.capability(), InstanceCapability::Verified);
        assert!(listed.contains(&app));
    }
}

#[test]
fn pending_chromium_apps_use_separate_form_not_equals() {
    let cases = [
        (DesktopAppId::DevinDesktop, "Devin.app"),
        (DesktopAppId::Kiro, "Kiro.app"),
    ];
    let dir = Path::new("/tmp/skillstar-instances/pending/abc");
    for (app, bundle) in cases {
        let argv = open_argv(app, dir).expect("separate-form argv");
        assert_eq!(
            argv,
            vec![
                "/usr/bin/open",
                "-n",
                "-a",
                bundle,
                "--args",
                "--user-data-dir",
                "/tmp/skillstar-instances/pending/abc",
                "--new-window",
            ]
        );
        assert!(
            argv.windows(2)
                .any(|w| w[0] == "--user-data-dir" && w[1] == dir.to_str().unwrap())
        );
        assert!(!argv.iter().any(|arg| arg.starts_with("--user-data-dir=")));
        assert!(app.env_assignments(dir, "Work").is_none());
        assert_eq!(app.process_match_dir(dir), dir);
        match app.launch_spec().mode {
            super::apps::LaunchMode::OpenArgs {
                user_data_dir_form: UserDataDirForm::Separate,
                ..
            } => {}
            other => panic!("{bundle} should be separate-form open args, got {other:?}"),
        }
    }
}

#[test]
fn zcode_env_spawn_has_no_user_data_dir_flag() {
    let app = DesktopAppId::parse("zcode").unwrap();
    let root = Path::new("/tmp/skillstar-instances/zcode/abc");
    assert!(open_argv(app, root).is_none());
    assert_eq!(app.launch_spec().macos_app_name, "ZCode.app");
    assert_eq!(app.process_match_dir(root), root.join("electron"));

    let env = app.env_assignments(root, "Work").expect("env spawn");
    let keys: Vec<_> = env.iter().map(|row| row.key).collect();
    assert_eq!(
        keys,
        [
            "ZCODE_DESKTOP_USER_DATA_DIR",
            "ZCODE_DESKTOP_SESSION_DATA_DIR",
            "ZCODE_DATA_BASE_DIR",
            "ZCODE_DESKTOP_HOME_DIR",
            "ZCODE_CREDENTIAL_SECRET",
            "ZCODE_DESKTOP_APPLICATION_NAME",
            "HOME",
            "USERPROFILE",
        ]
    );
    let value = |key: &str| env.iter().find(|row| row.key == key).unwrap();
    assert_eq!(
        value("ZCODE_DESKTOP_USER_DATA_DIR").value,
        EnvAssignmentValue::Path(root.join("electron").to_string_lossy().into_owned())
    );
    assert_eq!(
        value("ZCODE_DESKTOP_SESSION_DATA_DIR").value,
        EnvAssignmentValue::Path(root.join("electron/session").to_string_lossy().into_owned())
    );
    assert_eq!(
        value("ZCODE_DATA_BASE_DIR").value,
        EnvAssignmentValue::Path(root.join("data").to_string_lossy().into_owned())
    );
    assert_eq!(
        value("ZCODE_DESKTOP_HOME_DIR").value,
        value("ZCODE_DATA_BASE_DIR").value
    );
    assert!(!value("ZCODE_DATA_BASE_DIR").unix_only);
    assert!(value("HOME").unix_only);
    assert!(value("USERPROFILE").unix_only);
    assert_eq!(value("HOME").value, value("ZCODE_DATA_BASE_DIR").value);
    assert_eq!(
        value("ZCODE_DESKTOP_APPLICATION_NAME").value,
        EnvAssignmentValue::Literal("ZCode [Work]".to_string())
    );
    assert_eq!(
        value("ZCODE_CREDENTIAL_SECRET").value,
        EnvAssignmentValue::RealHomeCredential
    );
    assert!(env.iter().all(|row| !row.key.contains("user-data-dir")));
}

#[tokio::test]
async fn create_rejects_pending_and_blocked_without_writing_a_profile() {
    let temp = TempDir::new().unwrap();
    let _guard = EnvGuard::set(&[("SKILLSTAR_DATA_DIR", temp.path())]);

    for id in ["devin-desktop", "zcode", "kiro"] {
        let err = create_instance(id, "Nope".into()).unwrap_err();
        assert!(err.to_string().contains("Pending"), "{id}: {err}");
    }
    for id in ["zed", "Zed.app", "github-copilot"] {
        let err = create_instance(id, "Nope".into()).unwrap_err();
        assert!(!err.to_string().contains("Pending"), "{id}: {err}");
    }
    assert!(!temp.path().join("instances").exists());
}

#[tokio::test]
async fn removed_apps_are_rejected_and_their_records_survive_supported_edits() {
    let temp = TempDir::new().unwrap();
    let _guard = EnvGuard::set(&[("SKILLSTAR_DATA_DIR", temp.path())]);
    let retired: Vec<_> = [
        "grok-bot",
        "qoder",
        "codebuddy",
        "codebuddy-cn",
        "trae",
        "trae-solo",
        "trae-cn",
        "trae-solo-cn",
    ]
    .into_iter()
    .map(|id| {
        assert!(DesktopAppId::parse(id).is_err());
        assert!(create_instance(id, "Nope".into()).is_err());
        assert!(list_instances(id).is_err());
        serde_json::json!({"id": id, "app": id, "name": "Old", "created_at": 0,
            "extra_args": [], "future_field": "preserved"})
    })
    .collect();
    let path = ss_core::infra::paths::app_instances_config_path();
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(
        &path,
        serde_json::to_vec(&serde_json::json!({
            "version": 1, "instances": retired
        }))
        .unwrap(),
    )
    .unwrap();
    assert!(super::store::list_stored(None).unwrap().is_empty());
    assert!(super::store::delete_stored("grok-bot").is_err());
    let created = create_instance("cursor", "Work".into()).unwrap();
    assert_eq!(list_instances("cursor").unwrap().len(), 1);
    super::store::delete_stored(&created.id).unwrap();
    let saved: serde_json::Value = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
    assert_eq!(saved["instances"], serde_json::json!(retired));
}

#[tokio::test]
async fn malformed_supported_instance_is_still_an_error() {
    let temp = TempDir::new().unwrap();
    let _guard = EnvGuard::set(&[("SKILLSTAR_DATA_DIR", temp.path())]);
    let path = ss_core::infra::paths::app_instances_config_path();
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, r#"{"instances":[{"app":"cursor"}]}"#).unwrap();
    assert!(super::store::list_stored(None).is_err());
}
