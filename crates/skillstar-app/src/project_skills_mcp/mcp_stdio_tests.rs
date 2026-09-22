use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

use super::{is_mcp_invocation, is_mcp_serve, serve_with};
use crate::cli::{is_cli_subcommand, is_gui_force_arg};
use crate::test_support::ENV_LOCK;

#[test]
fn mcp_is_a_cli_subcommand_and_not_gui() {
    assert!(is_cli_subcommand("mcp"));
    assert!(!is_gui_force_arg("mcp"));
    assert!(is_gui_force_arg("gui"));
    assert!(!is_cli_subcommand("gui"));
}

#[test]
fn serve_without_stdio_writes_only_stderr() {
    let mut stderr = Vec::new();
    let code = serve_with(
        &["skillstar".into(), "mcp".into(), "serve".into()],
        &mut stderr,
    );
    assert_ne!(code, 0);
    let text = String::from_utf8(stderr).expect("stderr is utf-8");
    assert!(text.contains("--stdio"), "{text}");
    assert!(!text.contains('{'), "usage errors stay off JSON: {text}");
}

#[test]
fn serve_stdio_initialize_emits_only_jsonrpc() {
    let _guard = futures::executor::block_on(ENV_LOCK.lock());
    let temp = tempfile::tempdir().expect("tempdir");
    let previous_data = std::env::var_os("SKILLSTAR_DATA_DIR");
    let previous_home = std::env::var_os("HOME");
    let previous_log = std::env::var_os("RUST_LOG");
    unsafe {
        std::env::set_var("SKILLSTAR_DATA_DIR", temp.path().join("data"));
        std::env::set_var("HOME", temp.path().join("home"));
        std::env::set_var("RUST_LOG", "trace");
    }

    let result = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("runtime")
        .block_on(async {
            let (client, server) = tokio::io::duplex(64 * 1024);
            let server_task = tokio::spawn(super::stdio::serve_transport(server));
            let (client_read, mut client_write) = tokio::io::split(client);
            let initialize = r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2026-07-28","capabilities":{},"clientInfo":{"name":"probe","version":"0"}}}"#;
            client_write.write_all(initialize.as_bytes()).await?;
            client_write.write_all(b"\n").await?;
            let tools = r#"{"jsonrpc":"2.0","id":2,"method":"tools/list","params":{}}"#;
            client_write.write_all(tools.as_bytes()).await?;
            client_write.write_all(b"\n").await?;
            client_write.shutdown().await?;

            let mut lines = BufReader::new(client_read).lines();
            let mut frames = Vec::new();
            while let Some(line) = lines.next_line().await? {
                if !line.is_empty() {
                    frames.push(line);
                }
            }
            server_task.await.expect("server task")
                .expect("server");
            Ok::<_, anyhow::Error>(frames)
        });

    unsafe {
        restore_env("SKILLSTAR_DATA_DIR", previous_data);
        restore_env("HOME", previous_home);
        restore_env("RUST_LOG", previous_log);
    }

    let frames = result.expect("stdio session");
    assert!(frames.len() >= 2, "{frames:?}");
    for frame in &frames {
        assert!(frame.starts_with('{'), "stdout frame is not JSON: {frame}");
        let value: serde_json::Value = serde_json::from_str(frame).expect("json");
        assert_eq!(value["jsonrpc"], "2.0");
        assert!(
            value.get("method").is_none(),
            "server must not emit requests: {frame}"
        );
    }
    let init: serde_json::Value = serde_json::from_str(&frames[0]).unwrap();
    assert_eq!(init["id"], 1);
    assert!(init["result"]["protocolVersion"].is_string());
    assert!(init["result"]["capabilities"].get("roots").is_none());
    let tools: serde_json::Value = serde_json::from_str(&frames[1]).unwrap();
    assert_eq!(tools["id"], 2);
    let mut names: Vec<&str> = tools["result"]["tools"]
        .as_array()
        .expect("tools")
        .iter()
        .map(|tool| tool["name"].as_str().expect("tool name"))
        .collect();
    names.sort_unstable();
    assert_eq!(
        names,
        [
            "apply_project_skills",
            "get_project_skills",
            "recommend_project_skills",
        ]
    );
}

#[test]
fn askpass_env_does_not_swallow_mcp() {
    let _guard = futures::executor::block_on(ENV_LOCK.lock());
    let previous = std::env::var_os("SKILLSTAR_GIT_ASKPASS_MODE");
    unsafe { std::env::set_var("SKILLSTAR_GIT_ASKPASS_MODE", "1") };

    let serve_args = vec![
        "skillstar".into(),
        "mcp".into(),
        "serve".into(),
        "--stdio".into(),
    ];
    let approve_args = vec![
        "skillstar".into(),
        "mcp".into(),
        "approve".into(),
        "abcd".into(),
    ];
    let swallowed = |args: &[String]| {
        if is_mcp_serve(args) || is_mcp_invocation(args) {
            false
        } else {
            skillstar_git::transport::handle_internal_askpass(args)
        }
    };

    unsafe { restore_env("SKILLSTAR_GIT_ASKPASS_MODE", previous) };
    assert!(is_mcp_serve(&serve_args));
    assert!(!is_mcp_serve(&approve_args));
    assert!(!swallowed(&serve_args));
    assert!(!swallowed(&approve_args));
}

unsafe fn restore_env(key: &str, previous: Option<std::ffi::OsString>) {
    match previous {
        Some(value) => unsafe { std::env::set_var(key, value) },
        None => unsafe { std::env::remove_var(key) },
    }
}
