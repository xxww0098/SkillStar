//! Alma keeps providers in the app. A running process is updated through
//! `http://localhost:23001`. Alma not answering is success and writes nothing.

use std::io::{self, Read, Write};
use std::net::{SocketAddr, TcpStream, ToSocketAddrs};
use std::time::Duration;

use serde_json::Value;

use super::sandboxed;
use crate::PLACEHOLDER_BEARER;
use crate::codex::ApplyError;

const BODY_CAP: usize = 4 << 20;

pub(super) fn apply(origin: &str, model_ref: &str) -> Result<(), ApplyError> {
    let Some(endpoint) = endpoint()? else {
        return Ok(());
    };
    let timeout = probe_timeout();
    let Some(providers) = get_providers(&endpoint, timeout)? else {
        return Ok(());
    };
    let v1 = format!("{origin}/v1");
    if model_ref.is_empty() {
        return release(&endpoint, timeout, &providers, &v1);
    }
    let id = ensure_provider(&endpoint, timeout, &providers, &v1)?;
    put_json(
        &endpoint,
        timeout,
        &format!("/api/providers/{id}/models"),
        &serde_json::json!({
            "models": crate::visible::listed_ids("alma", model_ref),
            "availableModels": crate::visible::listed_ids("alma", model_ref)
                .into_iter()
                .map(|id| serde_json::json!({ "id": id, "name": id }))
                .collect::<Vec<_>>()
        }),
    )?;
    let mut settings = get_json(&endpoint, timeout, "/api/settings")?;
    assign_default(&mut settings, &format!("{id}:{model_ref}"));
    put_json(&endpoint, timeout, "/api/settings", &settings)
}

fn release(
    endpoint: &Endpoint,
    timeout: Duration,
    providers: &[Value],
    v1: &str,
) -> Result<(), ApplyError> {
    let Some(provider) = ours(providers, v1) else {
        return Ok(());
    };
    let id = provider_id(provider)?;
    let mut settings = get_json(endpoint, timeout, "/api/settings")?;
    if default_provider(&settings) == id {
        assign_default(&mut settings, "");
        put_json(endpoint, timeout, "/api/settings", &settings)?;
    }
    let (status, _) = exchange(endpoint, "DELETE", &format!("/api/providers/{id}"), None, timeout)?;
    http_ok(status, "/api/providers")
}

fn ensure_provider(
    endpoint: &Endpoint,
    timeout: Duration,
    providers: &[Value],
    v1: &str,
) -> Result<String, ApplyError> {
    let key = super::token_for("alma");
    if let Some(provider) = ours(providers, v1) {
        let id = provider_id(provider)?;
        let base = provider.get("baseURL").and_then(Value::as_str).unwrap_or("");
        let enabled = provider.get("enabled").and_then(Value::as_bool).unwrap_or(false);
        let api_key = provider.get("apiKey").and_then(Value::as_str).unwrap_or("");
        if base != v1 || !enabled {
            put_json(
                endpoint,
                timeout,
                &format!("/api/providers/{id}"),
                &serde_json::json!({ "baseURL": v1, "apiKey": key, "enabled": true }),
            )?;
        } else if api_key == PLACEHOLDER_BEARER {
            put_json(
                endpoint,
                timeout,
                &format!("/api/providers/{id}"),
                &serde_json::json!({ "apiKey": key }),
            )?;
        }
        return Ok(id);
    }
    let made = post_json(
        endpoint,
        timeout,
        "/api/providers",
        &serde_json::json!({
            "name": "skillstar",
            "type": "openai",
            "apiKey": key,
            "baseURL": v1,
            "enabled": true
        }),
    )?;
    if let Some(id) = made.get("id").and_then(Value::as_str)
        && !id.is_empty()
    {
        return provider_id(&made);
    }
    let again = get_providers(endpoint, timeout)?
        .ok_or_else(|| io::Error::other("Alma didn't keep skillstar's provider"))?;
    ours(&again, v1)
        .ok_or_else(|| io::Error::other("Alma didn't keep skillstar's provider").into())
        .and_then(provider_id)
}

fn ours<'a>(providers: &'a [Value], v1: &str) -> Option<&'a Value> {
    if let Some(named) = providers.iter().find(|provider| {
        provider.get("name").and_then(Value::as_str) == Some("skillstar") && openai_like(provider)
    }) {
        return Some(named);
    }
    let host = url_host(v1);
    if host.is_empty() {
        return None;
    }
    providers.iter().find(|provider| {
        openai_like(provider)
            && url_host(provider.get("baseURL").and_then(Value::as_str).unwrap_or("")) == host
    })
}

