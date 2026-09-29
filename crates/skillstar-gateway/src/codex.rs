//! Codex config writer. Two loopback shapes. No vendor host and no vendor key.
//!
//! Logged-in sets `openai_base_url` to `{origin}/backend-api/codex`. API mode
//! writes `[model_providers.skillstar]`, points `model_provider` and
//! `model_catalog_json` at it, and writes an empty `skillstar-models.json`.
//! The provider table and the catalog file stay after release: a thread opened
//! on that provider still has to find the table. This module does not read
//! Usage, `auth.json`, or the provider store. The caller names the route.

use std::collections::BTreeMap;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use skillstar_core::infra::fs_ops::atomic_write;
use skillstar_core::infra::paths::config_dir;

use crate::PLACEHOLDER_BEARER;

/// On-disk stash object. Keys stay sorted.
type Stash = BTreeMap<String, String>;

const PROVIDER_NAME: &str = "skillstar";
const CATALOG_FILE: &str = "skillstar-models.json";
const STASH_FILE: &str = "agent_stash.json";
/// magpie `json.MarshalIndent` of an empty `models` list, one-space indent.
const EMPTY_CATALOG: &[u8] = b"{\n \"models\": []\n}";

/// Which Codex shape to write. The gateway does not infer this from a login.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CodexRoute {
    LoggedIn,
    Api,
}

/// Why a Codex apply or release did not finish.
#[derive(Debug)]
pub enum ApplyError {
    /// The agent is not one this crate writes yet. Nothing was opened for write.
    NotManaged,
    Io(io::Error),
}

impl std::fmt::Display for ApplyError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotManaged => f.write_str("agent_not_managed"),
            Self::Io(error) => write!(f, "{error}"),
        }
    }
}

impl std::error::Error for ApplyError {}

impl From<io::Error> for ApplyError {
    fn from(error: io::Error) -> Self {
        Self::Io(error)
    }
}

/// Point `agent` at the loopback gateway, or refuse before any write.
///
/// `origin` is `http://127.0.0.1:<port>` with no path. `home` is the directory
/// that contains `.codex/`, so tests never touch the real home.
pub fn apply_agent(
    agent: &str,
    route: CodexRoute,
    origin: &str,
    home: &Path,
) -> Result<(), ApplyError> {
    if agent != "codex" {
        return Err(ApplyError::NotManaged);
    }
    let origin = origin.trim_end_matches('/');
    match route {
        CodexRoute::LoggedIn => apply_logged_in(origin, home),
        CodexRoute::Api => apply_api(origin, home),
    }
}

/// Write stashed Codex fields back. The provider table and catalog file stay.
pub fn release_agent(agent: &str, home: &Path) -> Result<(), ApplyError> {
    if agent != "codex" {
        return Err(ApplyError::NotManaged);
    }
    release_codex(home)
}

fn apply_logged_in(origin: &str, home: &Path) -> Result<(), ApplyError> {
    let url = format!("{origin}/backend-api/codex");
    let path = config_path(home);
    let mut doc = read_doc(&path)?;
    let mut stash = load_stash()?;
    let provider = doc.root_value("model_provider");
    take_over(&mut doc, &mut stash, "openai_base_url", &url);
    if provider.as_deref() == Some(PROVIDER_NAME) {
        drop_owned(&mut doc, &mut stash, "model_provider", PROVIDER_NAME);
        drop_owned(&mut doc, &mut stash, "model_catalog_json", "");
    }
    // Stash first so a crash cannot lose the user's previous value.
    save_stash(&stash)?;
    write_doc(&path, &doc)
}

fn apply_api(origin: &str, home: &Path) -> Result<(), ApplyError> {
    let base = format!("{origin}/v1");
    let catalog = home.join(".codex").join(CATALOG_FILE);
    let catalog_value = path_utf8(&catalog)?;
    let gateway_url = format!("{origin}/backend-api/codex");
    let path = config_path(home);
    let mut doc = read_doc(&path)?;
    let mut stash = load_stash()?;
    doc.set_provider_table(&base);
    take_over(&mut doc, &mut stash, "model_provider", PROVIDER_NAME);
    take_over(&mut doc, &mut stash, "model_catalog_json", &catalog_value);
    if doc.root_value("openai_base_url").as_deref() == Some(gateway_url.as_str()) {
        drop_owned(&mut doc, &mut stash, "openai_base_url", &gateway_url);
    }
    save_stash(&stash)?;
    atomic_write(&catalog, EMPTY_CATALOG)?;
    write_doc(&path, &doc)
}

