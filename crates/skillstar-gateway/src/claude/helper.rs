//! stdio MCP helper. Claude Code starts it as
//! `skillstar claude-mcp-helper <callback> <tools-file>`.
//!
//! stdout is only JSON-RPC frames. Diagnostics go to stderr. A `tools/call`
//! posts to the callback with a plain TCP client: the callback is on this
//! machine, and the proxy client would send that hop off-box.

use std::io::{self, BufRead, BufReader, Read, Write};
use std::net::TcpStream;
use std::sync::{Arc, Mutex};
use std::thread;

use serde_json::{Value, json};

use crate::outbound::note_outbound;

const MAX_LINE: usize = 16 << 20;
const MAX_HTTP_BODY: usize = 32 << 20;

pub fn run_mcp_helper(
    args: &[String],
    stdin: impl Read,
    stdout: impl Write + Send + 'static,
    mut stderr: impl Write,
) -> i32 {
    if args.len() != 2 {
        let _ = writeln!(
            stderr,
            "claude MCP helper expects callback URL and tools file"
        );
        return 1;
    }
    let callback = args[0].clone();
    let tools = match read_tools(&args[1]) {
        Ok(tools) => Arc::new(tools),
        Err(error) => {
            let _ = writeln!(stderr, "{error}");
            return 1;
        }
    };
    let stdout = Arc::new(Mutex::new(stdout));
    let mut reader = BufReader::new(stdin);
    let mut joins = Vec::new();
    loop {
        let frame = match read_frame(&mut reader) {
            Ok(Some(frame)) => frame,
            Ok(None) => break,
            Err(error) => {
                let _ = writeln!(stderr, "{error}");
                return 1;
            }
        };
        if frame.is_empty() {
            continue;
        }
        let Some(incoming) = Incoming::parse(&frame) else {
            continue;
        };
        let stdout = Arc::clone(&stdout);
        let tools = Arc::clone(&tools);
        let callback = callback.clone();
        joins.push(thread::spawn(move || {
            let response = incoming.respond(&callback, &tools);
            write_frame(&stdout, &response);
        }));
    }
    for join in joins {
        let _ = join.join();
    }
    0
}

struct Incoming {
    id: String,
    method: String,
    params: String,
}

impl Incoming {
    fn parse(frame: &[u8]) -> Option<Self> {
        let value: Value = serde_json::from_slice(frame).ok()?;
        // Notifications omit id and get no frame back. A present null id is still an id.
        let id = value.get("id")?;
        let method = value.get("method")?.as_str()?.to_string();
        let params = value
            .get("params")
            .map(|params| params.to_string())
            .unwrap_or_else(|| "null".to_string());
        Some(Self {
            id: id.to_string(),
            method,
            params,
        })
    }

    fn respond(&self, callback: &str, tools: &[Value]) -> Value {
        let id: Value = serde_json::from_str(&self.id).unwrap_or(Value::Null);
        let (result, error) = match self.method.as_str() {
            "initialize" => (
                Some(json!({
                    "protocolVersion": "2025-06-18",
                    "capabilities": {"tools": {}},
                    "serverInfo": {"name": "skillstar", "version": "1"}
                })),
                None,
            ),
            "tools/list" => (Some(json!({ "tools": tools })), None),
            "tools/call" => match call_tool(callback, &self.params) {
                Ok(result) => (Some(result), None),
                Err(error) => (None, Some(rpc_error(-32000, error.to_string()))),
            },
            _ => (None, Some(rpc_error(-32601, "method not found"))),
        };
        let mut response = serde_json::Map::new();
        response.insert("jsonrpc".into(), json!("2.0"));
        response.insert("id".into(), id);
        if let Some(error) = error {
            response.insert("error".into(), error);
        } else {
            response.insert("result".into(), result.unwrap_or(Value::Null));
        }
        Value::Object(response)
    }
}

fn rpc_error(code: i64, message: impl Into<String>) -> Value {
    json!({ "code": code, "message": message.into() })
}

/// One failed `tools/call`. The text is the JSON-RPC error Claude Code sees.
struct CallError {
    message: String,
}

impl CallError {
    fn from_display(error: impl std::fmt::Display) -> Self {
        Self {
            message: error.to_string(),
        }
    }
}

impl std::fmt::Display for CallError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