fn openai_like(provider: &Value) -> bool {
    matches!(
        provider.get("type").and_then(Value::as_str),
        Some("openai" | "custom")
    )
}

fn provider_id(provider: &Value) -> Result<String, ApplyError> {
    let id = provider.get("id").and_then(Value::as_str).unwrap_or("");
    if id.is_empty() || id.contains(['/', '?', '#', ' ', '\\', '\r', '\n']) {
        return Err(io::Error::other("Alma provider id").into());
    }
    Ok(id.to_string())
}

fn assign_default(settings: &mut Value, model: &str) {
    if !settings.is_object() {
        *settings = serde_json::json!({});
    }
    let root = settings.as_object_mut().expect("settings object");
    let chat = root.entry("chat").or_insert_with(|| serde_json::json!({}));
    if !chat.is_object() {
        *chat = serde_json::json!({});
    }
    chat.as_object_mut()
        .expect("chat object")
        .insert("defaultModel".to_string(), Value::String(model.to_string()));
}

fn default_provider(settings: &Value) -> &str {
    settings
        .get("chat")
        .and_then(|chat| chat.get("defaultModel"))
        .and_then(Value::as_str)
        .and_then(|value| value.split_once(':'))
        .map(|(provider, _)| provider)
        .unwrap_or("")
}

fn get_providers(endpoint: &Endpoint, timeout: Duration) -> Result<Option<Vec<Value>>, ApplyError> {
    let (status, body) = match exchange(endpoint, "GET", "/api/providers", None, timeout) {
        Ok(response) => response,
        Err(_) => return Ok(None),
    };
    http_ok(status, "/api/providers")?;
    serde_json::from_slice(&body)
        .map(Some)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error).into())
}

fn get_json(endpoint: &Endpoint, timeout: Duration, path: &str) -> Result<Value, ApplyError> {
    let (status, body) = exchange(endpoint, "GET", path, None, timeout)?;
    http_ok(status, path)?;
    if body.is_empty() {
        return Ok(serde_json::json!({}));
    }
    serde_json::from_slice(&body).map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error).into())
}

fn post_json(
    endpoint: &Endpoint,
    timeout: Duration,
    path: &str,
    body: &Value,
) -> Result<Value, ApplyError> {
    let bytes = serde_json::to_vec(body)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
    let (status, raw) = exchange(endpoint, "POST", path, Some(&bytes), timeout)?;
    http_ok(status, path)?;
    if raw.is_empty() {
        return Ok(serde_json::json!({}));
    }
    serde_json::from_slice(&raw).map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error).into())
}

fn put_json(
    endpoint: &Endpoint,
    timeout: Duration,
    path: &str,
    body: &Value,
) -> Result<(), ApplyError> {
    let bytes = serde_json::to_vec(body)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
    let (status, _) = exchange(endpoint, "PUT", path, Some(&bytes), timeout)?;
    http_ok(status, path)
}

fn http_ok(status: u16, path: &str) -> Result<(), ApplyError> {
    if (200..300).contains(&status) {
        return Ok(());
    }
    Err(io::Error::other(format!("Alma: {status} {path}")).into())
}

struct Endpoint {
    host: String,
    port: u16,
}

fn endpoint() -> Result<Option<Endpoint>, ApplyError> {
    let raw = if sandboxed() {
        match std::env::var("SKILLSTAR_ALMA_URL") {
            Ok(value) if !value.trim().is_empty() => value,
            _ => return Ok(None),
        }
    } else {
        production_base().to_string()
    };
    parse_endpoint(raw.trim()).map(Some)
}

fn production_base() -> &'static str {
    "http://localhost:23001"
}

fn parse_endpoint(raw: &str) -> Result<Endpoint, ApplyError> {
    let Some(rest) = raw.strip_prefix("http://") else {
        return Err(io::Error::other("Alma URL").into());
    };
    if rest.contains(['/', '?', '#', '@']) {
        return Err(io::Error::other("Alma URL").into());
    }
    let Some((host, port)) = rest.rsplit_once(':') else {
        return Err(io::Error::other("Alma URL").into());
    };
    if host != "localhost" && host != "127.0.0.1" {
        return Err(io::Error::other("Alma URL").into());
    }
    let Ok(port) = port.parse::<u16>() else {
        return Err(io::Error::other("Alma URL").into());
    };
    if port == 0 {
        return Err(io::Error::other("Alma URL").into());
    }
    Ok(Endpoint {
        host: host.to_string(),
        port,
    })
}

