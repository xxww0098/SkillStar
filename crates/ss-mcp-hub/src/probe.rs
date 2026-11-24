use crate::env_refs::expand_value_for_probe;
use crate::error::{McpHubError, McpHubResult};
use crate::model::{CheckResult, DiscoveredTool, McpServer, Transport};
use serde_json::{Value, json};
use ss_core::infra::http_client::probe_http_client_for_url;
use ss_core::infra::path_env::enriched_path;
use std::collections::{BTreeMap, BTreeSet};
use std::process::Stdio;
use std::time::{Duration, Instant};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, Command};

const PROBE_TIMEOUT: Duration = Duration::from_secs(12);
const PROTOCOL_VERSION: &str = "2024-11-05";
const MAX_TOOL_PAGES: usize = 16;

/// Check connectivity and enumerate tools (`initialize` -> `notifications/initialized` -> paginated `tools/list`)
/// for either a stdio command or an HTTP / SSE remote MCP server.
pub async fn check_server(server: &McpServer) -> CheckResult {
    let started = Instant::now();
    let res = match server.transport {
        Transport::Stdio => probe_stdio(server).await,
        Transport::Http | Transport::Sse => probe_remote(server).await,
    };
    let duration_ms = started.elapsed().as_millis() as u64;
    match res {
        Ok(mut ok) => {
            ok.duration_ms = duration_ms;
            ok
        }
        Err(err) => CheckResult {
            ok: false,
            server_name: String::new(),
            server_version: String::new(),
            tools: Vec::new(),
            error: Some(err.to_string()),
            duration_ms,
        },
    }
}

async fn probe_stdio(server: &McpServer) -> McpHubResult<CheckResult> {
    let expanded_cmd = expand_value_for_probe(&server.command, &server.env);
    if expanded_cmd.trim().is_empty() {
        return Err(McpHubError::Probe("missing command".to_string()));
    }
    let enriched = enriched_path();
    let effective_path = server
        .env
        .get("PATH")
        .map(|v| expand_value_for_probe(v, &server.env))
        .filter(|v| !v.is_empty())
        .unwrap_or_else(|| enriched.clone());
    let resolved_cmd = resolve_executable_for_probe(&expanded_cmd, &effective_path);
    let expanded_args: Vec<String> = server
        .args
        .iter()
        .map(|a| expand_value_for_probe(a, &server.env))
        .collect();

    let mut cmd = Command::new(&resolved_cmd);
    cmd.args(&expanded_args)
        .env("PATH", &enriched)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    for (k, v) in &server.env {
        cmd.env(k, expand_value_for_probe(v, &server.env));
    }

    let mut child: Child = cmd
        .spawn()
        .map_err(|e| McpHubError::Probe(format!("failed spawning '{resolved_cmd}': {e}")))?;
    let mut stdin = child
        .stdin
        .take()
        .ok_or_else(|| McpHubError::Probe("failed capturing child stdin".to_string()))?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| McpHubError::Probe("failed capturing child stdout".to_string()))?;
    let mut reader = BufReader::new(stdout).lines();

    let fut = async {
        // 1. initialize
        let init_req = json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize",
            "params": {
                "protocolVersion": PROTOCOL_VERSION,
                "capabilities": {},
                "clientInfo": { "name": "skillstar", "version": "1.0.0" }
            }
        });
        write_json_line(&mut stdin, &init_req).await?;
        let init_resp = read_jsonrpc_response(&mut reader, 1).await?;
        let (server_name, server_version) = extract_server_info(&init_resp);

        // 2. notifications/initialized
        let notif = json!({
            "jsonrpc": "2.0",
            "method": "notifications/initialized"
        });
        write_json_line(&mut stdin, &notif).await?;

        // 3. paginated tools/list
        let mut all_tools = Vec::new();
        let mut cursor: Option<String> = None;
        let mut req_id = 2u64;
        for _ in 0..8 {
            let mut params = serde_json::Map::new();
            if let Some(c) = &cursor {
                params.insert("cursor".into(), Value::String(c.clone()));
            }
            let req = json!({
                "jsonrpc": "2.0",
                "id": req_id,
                "method": "tools/list",
                "params": params
            });
            write_json_line(&mut stdin, &req).await?;
            let resp = read_jsonrpc_response(&mut reader, req_id).await;
            req_id += 1;
            match resp {
                Ok(val) => {
                    append_tools_from_result(&val, &mut all_tools);
                    cursor = val
                        .get("result")
                        .and_then(|r| r.get("nextCursor"))
                        .and_then(|c| c.as_str())
                        .filter(|s| !s.is_empty())
                        .map(str::to_string);
                    if cursor.is_none() {
                        break;
                    }
                }
                Err(_) => break,
            }
        }

        Ok::<CheckResult, McpHubError>(CheckResult {
            ok: true,
            server_name,
            server_version,
            tools: all_tools,
            error: None,
            duration_ms: 0,
        })
    };

    let result = tokio::time::timeout(PROBE_TIMEOUT, fut)
        .await
        .map_err(|_| McpHubError::Probe("timed out waiting for MCP server response".to_string()))?;
    let _ = child.kill().await;
    result
}

