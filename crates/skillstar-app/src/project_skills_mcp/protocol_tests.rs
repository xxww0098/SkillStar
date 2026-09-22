use std::ffi::OsString;
use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

use skillstar_core::infra::paths as fs_paths;

struct EnvGuard {
    root: PathBuf,
    home: Option<OsString>,
    data: Option<OsString>,
    hub: Option<OsString>,
    log: Option<OsString>,
    #[cfg(windows)]
    userprofile: Option<OsString>,
    _lock: tokio::sync::MutexGuard<'static, ()>,
}

impl EnvGuard {
    fn new(label: &str) -> Self {
        let _lock = futures::executor::block_on(crate::test_support::ENV_LOCK.lock());
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let root = std::env::temp_dir().join(format!("skillstar-protocol-{label}-{nanos}"));
        fs::create_dir_all(root.join("home")).unwrap();
        fs::create_dir_all(root.join("data")).unwrap();
        let guard = Self {
            home: std::env::var_os("HOME"),
            data: std::env::var_os("SKILLSTAR_DATA_DIR"),
            hub: std::env::var_os("SKILLSTAR_HUB_DIR"),
            log: std::env::var_os("RUST_LOG"),
            #[cfg(windows)]
            userprofile: std::env::var_os("USERPROFILE"),
            root,
            _lock,
        };
        unsafe {
            std::env::set_var("HOME", guard.root.join("home"));
            std::env::set_var("SKILLSTAR_DATA_DIR", guard.root.join("data"));
            std::env::remove_var("SKILLSTAR_HUB_DIR");
            std::env::set_var("RUST_LOG", "error");
            #[cfg(windows)]
            std::env::set_var("USERPROFILE", guard.root.join("home"));
        }
        guard
    }

    fn project(&self, name: &str) -> PathBuf {
        let path = self.root.join(name);
        fs::create_dir_all(&path).unwrap();
        fs::canonicalize(&path).unwrap()
    }
}

impl Drop for EnvGuard {
    fn drop(&mut self) {
        unsafe {
            restore("HOME", self.home.take());
            restore("SKILLSTAR_DATA_DIR", self.data.take());
            restore("SKILLSTAR_HUB_DIR", self.hub.take());
            restore("RUST_LOG", self.log.take());
            #[cfg(windows)]
            restore("USERPROFILE", self.userprofile.take());
        }
        let _ = fs::remove_dir_all(&self.root);
    }
}

unsafe fn restore(key: &str, previous: Option<OsString>) {
    match previous {
        Some(value) => unsafe { std::env::set_var(key, value) },
        None => unsafe { std::env::remove_var(key) },
    }
}

fn hub_skill(name: &str) {
    let dir = fs_paths::hub_skills_dir().join(name);
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("SKILL.md"), format!("# {name}\n")).unwrap();
}

fn exchange(requests: &[&str]) -> Vec<serde_json::Value> {
    let discover = stamp(
        r#"{"jsonrpc":"2.0","id":1,"method":"server/discover","params":{}}"#,
        &serde_json::json!({}),
    );
    let mut lines = vec![discover];
    lines.extend(
        requests
            .iter()
            .map(|line| stamp(line, &serde_json::json!({}))),
    );
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("runtime")
        .block_on(async {
            let (client, server) = tokio::io::duplex(64 * 1024);
            let server_task = tokio::spawn(super::super::stdio::serve_transport(server));
            let (client_read, mut client_write) = tokio::io::split(client);
            for line in &lines {
                client_write.write_all(line.as_bytes()).await?;
                client_write.write_all(b"\n").await?;
            }
            client_write.shutdown().await?;
            let mut incoming = BufReader::new(client_read).lines();
            let mut frames = Vec::new();
            while let Some(line) = incoming.next_line().await? {
                if !line.is_empty() {
                    assert!(line.starts_with('{'), "stdout frame is not JSON: {line}");
                    frames.push(serde_json::from_str(&line)?);
                }
            }
            server_task.await.expect("server task").expect("server");
            Ok::<_, anyhow::Error>(frames)
        })
        .expect("stdio session")
}

fn stamp(request: &str, capabilities: &serde_json::Value) -> String {
    let mut value: serde_json::Value = serde_json::from_str(request).expect("request json");
    let params = value
        .as_object_mut()
        .expect("request")
        .entry("params")
        .or_insert_with(|| serde_json::json!({}));
    if !params.is_object() {
        *params = serde_json::json!({});
    }
    params.as_object_mut().expect("params").insert(
        "_meta".to_string(),
        serde_json::json!({
            "io.modelcontextprotocol/protocolVersion": "2026-07-28",
            "io.modelcontextprotocol/clientInfo": {"name": "probe", "version": "0"},
            "io.modelcontextprotocol/clientCapabilities": capabilities,
        }),
    );
    value.to_string()
}

fn tool_names(list: &serde_json::Value) -> Vec<String> {
    list["result"]["tools"]
        .as_array()
        .expect("tools")
        .iter()
        .map(|tool| tool["name"].as_str().expect("name").to_string())
        .collect()
}

fn tool_by_name<'a>(list: &'a serde_json::Value, name: &str) -> &'a serde_json::Value {
    list["result"]["tools"]
        .as_array()
        .expect("tools")
        .iter()
        .find(|tool| tool["name"] == name)
        .unwrap_or_else(|| panic!("missing tool {name}"))
}