fn release_codex(home: &Path) -> Result<(), ApplyError> {
    let mut stash = load_stash()?;
    let path = config_path(home);
    let mut doc = read_doc(&path)?;
    let mut touched = false;
    for field in ["openai_base_url", "model_provider", "model_catalog_json"] {
        let key = stash_key(field);
        let Some(value) = stash.remove(&key) else {
            continue;
        };
        touched = true;
        if value.is_empty() {
            doc.remove_root(field);
        } else {
            doc.set_root(field, &value);
        }
    }
    if !touched {
        return Ok(());
    }
    write_doc(&path, &doc)?;
    save_stash(&stash)
}

fn take_over(doc: &mut Doc, stash: &mut Stash, field: &str, new_value: &str) {
    let current = doc.root_value(field);
    if current.as_deref() == Some(new_value) {
        return;
    }
    // Keep the first user value. A later mode switch must not overwrite it
    // with a URL or provider name this writer just stored.
    stash
        .entry(stash_key(field))
        .or_insert_with(|| current.unwrap_or_default());
    doc.set_root(field, new_value);
}

/// Remove a field only when it currently holds `sentinel`.
///
/// An empty `sentinel` means "whatever is there is ours" (the catalog path,
/// which this writer just set). A missing stash entry is recorded as `""`,
/// which release treats as "the user had no such key".
fn drop_owned(doc: &mut Doc, stash: &mut Stash, field: &str, sentinel: &str) {
    let Some(current) = doc.root_value(field) else {
        return;
    };
    if !sentinel.is_empty() && current != sentinel {
        return;
    }
    stash.entry(stash_key(field)).or_default();
    doc.remove_root(field);
}

fn stash_key(field: &str) -> String {
    format!("codex.{field}")
}

fn config_path(home: &Path) -> PathBuf {
    home.join(".codex").join("config.toml")
}

fn stash_path() -> PathBuf {
    config_dir().join(STASH_FILE)
}

fn path_utf8(path: &Path) -> Result<String, ApplyError> {
    path.to_str().map(str::to_string).ok_or_else(|| {
        ApplyError::Io(io::Error::new(
            io::ErrorKind::InvalidData,
            "path is not utf-8",
        ))
    })
}

fn load_stash() -> Result<Stash, ApplyError> {
    let path = stash_path();
    if !path.exists() {
        return Ok(Stash::new());
    }
    let bytes = fs::read(&path)?;
    serde_json::from_slice(&bytes).map_err(|error| {
        ApplyError::Io(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("agent_stash.json: {error}"),
        ))
    })
}

fn save_stash(stash: &Stash) -> Result<(), ApplyError> {
    let path = stash_path();
    if stash.is_empty() {
        if path.exists() {
            fs::remove_file(&path)?;
        }
        return Ok(());
    }
    let mut body = serde_json::to_vec_pretty(stash)
        .map_err(|error| ApplyError::Io(io::Error::new(io::ErrorKind::InvalidData, error)))?;
    if !body.ends_with(b"\n") {
        body.push(b'\n');
    }
    atomic_write(&path, &body)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&path, fs::Permissions::from_mode(0o600))?;
    }
    Ok(())
}

fn read_doc(path: &Path) -> Result<Doc, ApplyError> {
    if !path.exists() {
        return Ok(Doc { lines: Vec::new() });
    }
    let text = fs::read_to_string(path)?;
    Ok(Doc {
        lines: split_keep(&text),
    })
}

fn write_doc(path: &Path, doc: &Doc) -> Result<(), ApplyError> {
    atomic_write(path, doc.text().as_bytes())?;
    Ok(())
}

fn split_keep(text: &str) -> Vec<String> {
    if text.is_empty() {
        return Vec::new();
    }
    let mut lines = Vec::new();
    let mut start = 0;
    for (index, byte) in text.bytes().enumerate() {
        if byte == b'\n' {
            lines.push(text[start..=index].to_string());
            start = index + 1;
        }
    }
    if start < text.len() {
        lines.push(text[start..].to_string());
    }
    lines
}