fn probe_timeout() -> Duration {
    Duration::from_millis(timeout_ms(
        sandboxed(),
        std::env::var("SKILLSTAR_ALMA_TIMEOUT_MS").ok().as_deref(),
    ))
}

fn timeout_ms(sandboxed: bool, raw: Option<&str>) -> u64 {
    if sandboxed
        && let Some(raw) = raw
        && let Ok(ms) = raw.trim().parse::<u64>()
    {
        return ms.clamp(1, 1_000);
    }
    1_000
}

fn url_host(raw: &str) -> String {
    let rest = raw.split("://").nth(1).unwrap_or(raw);
    rest.split(['/', '?', '#']).next().unwrap_or("").to_ascii_lowercase()
}

fn exchange(
    endpoint: &Endpoint,
    method: &str,
    path: &str,
    body: Option<&[u8]>,
    timeout: Duration,
) -> io::Result<(u16, Vec<u8>)> {
    let mut stream = connect(endpoint, timeout)?;
    let mut header = format!(
        "{method} {path} HTTP/1.1\r\nHost: {host}:{port}\r\nConnection: close\r\n",
        host = endpoint.host,
        port = endpoint.port,
    );
    if let Some(body) = body {
        header.push_str("Content-Type: application/json\r\n");
        header.push_str(&format!("Content-Length: {}\r\n", body.len()));
    }
    header.push_str("\r\n");
    stream.write_all(header.as_bytes())?;
    if let Some(body) = body {
        stream.write_all(body)?;
    }
    let raw = read_limited(&mut stream, BODY_CAP)?;
    split_response(&raw)
}

fn connect(endpoint: &Endpoint, timeout: Duration) -> io::Result<TcpStream> {
    let addrs: Vec<SocketAddr> = (endpoint.host.as_str(), endpoint.port)
        .to_socket_addrs()?
        .collect();
    if addrs.is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            "Alma isn't running",
        ));
    }
    let mut last = io::Error::new(io::ErrorKind::ConnectionRefused, "Alma isn't running");
    for addr in addrs {
        match TcpStream::connect_timeout(&addr, timeout) {
            Ok(stream) => {
                stream.set_read_timeout(Some(timeout))?;
                stream.set_write_timeout(Some(timeout))?;
                return Ok(stream);
            }
            Err(error) => last = error,
        }
    }
    Err(last)
}

fn read_limited(stream: &mut TcpStream, max: usize) -> io::Result<Vec<u8>> {
    let mut raw = Vec::new();
    let mut tmp = [0u8; 8192];
    while raw.len() < max {
        match stream.read(&mut tmp) {
            Ok(0) => break,
            Ok(n) => {
                let room = max - raw.len();
                raw.extend_from_slice(&tmp[..n.min(room)]);
            }
            Err(error)
                if error.kind() == io::ErrorKind::WouldBlock
                    || error.kind() == io::ErrorKind::TimedOut =>
            {
                if raw.is_empty() {
                    return Err(error);
                }
                break;
            }
            Err(error) => return Err(error),
        }
    }
    Ok(raw)
}

fn split_response(raw: &[u8]) -> io::Result<(u16, Vec<u8>)> {
    let split = raw
        .windows(4)
        .position(|window| window == b"\r\n\r\n")
        .ok_or_else(|| io::Error::new(io::ErrorKind::UnexpectedEof, "short Alma response"))?;
    let head = std::str::from_utf8(&raw[..split])
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
    let status = head
        .split_whitespace()
        .nth(1)
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "Alma status missing"))?
        .parse::<u16>()
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
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
    Ok((status, body))
}

#[cfg(test)]
mod tests {
    use super::{parse_endpoint, production_base, timeout_ms};

    #[test]
    fn alma_production_base_is_localhost_23001() {
        assert_eq!(production_base(), "http://localhost:23001");
        assert!(parse_endpoint(production_base()).is_ok());
        assert!(parse_endpoint("http://127.0.0.1:9").is_ok());
        assert!(parse_endpoint("http://evil.example:23001").is_err());
        assert!(parse_endpoint("https://localhost:23001").is_err());
    }

    #[test]
    fn alma_timeout_caps_at_one_second() {
        assert_eq!(timeout_ms(true, Some("200")), 200);
        assert_eq!(timeout_ms(true, Some("5000")), 1_000);
        assert_eq!(timeout_ms(true, Some("0")), 1);
        assert_eq!(timeout_ms(false, Some("200")), 1_000);
        assert_eq!(timeout_ms(true, None), 1_000);
    }
}