#[test]
fn tools_list_is_exactly_the_three_project_skill_tools() {
    let _env = EnvGuard::new("list");
    let frames = exchange(&[r#"{"jsonrpc":"2.0","id":2,"method":"tools/list","params":{}}"#]);
    assert!(frames[0]["result"]["capabilities"].get("roots").is_none());
    let mut names = tool_names(&frames[1]);
    names.sort();
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
fn recommend_tool_rejects_user_confirmed() {
    let env = EnvGuard::new("reject-confirm");
    let project = env.project("project");
    let request = format!(
        r#"{{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{{"name":"recommend_project_skills","arguments":{{"project_path":{},"query":"demo","catalog_scope":"installed","user_confirmed":true}}}}}}"#,
        serde_json::to_string(&project.to_string_lossy()).unwrap()
    );
    let frames = exchange(&[&request]);
    let response = &frames[1]["result"];
    assert_eq!(response["isError"], true, "{response}");
    assert!(
        response["content"][0]["text"]
            .as_str()
            .unwrap_or_default()
            .contains("user_confirmed"),
        "{response}"
    );
    assert!(response.get("structuredContent").is_none(), "{response}");
    assert!(!fs_paths::projects_manifest_path().exists());
    assert!(!project.join(".agents").exists());
}

#[test]
fn apply_tool_has_no_approval_field() {
    let _env = EnvGuard::new("no-approval-field");
    let frames = exchange(&[
        r#"{"jsonrpc":"2.0","id":2,"method":"tools/list","params":{}}"#,
        r#"{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"apply_project_skills","arguments":{"plan_id":"abcd","idempotency_key":"key-1","plan_hash":"deadbeef","user_confirmed":true}}}"#,
    ]);
    let apply = tool_by_name(&frames[1], "apply_project_skills");
    let properties = apply["inputSchema"]["properties"]
        .as_object()
        .expect("properties");
    assert!(properties.contains_key("plan_id"));
    assert!(properties.contains_key("idempotency_key"));
    for forbidden in ["user_confirmed", "plan_hash", "approval", "source"] {
        assert!(
            !properties.contains_key(forbidden),
            "{forbidden} in {apply}"
        );
    }
    assert_eq!(apply["inputSchema"]["additionalProperties"], false);
    let response = &frames[2]["result"];
    assert_eq!(response["isError"], true, "{response}");
    assert!(
        response["content"][0]["text"]
            .as_str()
            .unwrap_or_default()
            .contains("plan_hash"),
        "{response}"
    );
    assert!(response.get("structuredContent").is_none(), "{response}");
    assert!(!fs_paths::projects_manifest_path().exists());
}

#[test]
fn server_capabilities_omit_roots() {
    let _env = EnvGuard::new("roots");
    let frames = exchange(&[]);
    let capabilities = &frames[0]["result"]["capabilities"];
    assert!(capabilities.get("roots").is_none(), "{capabilities}");
    assert!(capabilities.get("resources").is_none(), "{capabilities}");
    assert!(capabilities.get("tools").is_some(), "{capabilities}");
}

#[test]
fn unapproved_apply_does_not_mutate_the_project() {
    let env = EnvGuard::new("unapproved");
    hub_skill("demo");
    let project = env.project("project");
    let path = serde_json::to_string(&project.to_string_lossy()).unwrap();
    let plain = format!(
        r#"{{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{{"name":"recommend_project_skills","arguments":{{"project_path":{path},"query":"demo","catalog_scope":"installed"}}}}}}"#
    );
    let selected = format!(
        r#"{{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{{"name":"recommend_project_skills","arguments":{{"project_path":{path},"query":"demo","catalog_scope":"installed","selection":{{"agent_id":"codex","skill_names":["demo"]}}}}}}}}"#
    );
    let frames = exchange(&[&plain, &selected]);
    let plain_body = &frames[1]["result"]["structuredContent"];
    assert!(plain_body["plan"].is_null(), "{plain_body}");
    assert!(!fs_paths::projects_manifest_path().exists());

    let plan = &frames[2]["result"]["structuredContent"]["plan"];
    let plan_id = plan["plan_id"].as_str().expect("plan id");
    let plan_hash = plan["plan_hash"].as_str().expect("plan hash");
    assert!(!plan_id.is_empty());
    assert!(!plan_hash.is_empty());
    assert!(!fs_paths::projects_manifest_path().exists());
    assert!(!project.join(".agents").exists());

    let apply = format!(
        r#"{{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{{"name":"apply_project_skills","arguments":{{"plan_id":{},"idempotency_key":"apply-1"}}}}}}"#,
        serde_json::to_string(plan_id).unwrap()
    );
    let applied = exchange(&[&apply]);
    let body = &applied[1]["result"];
    assert_eq!(body["isError"], false, "{body}");
    assert_eq!(body["structuredContent"]["outcome"], "approval_required");
    assert!(body["structuredContent"]["receipt"].is_null());
    assert!(
        body["content"][0]["text"]
            .as_str()
            .unwrap_or_default()
            .contains("Approval required"),
        "{body}"
    );
    assert!(!fs_paths::projects_manifest_path().exists());
    assert!(!project.join(".agents").exists());
}
