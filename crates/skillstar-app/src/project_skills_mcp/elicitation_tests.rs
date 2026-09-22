use std::ffi::OsString;
use std::fs;
use std::path::PathBuf;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

use skillstar_core::infra::fs_ops::is_link;
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
        let root = std::env::temp_dir().join(format!("skillstar-elicit-{label}-{nanos}"));
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

    fn project(&self) -> PathBuf {
        let path = self.root.join("project");
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

enum Elicit {
    Forbid,
    Decline,
    Accept(serde_json::Value),
}

struct Reply {
    tool: serde_json::Value,
    elicitations: Vec<serde_json::Value>,
}

fn call_tool(capabilities: &str, request: &str, elicit: &Elicit) -> Reply {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("runtime")
        .block_on(async {
            let (client, server) = tokio::io::duplex(256 * 1024);
            let server_task = tokio::spawn(super::super::stdio::serve_transport(server));
            let (read, mut write) = tokio::io::split(client);
            let mut lines = BufReader::new(read).lines();
            let init = format!(
                r#"{{"jsonrpc":"2.0","id":1,"method":"initialize","params":{{"protocolVersion":"2026-07-28","capabilities":{capabilities},"clientInfo":{{"name":"probe","version":"0"}}}}}}"#
            );
            write.write_all(init.as_bytes()).await?;
            write.write_all(b"\n").await?;
            let mut elicitations = Vec::new();
            let init_result = next_response(&mut lines, &mut write, 1, elicit, &mut elicitations).await?;
            assert!(init_result.get("result").is_some(), "{init_result}");
            write.write_all(request.as_bytes()).await?;
            write.write_all(b"\n").await?;
            let tool = next_response(&mut lines, &mut write, 2, elicit, &mut elicitations).await?;
            write.shutdown().await?;
            server_task.await.expect("server task").expect("server");
            Ok::<_, anyhow::Error>(Reply { tool, elicitations })
        })
        .expect("session")
}

async fn next_response<R>(
    lines: &mut tokio::io::Lines<R>,
    write: &mut (impl AsyncWriteExt + Unpin),
    expected_id: i64,
    elicit: &Elicit,
    elicitations: &mut Vec<serde_json::Value>,
) -> anyhow::Result<serde_json::Value>
where
    R: tokio::io::AsyncBufRead + Unpin,
{
    loop {
        let line = tokio::time::timeout(Duration::from_secs(5), lines.next_line())
            .await
            .expect("timed out waiting for the server")
            .expect("read")
            .expect("server closed the stream");
        if line.is_empty() {
            continue;
        }
        let frame: serde_json::Value = serde_json::from_str(&line)?;
        if frame["method"] == "elicitation/create" {
            elicitations.push(frame.clone());
            let result = match elicit {
                Elicit::Forbid => panic!("unexpected elicitation: {frame}"),
                Elicit::Decline => serde_json::json!({"action": "decline"}),
                Elicit::Accept(content) => {
                    serde_json::json!({"action": "accept", "content": content})
                }
            };
            let response = serde_json::json!({
                "jsonrpc": "2.0",
                "id": frame["id"],
                "result": result,
            });
            let bytes = serde_json::to_vec(&response)?;
            write.write_all(&bytes).await?;
            write.write_all(b"\n").await?;
            continue;
        }
        if frame.get("method").is_none() && frame["id"].as_i64() == Some(expected_id) {
            return Ok(frame);
        }
    }
}

fn recommend(path: &str) -> serde_json::Value {
    let request = format!(
        r#"{{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{{"name":"recommend_project_skills","arguments":{{"project_path":{},"query":"demo","catalog_scope":"installed","selection":{{"agent_id":"codex","skill_names":["demo"]}}}}}}}}"#,
        serde_json::to_string(path).unwrap()
    );
    let reply = call_tool("{}", &request, &Elicit::Forbid);
    assert!(reply.elicitations.is_empty());
    reply.tool["result"]["structuredContent"]["plan"].clone()
}

fn apply_request(plan_id: &str, extra: &str) -> String {
    format!(
        r#"{{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{{"name":"apply_project_skills","arguments":{{"plan_id":{},"idempotency_key":"apply-1"{extra}}}}}}}"#,
        serde_json::to_string(plan_id).unwrap()
    )
}

fn acceptance(plan: &serde_json::Value) -> serde_json::Value {
    let changes = plan["skills"]
        .as_array()
        .unwrap()
        .iter()
        .map(|skill| {
            format!(
                "{} {}",
                skill["action"].as_str().unwrap(),
                skill["skill_path"].as_str().unwrap()
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    let affected = plan["affected_agents"]
        .as_array()
        .unwrap()
        .iter()
        .map(|agent| agent.as_str().unwrap())
        .collect::<Vec<_>>()
        .join(", ");
    serde_json::json!({
        "plan_hash": plan["plan_hash"],
        "root": plan["root"],
        "will_register": plan["will_register"],
        "owner_id": plan["owner_id"],
        "affected_agents": affected,
        "changes": changes,
    })
}

fn prepared(label: &str) -> (EnvGuard, PathBuf, serde_json::Value) {
    let env = EnvGuard::new(label);
    hub_skill("demo");
    let project = env.project();
    let plan = recommend(project.to_str().unwrap());
    assert!(plan["plan_id"].is_string(), "{plan}");
    (env, project, plan)
}

const FORM: &str = r#"{"elicitation":{"form":{}}}"#;

#[test]
fn elicitation_decline_writes_nothing() {
    let (_env, project, plan) = prepared("decline");
    let reply = call_tool(
        FORM,
        &apply_request(plan["plan_id"].as_str().unwrap(), ""),
        &Elicit::Decline,
    );
    assert_eq!(reply.elicitations.len(), 1, "{:?}", reply.elicitations);
    let message = reply.elicitations[0]["params"]["message"].as_str().unwrap();
    assert!(
        message.contains(plan["plan_hash"].as_str().unwrap()),
        "{message}"
    );
    assert!(
        message.contains(".agents/skills/demo/SKILL.md"),
        "{message}"
    );
    assert_eq!(reply.elicitations[0]["params"]["mode"], "form");
    assert_eq!(
        reply.tool["result"]["structuredContent"]["outcome"],
        "declined"
    );
    assert!(!project.join(".agents").exists());
    assert!(!fs_paths::projects_manifest_path().exists());
    assert!(
        !fs_paths::state_dir()
            .join("project-skill-approvals")
            .exists()
    );
}

#[test]
fn elicitation_accept_records_elicitation_source_then_applies() {
    let (_env, project, plan) = prepared("accept");
    let reply = call_tool(
        FORM,
        &apply_request(plan["plan_id"].as_str().unwrap(), ""),
        &Elicit::Accept(acceptance(&plan)),
    );
    assert_eq!(reply.elicitations.len(), 1);
    let body = &reply.tool["result"];
    assert_eq!(body["isError"], false, "{body}");
    assert_eq!(body["structuredContent"]["outcome"], "applied");
    assert!(is_link(&project.join(".agents/skills/demo")));
    let approval: serde_json::Value = serde_json::from_slice(
        &fs::read(
            fs_paths::state_dir()
                .join("project-skill-approvals")
                .join(format!("{}.json", plan["plan_id"].as_str().unwrap())),
        )
        .unwrap(),
    )
    .unwrap();
    assert_eq!(approval["source"], "elicitation");
    assert_eq!(approval["plan_hash"], plan["plan_hash"]);
}

#[test]
fn preexisting_skillstar_approval_does_not_skip_elicitation() {
    let (_env, project, plan) = prepared("preexisting");
    let plan_id = plan["plan_id"].as_str().unwrap();
    crate::project_skills_mcp::approval::record_from_skillstar(
        plan_id,
        plan["plan_hash"].as_str().unwrap(),
    )
    .unwrap();
    let reply = call_tool(FORM, &apply_request(plan_id, ""), &Elicit::Decline);
    assert_eq!(reply.elicitations.len(), 1, "{}", reply.tool);
    assert!(!project.join(".agents").exists());
    let accepted = call_tool(
        FORM,
        &apply_request(plan_id, ""),
        &Elicit::Accept(acceptance(&plan)),
    );
    assert_eq!(accepted.elicitations.len(), 1, "{}", accepted.tool);
    assert_eq!(
        accepted.tool["result"]["isError"], true,
        "{}",
        accepted.tool
    );
    assert!(!project.join(".agents").exists());
    let approval: serde_json::Value = serde_json::from_slice(
        &fs::read(
            fs_paths::state_dir()
                .join("project-skill-approvals")
                .join(format!("{plan_id}.json")),
        )
        .unwrap(),
    )
    .unwrap();
    assert_eq!(approval["source"], "skillstar");
}

#[test]
fn client_without_elicitation_does_not_receive_an_elicit_request() {
    let (_env, project, plan) = prepared("no-form");
    let reply = call_tool(
        "{}",
        &apply_request(plan["plan_id"].as_str().unwrap(), ""),
        &Elicit::Forbid,
    );
    assert!(reply.elicitations.is_empty(), "{:?}", reply.elicitations);
    assert_eq!(
        reply.tool["result"]["structuredContent"]["outcome"],
        "approval_required"
    );
    assert!(!project.join(".agents").exists());
    assert!(!fs_paths::projects_manifest_path().exists());
}

#[test]
fn tool_arguments_cannot_supply_the_accept() {
    let (_env, project, plan) = prepared("args");
    let reply = call_tool(
        FORM,
        &apply_request(plan["plan_id"].as_str().unwrap(), r#","accept":true"#),
        &Elicit::Forbid,
    );
    assert!(reply.elicitations.is_empty(), "{:?}", reply.elicitations);
    let body = &reply.tool["result"];
    assert_eq!(body["isError"], true, "{body}");
    assert!(
        body["content"][0]["text"]
            .as_str()
            .unwrap_or_default()
            .contains("accept"),
        "{body}"
    );
    assert!(!project.join(".agents").exists());
}