async fn write_json_line(stdin: &mut tokio::process::ChildStdin, val: &Value) -> McpHubResult<()> {
    let mut line = serde_json::to_string(val).map_err(|e| McpHubError::Probe(e.to_string()))?;
    line.push('\n');
    stdin
        .write_all(line.as_bytes())
        .await
        .map_err(|e| McpHubError::Probe(format!("failed writing stdin: {e}")))?;
    stdin
        .flush()
        .await
        .map_err(|e| McpHubError::Probe(format!("failed flushing stdin: {e}")))
}

async fn read_jsonrpc_response(
    lines: &mut tokio::io::Lines<BufReader<tokio::process::ChildStdout>>,
    expected_id: u64,
) -> McpHubResult<Value> {
    while let Some(line) = lines
        .next_line()
        .await
        .map_err(|e| McpHubError::Probe(format!("stdout read error: {e}")))?
    {
        let trimmed = line.trim();
        if !trimmed.starts_with('{') {
            continue;
        }
        let Ok(val) = serde_json::from_str::<Value>(trimmed) else {
            continue;
        };
        if val.get("id").and_then(|v| v.as_u64()) != Some(expected_id) {
            continue;
        }
        if let Some(err) = val.get("error") {
            let msg = err
                .get("message")
                .and_then(|m| m.as_str())
                .unwrap_or("MCP JSON-RPC error");
            return Err(McpHubError::Probe(msg.to_string()));
        }
        return Ok(val);
    }
    Err(McpHubError::Probe(
        "MCP server closed stdout unexpectedly".to_string(),
    ))
}

async fn probe_remote(server: &McpServer) -> McpHubResult<CheckResult> {
    let url = expand_value_for_probe(&server.url, &server.env);
    let client = probe_http_client_for_url(&url, PROBE_TIMEOUT).map_err(McpHubError::Other)?;
    probe_remote_with_client(server, &client).await
}

async fn probe_remote_with_client(
    server: &McpServer,
    client: &reqwest::Client,
) -> McpHubResult<CheckResult> {
    let url = expand_value_for_probe(&server.url, &server.env);
    let mut headers = BTreeMap::new();
    for (k, v) in &server.headers {
        headers.insert(k.clone(), expand_value_for_probe(v, &server.env));
    }
    let fut = async {
        match server.transport {
            Transport::Sse => match probe_legacy_sse(client, &url, &headers).await? {
                Some(res) => Ok(res),
                None => probe_streamable_http(client, &url, &headers)
                    .await?
                    .ok_or_else(|| {
                        McpHubError::Probe(format!("HTTP 405 Method Not Allowed from {url}"))
                    }),
            },
            Transport::Http | Transport::Stdio => {
                match probe_streamable_http(client, &url, &headers).await? {
                    Some(res) => Ok(res),
                    None => probe_legacy_sse(client, &url, &headers)
                        .await?
                        .ok_or_else(|| {
                            McpHubError::Probe(format!("HTTP 405 Method Not Allowed from {url}"))
                        }),
                }
            }
        }
    };
    tokio::time::timeout(PROBE_TIMEOUT, fut)
        .await
        .map_err(|_| McpHubError::Probe("timed out waiting for remote MCP server".to_string()))?
}

fn initialize_payload() -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "initialize",
        "params": {
            "protocolVersion": PROTOCOL_VERSION,
            "capabilities": {},
            "clientInfo": { "name": "skillstar", "version": "1.0.0" }
        }
    })
}

