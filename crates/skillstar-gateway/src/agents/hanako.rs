//! OpenHanako. A running process is updated through its local API.
//! Otherwise the catalog and the primary agent's config are replaced.

use std::fs;
use std::io::{self, Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::path::{Path, PathBuf};
use std::time::Duration;

use serde_json::Value;

use super::body::file;
use super::sandboxed;
use crate::codex::ApplyError;

const PROBE: Duration = Duration::from_millis(1500);
const PUT: Duration = Duration::from_secs(15);
const BODY_CAP: usize = 1 << 20;

pub(super) fn apply(home: &Path, origin: &str, model_ref: &str) -> Result<(), ApplyError> {
    let dir = hanako_dir(home);
    let agent = find_agent(&dir);
    if let Some(server) = live(&dir) {
        return if model_ref.is_empty() {
            release_live(&server, agent.as_deref())
        } else {
            let agent = agent.ok_or_else(no_agent)?;
            save_live(&server, &agent, origin, model_ref)
        };
    }
    if model_ref.is_empty() {
        return restore_files(home, &dir, agent.as_deref());
    }
    let agent = agent.ok_or_else(no_agent)?;
    write_files(home, &dir, &agent, origin, model_ref)
}

fn no_agent() -> ApplyError {
    io::Error::new(
        io::ErrorKind::NotFound,
        "OpenHanako has no agent yet",
    )
    .into()
}

fn save_live(server: &Server, agent: &str, origin: &str, model_ref: &str) -> Result<(), ApplyError> {
    let key = super::token_for("hanako");
    let v1 = format!("{origin}/v1");
    server.put(
        "/api/config",
        &serde_json::json!({
            "providers": {
                "skillstar": {
                    "display_name": "skillstar",
                    "base_url": v1,
                    "api": "openai-completions",
                    "api_key": key,
                    "models": [{
                        "id": model_ref,
                        "image": false,
                        "reasoning": false
                    }]
                }
            }
        }),
    )?;
    server.put(
        &format!("/api/agents/{agent}/config"),
        &serde_json::json!({
            "models": {
                "chat": {
                    "id": model_ref,
                    "provider": "skillstar"
                }
            }
        }),
    )
}

fn release_live(server: &Server, agent: Option<&str>) -> Result<(), ApplyError> {
    server.put(
        "/api/config",
        &serde_json::json!({ "providers": { "skillstar": null } }),
    )?;
    if let Some(agent) = agent {
        server.put(
            &format!("/api/agents/{agent}/config"),
            &serde_json::json!({ "models": { "chat": null } }),
        )?;
    }
    Ok(())
}

fn write_files(
    home: &Path,
    dir: &Path,
    agent: &str,
    origin: &str,
    model_ref: &str,
) -> Result<(), ApplyError> {
    let files = owned_files(home, dir, Some(agent), origin, model_ref);
    super::remember("hanako", &files)?;
    for written in &files {
        skillstar_core::infra::fs_ops::atomic_write(&written.path, written.body.as_bytes())?;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(
            dir.join("provider-catalog.json"),
            fs::Permissions::from_mode(0o600),
        )?;
    }
    let plugin = dir.join("provider-plugins").join("skillstar");
    if plugin.exists() {
        fs::remove_dir_all(plugin)?;
    }
    Ok(())
}

fn restore_files(home: &Path, dir: &Path, agent: Option<&str>) -> Result<(), ApplyError> {
    let files = owned_files(home, dir, agent, "", "");
    super::restore("hanako", &files)
}

fn owned_files(
    home: &Path,
    dir: &Path,
    agent: Option<&str>,
    origin: &str,
    model_ref: &str,
) -> Vec<super::Written> {
    let mut files = vec![file(
        home,
        dir.join("provider-catalog.json"),
        catalog_body(origin, model_ref),
    )];
    if let Some(agent) = agent {
        files.push(file(
            home,
            dir.join("agents").join(agent).join("config.yaml"),
            chat_body(model_ref),
        ));
    }
    files
}

fn catalog_body(origin: &str, model_ref: &str) -> String {
    if origin.is_empty() {
        return String::new();
    }
    let key = super::token_for("hanako");
    format!(
        "{{\n  \"catalogVersion\": 2,\n  \"providers\": {{\n    \"skillstar\": {{\n      \"display_name\": \"skillstar\",\n      \"base_url\": {url},\n      \"api\": \"openai-completions\",\n      \"api_key\": {key},\n      \"models\": [\n        {{\n          \"id\": {model},\n          \"image\": false,\n          \"reasoning\": false\n        }}\n      ]\n    }}\n  }}\n}}\n",
        url = json_string(&format!("{origin}/v1")),
        key = json_string(&key),
        model = json_string(model_ref),
    )
}

fn chat_body(model_ref: &str) -> String {
    if model_ref.is_empty() {
        return String::new();
    }
    format!(
        "models:\n  chat:\n    id: {model}\n    provider: \"skillstar\"\n",
        model = yaml_string(model_ref),
    )
}

pub(super) fn catalog_path(home: &Path) -> PathBuf {
    hanako_dir(home).join("provider-catalog.json")
}

fn hanako_dir(home: &Path) -> PathBuf {
    if !sandboxed()
        && let Some(value) = std::env::var("HANA_HOME").ok().filter(|value| !value.is_empty())
    {
        return expand_home(home, &value);
    }
    home.join(".hanako")
}

fn expand_home(home: &Path, value: &str) -> PathBuf {
    if let Some(rest) = value.strip_prefix('~')
        && (rest.is_empty() || rest.starts_with('/') || rest.starts_with('\\'))
    {
        return home.join(rest.trim_start_matches(['/', '\\']));
    }
    let path = PathBuf::from(value);
    if path.is_absolute() {
        path
    } else {
        home.join(path)
    }
}

fn find_agent(dir: &Path) -> Option<String> {
    if let Some(id) = primary_agent(dir)
        && has_config(dir, &id)
    {
        return Some(id);
    }
    let mut names = Vec::new();
    let entries = fs::read_dir(dir.join("agents")).ok()?;
    for entry in entries.flatten() {
        let Some(name) = entry.file_name().to_str().map(str::to_string) else {
            continue;
        };
        if entry.path().is_dir() && has_config(dir, &name) {
            names.push(name);
        }
    }
    names.sort();
    names.into_iter().next()
}

fn primary_agent(dir: &Path) -> Option<String> {
    let bytes = fs::read(dir.join("user").join("preferences.json")).ok()?;
    let value: Value = serde_json::from_slice(&bytes).ok()?;
    let id = value.get("primaryAgent").and_then(Value::as_str)?;
    agent_name_ok(id).then(|| id.to_string())
}

fn has_config(dir: &Path, id: &str) -> bool {
    agent_name_ok(id) && dir.join("agents").join(id).join("config.yaml").is_file()
}

fn agent_name_ok(id: &str) -> bool {
    !id.is_empty() && !id.contains(['/', '\\'])
}

struct Server {
    port: u16,
    token: String,
}

fn live(dir: &Path) -> Option<Server> {
    let bytes = fs::read(dir.join("server-info.json")).ok()?;
    let value: Value = serde_json::from_slice(&bytes).ok()?;
    let port = value.get("port")?.as_u64()?;
    let port = u16::try_from(port).ok()?;
    let token = value.get("token")?.as_str()?.to_string();
    if port == 0 || token.is_empty() || token.contains(['\r', '\n']) {
        return None;
    }
    let server = Server { port, token };
    let (status, body) = server
        .exchange("GET", "/api/server/identity", None, PROBE)
        .ok()?;
    if status != 200 {
        return None;
    }
    let parsed: Value = serde_json::from_slice(&body).ok()?;
    let id = parsed.get("serverId").and_then(Value::as_str).unwrap_or("");
    (!id.is_empty()).then_some(server)
}

impl Server {
    fn put(&self, path: &str, body: &Value) -> Result<(), ApplyError> {
        let bytes = serde_json::to_vec(body)
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
        let (status, _) = self.exchange("PUT", path, Some(&bytes), PUT)?;
        if (200..300).contains(&status) {
            return Ok(());
        }
        Err(io::Error::other(format!("OpenHanako: {status} {path}")).into())
    }

    fn exchange(
        &self,
        method: &str,
        path: &str,
        body: Option<&[u8]>,
        timeout: Duration,
    ) -> Result<(u16, Vec<u8>), ApplyError> {
        let addr = SocketAddr::from(([127, 0, 0, 1], self.port));
        let mut stream = TcpStream::connect_timeout(&addr, timeout)?;
        stream.set_read_timeout(Some(timeout))?;
        stream.set_write_timeout(Some(timeout))?;
        let mut header = format!(
            "{method} {path} HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nAuthorization: Bearer {token}\r\nConnection: close\r\n",
            port = self.port,
            token = self.token,
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
        split_response(&raw).map_err(ApplyError::from)
    }
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
        .ok_or_else(|| io::Error::new(io::ErrorKind::UnexpectedEof, "short Hanako response"))?;
    let head = std::str::from_utf8(&raw[..split])
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
    let status = head
        .split_whitespace()
        .nth(1)
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "Hanako status missing"))?
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

fn json_string(value: &str) -> String {
    serde_json::to_string(value).unwrap_or_else(|_| "\"\"".to_string())
}

fn yaml_string(value: &str) -> String {
    format!(
        "\"{}\"",
        value
            .replace('\\', "\\\\")
            .replace('"', "\\\"")
            .replace('\n', "\\n")
    )
}