fn call_tool(callback: &str, params: &str) -> Result<Value, CallError> {
    let params: Value = serde_json::from_str(params).map_err(CallError::from_display)?;
    let name = params
        .get("name")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    let arguments = params.get("arguments").cloned().unwrap_or(Value::Null);
    // The key contains a slash. A JSON pointer would treat that slash as a path split.
    let meta_id = params
        .get("_meta")
        .and_then(|meta| meta.get("claudecode/toolUseId"))
        .and_then(Value::as_str)
        .filter(|id| !id.is_empty())
        .map(str::to_string);
    let tool_call_id = meta_id.unwrap_or_else(|| format!("call_{}", super::random_hex(12)));
    let payload = json!({
        "tool_call_id": tool_call_id,
        "name": name,
        "arguments": arguments,
    });
    let bytes = serde_json::to_vec(&payload).map_err(CallError::from_display)?;
    let (status, body) = post_json(callback, &bytes).map_err(CallError::from_display)?;
    if status != 200 {
        return Err(CallError::from_display(
            String::from_utf8_lossy(&body).into_owned(),
        ));
    }
    let parsed: Value = serde_json::from_slice(&body).map_err(CallError::from_display)?;
    let content = parsed.get("content").cloned().unwrap_or(Value::Null);
    let is_error = parsed
        .get("is_error")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    Ok(json!({ "content": content, "isError": is_error }))
}

fn post_json(url: &str, payload: &[u8]) -> Result<(u16, Vec<u8>), String> {
    note_outbound(url);
    let rest = url
        .strip_prefix("http://")
        .ok_or_else(|| "callback must be http".to_string())?;
    let (authority, path) = match rest.split_once('/') {
        Some((authority, path)) => (authority, format!("/{path}")),
        None => (rest, "/".to_string()),
    };
    if authority.is_empty() {
        return Err("callback is missing a host".to_string());
    }
    let mut stream = TcpStream::connect(authority).map_err(|error| error.to_string())?;
    let header = format!(
        "POST {path} HTTP/1.1\r\nHost: {authority}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        payload.len()
    );
    stream
        .write_all(header.as_bytes())
        .map_err(|error| error.to_string())?;
    stream
        .write_all(payload)
        .map_err(|error| error.to_string())?;
    let mut raw = Vec::new();
    stream
        .take((MAX_HTTP_BODY + 64 * 1024) as u64)
        .read_to_end(&mut raw)
        .map_err(|error| error.to_string())?;
    split_http(&raw)
}

fn split_http(raw: &[u8]) -> Result<(u16, Vec<u8>), String> {
    let split = raw
        .windows(4)
        .position(|window| window == b"\r\n\r\n")
        .ok_or_else(|| "short callback response".to_string())?;
    let head = std::str::from_utf8(&raw[..split]).map_err(|error| error.to_string())?;
    let status = head
        .split_whitespace()
        .nth(1)
        .ok_or_else(|| "callback status missing".to_string())?
        .parse::<u16>()
        .map_err(|error| error.to_string())?;
    let mut body = raw[split + 4..].to_vec();
    for line in head.split("\r\n").skip(1) {
        let Some((name, value)) = line.split_once(':') else {
            continue;
        };
        if name.eq_ignore_ascii_case("content-length")
            && let Ok(length) = value.trim().parse::<usize>()
        {
            body.truncate(length.min(body.len()));
        }
    }
    body.truncate(MAX_HTTP_BODY);
    Ok((status, body))
}

fn read_tools(path: &str) -> Result<Vec<Value>, String> {
    let bytes = std::fs::read(path).map_err(|error| error.to_string())?;
    serde_json::from_slice(&bytes).map_err(|error| format!("read MCP tools: {error}"))
}

fn read_frame(reader: &mut impl BufRead) -> io::Result<Option<Vec<u8>>> {
    let mut buf = Vec::new();
    let read = reader
        .take((MAX_LINE as u64) + 1)
        .read_until(b'\n', &mut buf)?;
    if read == 0 {
        return Ok(None);
    }
    if buf.len() > MAX_LINE {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "mcp line too long",
        ));
    }
    if buf.last() == Some(&b'\n') {
        buf.pop();
    }
    if buf.last() == Some(&b'\r') {
        buf.pop();
    }
    Ok(Some(buf))
}

fn write_frame(stdout: &Mutex<impl Write>, payload: &Value) {
    let mut guard = stdout
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let _ = serde_json::to_writer(&mut *guard, payload);
    let _ = guard.write_all(b"\n");
    let _ = guard.flush();
}