fn check_http_status(status: reqwest::StatusCode, url: &str) -> McpHubResult<()> {
    if status.as_u16() == 401 || status.as_u16() == 403 {
        return Err(McpHubError::Probe(format!(
            "HTTP {status}: authentication / API key required"
        )));
    }
    if !status.is_success() {
        return Err(McpHubError::Probe(format!("HTTP {status} from {url}")));
    }
    Ok(())
}

async fn probe_streamable_http(
    client: &reqwest::Client,
    url: &str,
    headers: &BTreeMap<String, String>,
) -> McpHubResult<Option<CheckResult>> {
    let init_body = initialize_payload();
    let mut req = client
        .post(url)
        .header("Content-Type", "application/json")
        .header("Accept", "application/json, text/event-stream");
    for (k, v) in headers {
        req = req.header(k.as_str(), v.as_str());
    }
    let resp = req
        .json(&init_body)
        .send()
        .await
        .map_err(|e| McpHubError::Probe(format!("HTTP request failed: {e}")))?;

    let status = resp.status();
    if status.as_u16() == 405 {
        return Ok(None);
    }
    check_http_status(status, url)?;

    let session_id = resp
        .headers()
        .get("mcp-session-id")
        .and_then(|h| h.to_str().ok())
        .map(str::to_string);
    let body_text = resp.text().await.map_err(|e| McpHubError::Probe(e.to_string()))?;
    let init_val = parse_http_or_sse_jsonrpc(&body_text, 1)?;
    let (server_name, server_version) = extract_server_info(&init_val);

    let notif_body = json!({ "jsonrpc": "2.0", "method": "notifications/initialized" });
    let mut nreq = client
        .post(url)
        .header("Content-Type", "application/json")
        .header("Accept", "application/json, text/event-stream");
    if let Some(sid) = &session_id {
        nreq = nreq.header("mcp-session-id", sid.as_str());
    }
    for (k, v) in headers {
        nreq = nreq.header(k.as_str(), v.as_str());
    }
    let _ = nreq.json(&notif_body).send().await;

    let mut all_tools = Vec::new();
    let mut cursor: Option<String> = None;
    let mut seen_cursors: BTreeSet<String> = BTreeSet::new();
    for req_id in (2u64..).take(MAX_TOOL_PAGES) {
        let mut params = serde_json::Map::new();
        if let Some(c) = &cursor {
            params.insert("cursor".into(), Value::String(c.clone()));
        }
        let tools_body = json!({
            "jsonrpc": "2.0",
            "id": req_id,
            "method": "tools/list",
            "params": params
        });
        let mut treq = client
            .post(url)
            .header("Content-Type", "application/json")
            .header("Accept", "application/json, text/event-stream");
        if let Some(sid) = &session_id {
            treq = treq.header("mcp-session-id", sid.as_str());
        }
        for (k, v) in headers {
            treq = treq.header(k.as_str(), v.as_str());
        }
        let tresp = treq
            .json(&tools_body)
            .send()
            .await
            .map_err(|e| McpHubError::Probe(format!("tools/list request failed: {e}")))?;
        check_http_status(tresp.status(), url)?;
        let tbody = tresp
            .text()
            .await
            .map_err(|e| McpHubError::Probe(format!("tools/list read failed: {e}")))?;
        let tval = parse_http_or_sse_jsonrpc(&tbody, req_id)?;
        append_tools_from_result(&tval, &mut all_tools);
        cursor = next_cursor_from_result(&tval, &mut seen_cursors);
        if cursor.is_none() {
            break;
        }
    }

    Ok(Some(CheckResult {
        ok: true,
        server_name,
        server_version,
        tools: all_tools,
        error: None,
        duration_ms: 0,
    }))
}

struct SseStreamReader {
    resp: reqwest::Response,
    buf: String,
    eof: bool,
}

impl SseStreamReader {
    fn new(resp: reqwest::Response) -> Self {
        Self {
            resp,
            buf: String::new(),
            eof: false,
        }
    }

    async fn next_event(&mut self) -> McpHubResult<Option<(String, String)>> {
        loop {
            if let Some(event) = self.pop_buffered_event() {
                return Ok(Some(event));
            }
            if self.eof {
                return Ok(None);
            }
            match self
                .resp
                .chunk()
                .await
                .map_err(|e| McpHubError::Probe(format!("SSE stream read failed: {e}")))?
            {
                Some(chunk) => {
                    let text = String::from_utf8_lossy(&chunk);
                    self.buf.push_str(&text.replace("\r\n", "\n"));
                }
                None => {
                    self.eof = true;
                    if !self.buf.trim().is_empty() {
                        self.buf.push_str("\n\n");
                        continue;
                    }
                    return Ok(None);
                }
            }
        }
    }