struct Doc {
    lines: Vec<String>,
}

impl Doc {
    fn text(&self) -> String {
        self.lines.concat()
    }

    fn preamble_end(&self) -> usize {
        self.lines
            .iter()
            .position(|line| line_body(line).trim_start().starts_with('['))
            .unwrap_or(self.lines.len())
    }

    fn find_root(&self, key: &str) -> Option<usize> {
        let end = self.preamble_end();
        self.lines[..end]
            .iter()
            .position(|line| is_root_assign(line, key))
    }

    fn root_value(&self, key: &str) -> Option<String> {
        let index = self.find_root(key)?;
        let body = line_body(&self.lines[index]);
        let rest = body
            .strip_prefix(key)?
            .trim_start()
            .strip_prefix('=')?
            .trim_start();
        Some(unquote_basic(rest))
    }

    fn set_root(&mut self, key: &str, value: &str) {
        let rendered = format!("{key} = {}", quote_basic(value));
        if let Some(index) = self.find_root(key) {
            let newline = newline_suffix(&self.lines[index]);
            self.lines[index] = format!("{rendered}{newline}");
            return;
        }
        let end = self.preamble_end();
        if end > 0 {
            ensure_newline(&mut self.lines[end - 1]);
        }
        self.lines.insert(end, format!("{rendered}\n"));
    }

    fn remove_root(&mut self, key: &str) {
        if let Some(index) = self.find_root(key) {
            self.lines.remove(index);
        }
    }

    fn set_provider_table(&mut self, base_url: &str) {
        let header = format!("[model_providers.{PROVIDER_NAME}]");
        let block = vec![
            format!("{header}\n"),
            format!("name = {}\n", quote_basic(PROVIDER_NAME)),
            format!("base_url = {}\n", quote_basic(base_url)),
            "wire_api = \"responses\"\n".to_string(),
            format!(
                "experimental_bearer_token = {}\n",
                quote_basic(PLACEHOLDER_BEARER)
            ),
        ];
        if let Some(start) = self
            .lines
            .iter()
            .position(|line| line_body(line).trim() == header)
        {
            let mut end = start + 1;
            while end < self.lines.len()
                && !line_body(&self.lines[end]).trim_start().starts_with('[')
            {
                end += 1;
            }
            self.lines.splice(start..end, block);
            return;
        }
        if let Some(last) = self.lines.last_mut() {
            ensure_newline(last);
        }
        if self
            .lines
            .last()
            .is_some_and(|line| !line_body(line).trim().is_empty())
        {
            self.lines.push("\n".to_string());
        }
        self.lines.extend(block);
    }
}

fn line_body(line: &str) -> &str {
    line.trim_end_matches(['\n', '\r'])
}

fn is_root_assign(line: &str, key: &str) -> bool {
    let body = line_body(line);
    let Some(rest) = body.strip_prefix(key) else {
        return false;
    };
    rest.trim_start().starts_with('=')
}

fn newline_suffix(line: &str) -> &str {
    if line.ends_with("\r\n") {
        "\r\n"
    } else if line.ends_with('\n') {
        "\n"
    } else {
        ""
    }
}

fn ensure_newline(line: &mut String) {
    if !line.ends_with('\n') {
        line.push('\n');
    }
}

fn quote_basic(value: &str) -> String {
    let mut out = String::from("\"");
    for ch in value.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            _ => out.push(ch),
        }
    }
    out.push('"');
    out
}

fn unquote_basic(raw: &str) -> String {
    let Some(body) = raw.strip_prefix('"') else {
        return raw.split_whitespace().next().unwrap_or("").to_string();
    };
    let mut out = String::new();
    let mut chars = body.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '\\' {
            match chars.next() {
                Some('n') => out.push('\n'),
                Some('t') => out.push('\t'),
                Some('r') => out.push('\r'),
                Some(other) => out.push(other),
                None => break,
            }
        } else if ch == '"' {
            break;
        } else {
            out.push(ch);
        }
    }
    out
}