    fn pop_buffered_event(&mut self) -> Option<(String, String)> {
        while let Some(sep) = self.buf.find("\n\n") {
            let block = self.buf[..sep].to_string();
            self.buf.drain(..sep + 2);
            if let Some(parsed) = parse_sse_event_block(&block) {
                return Some(parsed);
            }
        }
        None
    }
}

fn parse_sse_event_block(block: &str) -> Option<(String, String)> {
    let mut event_name = "message".to_string();
    let mut data_lines = Vec::new();
    for line in block.lines() {
        if line.starts_with(':') || line.trim().is_empty() {
            continue;
        }
        if let Some(ev) = line.strip_prefix("event:") {
            event_name = ev.trim().to_string();
        } else if let Some(d) = line.strip_prefix("data:") {
            data_lines.push(d.strip_prefix(' ').unwrap_or(d).to_string());
        }
    }
    if data_lines.is_empty() {
        None
    } else {
        Some((event_name, data_lines.join("\n")))
    }
}

async fn probe_legacy_sse(
    client: &reqwest::Client,
    url: &str,
    headers: &BTreeMap<String, String>,
) -> McpHubResult<Option<CheckResult>> {
    let mut greq = client.get(url).header("Accept", "text/event-stream");
    for (k, v) in headers {
        greq = greq.header(k.as_str(), v.as_str());
    }
    let resp = greq
        .send()
        .await
        .map_err(|e| McpHubError::Probe(format!("SSE connect failed: {e}")))?;
    let status = resp.status();
    if status.as_u16() == 405 {
        return Ok(None);
    }
    check_http_status(status, url)?;

    let base_url = url::Url::parse(url)
        .map_err(|e| McpHubError::Probe(format!("invalid SSE URL '{url}': {e}")))?;
    let mut sse = SseStreamReader::new(resp);
    let mut post_url: Option<String> = None;
    while let Some((ev, data)) = sse.next_event().await? {
        if ev == "endpoint" {
            let resolved = base_url
                .join(data.trim())
                .map_err(|e| McpHubError::Probe(format!("invalid SSE endpoint '{data}': {e}")))?;
            if !matches!(resolved.scheme(), "http" | "https") {
                return Err(McpHubError::Probe(format!(
                    "unsupported SSE endpoint scheme '{}'",
                    resolved.scheme()
                )));
            }
            post_url = Some(resolved.to_string());
            break;
        }
    }
    let Some(endpoint_url) = post_url else {
        return Err(McpHubError::Probe(
            "SSE stream closed before emitting endpoint event".to_string(),
        ));
    };

    let init_val = legacy_sse_rpc(
        client,
        &endpoint_url,
        headers,
        &mut sse,
        &initialize_payload(),
        1,
    )
    .await?;
    let (server_name, server_version) = extract_server_info(&init_val);

    let notif = json!({ "jsonrpc": "2.0", "method": "notifications/initialized" });
    let mut nreq = client
        .post(&endpoint_url)
        .header("Content-Type", "application/json");
    for (k, v) in headers {
        nreq = nreq.header(k.as_str(), v.as_str());
    }
    let _ = nreq.json(&notif).send().await;

    let mut all_tools = Vec::new();
    let mut cursor: Option<String> = None;
    let mut seen_cursors: BTreeSet<String> = BTreeSet::new();
    for req_id in (2u64..).take(MAX_TOOL_PAGES) {
        let mut params = serde_json::Map::new();
        if let Some(c) = &cursor {
            params.insert("cursor".into(), Value::String(c.clone()));
        }
        let body = json!({
            "jsonrpc": "2.0",
            "id": req_id,
            "method": "tools/list",
            "params": params
        });
        let tval = legacy_sse_rpc(client, &endpoint_url, headers, &mut sse, &body, req_id).await?;
        append_tools_from_result(&tval, &mut all_tools);
        cursor = next_cursor_from_result(&tval, &mut seen_cursors);
        if cursor.is_none() {
            break;
        }
    }

    Ok(Some(CheckResult {
        ok: true,
        server_name,
        server_version,
        tools: all_tools,
        error: None,
        duration_ms: 0,
    }))
}

async fn legacy_sse_rpc(
    client: &reqwest::Client,
    endpoint_url: &str,
    headers: &BTreeMap<String, String>,
    sse: &mut SseStreamReader,
    body: &Value,
    expected_id: u64,
) -> McpHubResult<Value> {
    let mut req = client
        .post(endpoint_url)
        .header("Content-Type", "application/json")
        .header("Accept", "application/json, text/event-stream");
    for (k, v) in headers {
        req = req.header(k.as_str(), v.as_str());
    }
    let resp = req
        .json(body)
        .send()
        .await
        .map_err(|e| McpHubError::Probe(format!("SSE POST failed: {e}")))?;
    check_http_status(resp.status(), endpoint_url)?;
    let text = resp
        .text()
        .await
        .map_err(|e| McpHubError::Probe(e.to_string()))?;
    if !text.trim().is_empty() {
        return parse_http_or_sse_jsonrpc(&text, expected_id);
    }

    while let Some((ev, data)) = sse.next_event().await? {
        if ev != "message" {
            continue;
        }
        let payload = data.trim();
        if !payload.starts_with('{') {
            continue;
        }
        if let Ok(val) = serde_json::from_str::<Value>(payload)
            && val.get("id").and_then(|v| v.as_u64()) == Some(expected_id)
        {
            return check_jsonrpc_error(val);
        }
    }
    Err(McpHubError::Probe(format!(
        "SSE stream closed before response id {expected_id}"
    )))
}

fn next_cursor_from_result(resp: &Value, seen: &mut BTreeSet<String>) -> Option<String> {
    resp.get("result")
        .and_then(|r| r.get("nextCursor"))
        .and_then(|c| c.as_str())
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .filter(|c| seen.insert(c.clone()))
}

fn check_jsonrpc_error(val: Value) -> McpHubResult<Value> {
    if let Some(err) = val.get("error").filter(|v| !v.is_null()) {
        let msg = err
            .get("message")
            .and_then(|m| m.as_str())
            .unwrap_or("MCP JSON-RPC error");
        return Err(McpHubError::Probe(msg.to_string()));
    }
    Ok(val)
}

fn parse_http_or_sse_jsonrpc(body: &str, expected_id: u64) -> McpHubResult<Value> {
    let trimmed = body.trim();
    if trimmed.starts_with('{') {
        let val: Value =
            serde_json::from_str(trimmed).map_err(|e| McpHubError::Probe(e.to_string()))?;
        return check_jsonrpc_error(val);
    }
    let normalized = body.replace("\r\n", "\n");
    let mut last_uncorrelated_error: Option<McpHubError> = None;
    for block in normalized.split("\n\n") {
        let Some((_, payload)) = parse_sse_event_block(block) else {
            continue;
        };
        let p = payload.trim();
        if !p.starts_with('{') {
            continue;
        }
        if let Ok(val) = serde_json::from_str::<Value>(p) {
            let id_matches = val.get("id").and_then(|v| v.as_u64()) == Some(expected_id);
            if id_matches {
                return check_jsonrpc_error(val);
            }
            if val.get("id").is_none_or(|v| v.is_null())
                && let Err(err) = check_jsonrpc_error(val)
            {
                last_uncorrelated_error = Some(err);
            }
        }
    }
    if let Some(err) = last_uncorrelated_error {
        return Err(err);
    }
    Err(McpHubError::Probe(
        "response did not contain a valid JSON-RPC payload".to_string(),
    ))
}

fn extract_server_info(resp: &Value) -> (String, String) {
    let info = resp.get("result").and_then(|r| r.get("serverInfo"));
    let name = info
        .and_then(|i| i.get("name"))
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let version = info
        .and_then(|i| i.get("version"))
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    (name, version)
}

fn append_tools_from_result(resp: &Value, dst: &mut Vec<DiscoveredTool>) {
    let Some(arr) = resp
        .get("result")
        .and_then(|r| r.get("tools"))
        .and_then(|t| t.as_array())
    else {
        return;
    };
    for item in arr {
        let Some(name) = item.get("name").and_then(|v| v.as_str()) else {
            continue;
        };
        let description = item
            .get("description")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        dst.push(DiscoveredTool {
            name: name.to_string(),
            description,
        });
    }
}

fn resolve_executable_for_probe(cmd: &str, path_env: &str) -> String {
    if cmd.contains('/') || cmd.contains('\\') {
        return cmd.to_string();
    }
    let cwd = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
    if let Ok(found) = which::which_in(cmd, Some(path_env), &cwd) {
        return found.to_string_lossy().to_string();
    }
    cmd.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    #[test]
    fn parse_sse_checks_jsonrpc_error_field() {
        let sse_err = "event: message\ndata: {\"jsonrpc\":\"2.0\",\"id\":2,\"error\":{\"code\":-32601,\"message\":\"tools not enabled\"}}\n\n";
        let err = parse_http_or_sse_jsonrpc(sse_err, 2).unwrap_err();
        assert!(err.to_string().contains("tools not enabled"), "{err}");
    }

    #[tokio::test]
    async fn probe_remote_paginates_next_cursor_and_propagates_tools_list_failure() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let server_task = tokio::spawn(async move {
            loop {
                let Ok((mut stream, _)) = listener.accept().await else {
                    break;
                };
                tokio::spawn(async move {
                    let mut buf = vec![0u8; 8192];
                    let n = stream.read(&mut buf).await.unwrap_or(0);
                    let req = String::from_utf8_lossy(&buf[..n]);
                    let body = req.split("\r\n\r\n").nth(1).unwrap_or("");
                    let json_body: Value = serde_json::from_str(body).unwrap_or(Value::Null);
                    let method = json_body["method"].as_str().unwrap_or("");
                    let id = json_body["id"].as_u64().unwrap_or(0);
                    let (status, resp_body) = match method {
                        "initialize" => (
                            "200 OK",
                            json!({"jsonrpc":"2.0","id":id,"result":{"serverInfo":{"name":"http-srv","version":"2.0"}}})
                                .to_string(),
                        ),
                        "notifications/initialized" => ("202 Accepted", String::new()),
                        "tools/list" => {
                            let cursor = json_body["params"]["cursor"].as_str();
                            match cursor {
                                None => (
                                    "200 OK",
                                    json!({"jsonrpc":"2.0","id":id,"result":{"tools":[{"name":"t1","description":"first"}],"nextCursor":"p2"}}).to_string(),
                                ),
                                Some("p2") => (
                                    "200 OK",
                                    json!({"jsonrpc":"2.0","id":id,"result":{"tools":[{"name":"t2","description":"second"}]}}).to_string(),
                                ),
                                _ => ("500 Internal Server Error", "boom".to_string()),
                            }
                        }
                        _ => ("404 Not Found", String::new()),
                    };
                    let resp = format!(
                        "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                        resp_body.len(),
                        resp_body
                    );
                    let _ = stream.write_all(resp.as_bytes()).await;
                });
            }
        });

        let client = reqwest::Client::builder().no_proxy().build().unwrap();
        let srv = McpServer {
            name: "http-demo".into(),
            transport: Transport::Http,
            url: format!("http://{addr}/mcp"),
            ..Default::default()
        };
        let res = probe_remote_with_client(&srv, &client).await.unwrap();
        assert!(res.ok);
        assert_eq!(res.tools.len(), 2);
        assert_eq!(res.tools[0].name, "t1");
        assert_eq!(res.tools[1].name, "t2");

        // Now test a server returning 500 on tools/list -> must return Err (ok: false).
        let fail_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let fail_addr = fail_listener.local_addr().unwrap();
        let fail_task = tokio::spawn(async move {
            loop {
                let Ok((mut stream, _)) = fail_listener.accept().await else {
                    break;
                };
                tokio::spawn(async move {
                    let mut buf = vec![0u8; 8192];
                    let n = stream.read(&mut buf).await.unwrap_or(0);
                    let req = String::from_utf8_lossy(&buf[..n]);
                    let body = req.split("\r\n\r\n").nth(1).unwrap_or("");
                    let json_body: Value = serde_json::from_str(body).unwrap_or(Value::Null);
                    let method = json_body["method"].as_str().unwrap_or("");
                    let id = json_body["id"].as_u64().unwrap_or(0);
                    let (status, resp_body) = if method == "initialize" {
                        (
                            "200 OK",
                            json!({"jsonrpc":"2.0","id":id,"result":{"serverInfo":{"name":"broken","version":"1.0"}}})
                                .to_string(),
                        )
                    } else if method == "notifications/initialized" {
                        ("202 Accepted", String::new())
                    } else {
                        ("500 Internal Server Error", "tools list broken".to_string())
                    };
                    let resp = format!(
                        "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                        resp_body.len(),
                        resp_body
                    );
                    let _ = stream.write_all(resp.as_bytes()).await;
                });
            }
        });
        let fail_srv = McpServer {
            name: "http-fail".into(),
            transport: Transport::Http,
            url: format!("http://{fail_addr}/mcp"),
            ..Default::default()
        };
        let err = probe_remote_with_client(&fail_srv, &client).await.unwrap_err();
        assert!(err.to_string().contains("500"), "{err}");
        server_task.abort();
        fail_task.abort();
    }

    #[tokio::test]
    async fn probe_remote_supports_legacy_sse_get_endpoint_and_post() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<String>();

        let server_task = tokio::spawn(async move {
            loop {
                let Ok((mut stream, _)) = listener.accept().await else {
                    break;
                };
                let tx_clone = tx.clone();
                if req_is_sse_get(&mut stream, &tx_clone, &mut rx).await {
                    break;
                }
            }
        });

        // Wait — use a simple router that handles GET /sse on one connection and POST /messages on others.
        drop(server_task);
        // Rebind cleanly with shared tx:
        let listener2 = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr2 = listener2.local_addr().unwrap();
        let (tx2, rx2) = tokio::sync::mpsc::unbounded_channel::<String>();
        let rx_shared = std::sync::Arc::new(tokio::sync::Mutex::new(rx2));
        let task2 = tokio::spawn(async move {
            loop {
                let Ok((mut stream, _)) = listener2.accept().await else {
                    break;
                };
                let tx_c = tx2.clone();
                let rx_c = rx_shared.clone();
                tokio::spawn(async move {
                    let mut buf = vec![0u8; 8192];
                    let n = stream.read(&mut buf).await.unwrap_or(0);
                    let req = String::from_utf8_lossy(&buf[..n]);
                    if req.starts_with("GET /sse ") {
                        let head = "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nCache-Control: no-cache\r\nConnection: close\r\n\r\nevent: endpoint\ndata: /messages?sid=1\n\n";
                        let _ = stream.write_all(head.as_bytes()).await;
                        let _ = stream.flush().await;
                        let mut guard = rx_c.lock().await;
                        while let Some(ev) = guard.recv().await {
                            if stream.write_all(ev.as_bytes()).await.is_err() {
                                break;
                            }
                            let _ = stream.flush().await;
                        }
                    } else if req.starts_with("POST /messages") {
                        let body = req.split("\r\n\r\n").nth(1).unwrap_or("");
                        let json_body: Value = serde_json::from_str(body).unwrap_or(Value::Null);
                        let method = json_body["method"].as_str().unwrap_or("");
                        let id = json_body["id"].as_u64().unwrap_or(0);
                        match method {
                            "initialize" => {
                                let msg = json!({"jsonrpc":"2.0","id":id,"result":{"serverInfo":{"name":"legacy-sse","version":"0.9"}}});
                                let _ = tx_c.send(format!("event: message\ndata: {msg}\n\n"));
                            }
                            "tools/list" => {
                                let msg = json!({"jsonrpc":"2.0","id":id,"result":{"tools":[{"name":"sse_ping","description":"Legacy SSE tool"}]}});
                                let _ = tx_c.send(format!("event: message\ndata: {msg}\n\n"));
                            }
                            _ => {}
                        }
                        let resp = "HTTP/1.1 202 Accepted\r\nContent-Length: 0\r\nConnection: close\r\n\r\n";
                        let _ = stream.write_all(resp.as_bytes()).await;
                    }
                });
            }
        });

        let client = reqwest::Client::builder().no_proxy().build().unwrap();
        let srv = McpServer {
            name: "sse-demo".into(),
            transport: Transport::Sse,
            url: format!("http://{addr2}/sse"),
            ..Default::default()
        };
        let res = probe_remote_with_client(&srv, &client).await.unwrap();
        assert!(res.ok);
        assert_eq!(res.server_name, "legacy-sse");
        assert_eq!(res.tools.len(), 1);
        assert_eq!(res.tools[0].name, "sse_ping");
        let _ = addr;
        task2.abort();
    }

    async fn req_is_sse_get(
        _stream: &mut tokio::net::TcpStream,
        _tx: &tokio::sync::mpsc::UnboundedSender<String>,
        _rx: &mut tokio::sync::mpsc::UnboundedReceiver<String>,
    ) -> bool {
        true
    }
}
