//! DeepSeek Harness MCP projection — `$DSH_HOME/cordis.patch.yml`.
//!
//! DSH has no `mcpServers` JSON file. Each MCP server is one instance of
//! `@deepseek-ai/dsh-mcp-client`, persisted as a Cordis `insert` row. The
//! documented user-scope seam is the **home-level** patch (applied to every
//! profile). SkillStar does not write `profiles/<name>/cordis.patch.yml`:
//! there is no DSH profile picker, and every other MCP target is user-scope.
//!
//! Wire facts the rest of the registry cannot guess:
//! - transports are `stdio` and `streamable-http` only — SSE is written as
//!   streamable-http and reads back as `http`
//! - `serverName` is `[A-Za-z0-9_-]{1,32}`; the plugin `id` is `mcp-<store
//!   name>` so a truncated serverName still round-trips the SkillStar key.
//!   Matching keys off that `id`, never the truncated `serverName`
//! - the file is a YAML **sequence of patch ops**, not a mapping; a mapping
//!   root is refused rather than rewritten

use anyhow::{Context, Result, bail};
use serde_yaml::{Mapping, Value};
use skillstar_core::infra::fs_ops::atomic_write;
use std::collections::BTreeMap;
use std::path::Path;

use super::{McpServerEntry, blank_entry};

const PLUGIN_NAME: &str = "@deepseek-ai/dsh-mcp-client";
const INSERT: &str = "insert";
const ID: &str = "id";
const NAME: &str = "name";
const CONFIG: &str = "config";
const SERVER_NAME: &str = "serverName";
const TRANSPORT: &str = "transport";
const STREAMABLE_HTTP: &str = "streamable-http";
const TOOL_CALL_TIMEOUT_MS: &str = "toolCallTimeoutMs";
const PLUGIN_ID_PREFIX: &str = "mcp-";
const SERVER_NAME_MAX: usize = 32;

pub(crate) fn dsh_upsert(path: &Path, entry: &McpServerEntry) -> Result<()> {
    let mut ops = read_patch_strict(path)?;
    let row = plugin_row(entry);
    if let Some((op_i, row_i)) = find_row_coords(&ops, &entry.name) {
        let entries = insert_entries_mut(&mut ops[op_i])
            .expect("invariant: find_row_coords only yields insert ops");
        entries[row_i] = row;
    } else if let Some(op_i) = last_mcp_insert_index(&ops) {
        insert_entries_mut(&mut ops[op_i])
            .expect("invariant: last_mcp_insert_index only yields insert ops")
            .push(row);
    } else {
        let mut op = Mapping::new();
        op.insert(yaml_str(INSERT), Value::Sequence(vec![row]));
        ops.push(Value::Mapping(op));
    }
    write_yaml(path, &Value::Sequence(ops))
}

pub(crate) fn dsh_remove(path: &Path, name: &str) -> Result<()> {
    if !path.exists() {
        return Ok(());
    }
    let mut ops = read_patch_strict(path)?;
    let Some((op_i, row_i)) = find_row_coords(&ops, name) else {
        // **Nothing to remove means nothing is written.** Rewriting drops the
        // document's comments (see [`write_yaml`]), so an absent server must
        // not cost the user the rest of their patch file's formatting.
        return Ok(());
    };
    {
        let entries = insert_entries_mut(&mut ops[op_i])
            .expect("invariant: find_row_coords only yields insert ops");
        entries.remove(row_i);
    }
    if insert_entries(&ops[op_i]).is_some_and(Vec::is_empty) {
        ops.remove(op_i);
    }
    write_yaml(path, &Value::Sequence(ops))
}

pub(crate) fn count_dsh_mcp(content: &str) -> usize {
    parse_patch(content)
        .map(|ops| mcp_client_rows(&ops).count())
        .unwrap_or(0)
}

pub(crate) fn read_dsh_entries(content: &str) -> Result<Vec<McpServerEntry>> {
    let ops = parse_patch(content)?;
    Ok(mcp_client_rows(&ops).filter_map(entry_from_row).collect())
}

fn plugin_id(store_name: &str) -> String {
    format!("{PLUGIN_ID_PREFIX}{store_name}")
}

/// DSH `serverName`: `[A-Za-z0-9_-]{1,32}`. The store key is already that
/// charset (see `sanitize_key`); this only enforces the length cap.
fn dsh_server_name(store_name: &str) -> String {
    let cleaned: String = store_name
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '_' || *c == '-')
        .take(SERVER_NAME_MAX)
        .collect();
    if cleaned.is_empty() {
        "mcp".into()
    } else {
        cleaned
    }
}

fn plugin_row(entry: &McpServerEntry) -> Value {
    let mut row = Mapping::new();
    row.insert(yaml_str(ID), yaml_str(&plugin_id(&entry.name)));
    row.insert(yaml_str(NAME), yaml_str(PLUGIN_NAME));
    row.insert(yaml_str(CONFIG), Value::Mapping(dsh_config(entry)));
    Value::Mapping(row)
}

fn dsh_config(entry: &McpServerEntry) -> Mapping {
    let mut config = Mapping::new();
    config.insert(
        yaml_str(SERVER_NAME),
        yaml_str(&dsh_server_name(&entry.name)),
    );
    match entry.transport.as_str() {
        "http" | "sse" => {
            config.insert(yaml_str(TRANSPORT), yaml_str(STREAMABLE_HTTP));
            if let Some(url) = &entry.url {
                config.insert(yaml_str("url"), yaml_str(url));
            }
            if !entry.headers.is_empty() {
                config.insert(yaml_str("headers"), yaml_string_mapping(&entry.headers));
            }
        }
        _ => {
            config.insert(yaml_str(TRANSPORT), yaml_str("stdio"));
            if let Some(cmd) = &entry.command {
                config.insert(yaml_str("command"), yaml_str(cmd));
            }
            if !entry.args.is_empty() {
                config.insert(
                    yaml_str("args"),
                    Value::Sequence(entry.args.iter().map(|s| yaml_str(s)).collect()),
                );
            }
            if !entry.env.is_empty() {
                config.insert(yaml_str("env"), yaml_string_mapping(&entry.env));
            }
            if let Some(cwd) = &entry.cwd {
                config.insert(yaml_str("cwd"), yaml_str(cwd));
            }
        }
    }
    if let Some(ms) = entry.timeout_ms.filter(|&ms| ms > 0) {
        config.insert(
            yaml_str(TOOL_CALL_TIMEOUT_MS),
            Value::Number(serde_yaml::Number::from(ms)),
        );
    }
    config
}

fn entry_from_row(row: &Value) -> Option<McpServerEntry> {
    let map = row.as_mapping()?;
    let config = yaml_map(map, CONFIG)?;
    let store_name = store_name_from_row(map)?;
    let url = yaml_map_str(config, "url");
    let transport_token = yaml_map_str(config, TRANSPORT);
    let remote = transport_token.as_deref() == Some(STREAMABLE_HTTP) || url.is_some();
    let mut entry = blank_entry(&store_name, if remote { "http" } else { "stdio" });
    if remote {
        entry.url = url;
        if let Some(headers) = yaml_map(config, "headers") {
            entry.headers = yaml_string_map(headers);
        }
        entry.url.as_ref()?;
    } else {
        entry.command = yaml_map_str(config, "command");
        if let Some(Value::Sequence(args)) = config.get(Value::String("args".into())) {
            entry.args = args
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect();
        }
        if let Some(env) = yaml_map(config, "env") {
            entry.env = yaml_string_map(env);
        }
        entry.cwd = yaml_map_str(config, "cwd");
        entry.command.as_ref()?;
    }
    if let Some(ms) = config
        .get(Value::String(TOOL_CALL_TIMEOUT_MS.into()))
        .and_then(Value::as_u64)
    {
        entry.timeout_ms = Some(ms);
    }
    Some(entry)
}

fn store_name_from_row(row: &Mapping) -> Option<String> {
    let id = yaml_map_str(row, ID)?;
    if let Some(rest) = id.strip_prefix(PLUGIN_ID_PREFIX)
        && !rest.is_empty()
    {
        return Some(rest.to_string());
    }
    yaml_map(row, CONFIG).and_then(|config| yaml_map_str(config, SERVER_NAME))
}

fn is_mcp_client_row(row: &Value) -> bool {
    row.as_mapping()
        .and_then(|map| yaml_map_str(map, NAME))
        .as_deref()
        == Some(PLUGIN_NAME)
}

/// Whether `row` is the persisted form of `store_name`.
///
/// The plugin `id` is authoritative: SkillStar always writes `mcp-<store name>`,
/// and it is the only key that survives [`dsh_server_name`]'s 32-character cut.
/// Two long store names can share their first [`SERVER_NAME_MAX`] characters,
/// so the truncated `serverName` must **never** be used to match between rows we
/// wrote — doing so let one server overwrite the other on upsert and delete the
/// wrong row on remove. It is kept only as an adoption path for hand-written
/// `insert` rows that carry no `mcp-` id of ours.
fn row_matches(row: &Value, store_name: &str) -> bool {
    let Some(map) = row.as_mapping() else {
        return false;
    };
    let Some(row_id) = yaml_map_str(map, ID) else {
        return adopt_by_server_name(row, store_name);
    };
    if row_id == plugin_id(store_name) {
        return true;
    }
    if row_id.starts_with(PLUGIN_ID_PREFIX) {
        return false;
    }
    adopt_by_server_name(row, store_name)
}

/// Claim a hand-written row (no `id`, or an id that is not one of our
/// `mcp-`-prefixed keys) by its `serverName`.
fn adopt_by_server_name(row: &Value, store_name: &str) -> bool {
    if !is_mcp_client_row(row) {
        return false;
    }
    row.as_mapping()
        .and_then(|map| yaml_map(map, CONFIG))
        .and_then(|config| yaml_map_str(config, SERVER_NAME))
        .as_deref()
        == Some(dsh_server_name(store_name).as_str())
}

fn insert_entries(op: &Value) -> Option<&Vec<Value>> {
    op.as_mapping()?
        .get(Value::String(INSERT.into()))?
        .as_sequence()
}

fn insert_entries_mut(op: &mut Value) -> Option<&mut Vec<Value>> {
    op.as_mapping_mut()?
        .get_mut(Value::String(INSERT.into()))?
        .as_sequence_mut()
}

fn mcp_client_rows(ops: &[Value]) -> impl Iterator<Item = &Value> {
    ops.iter()
        .filter_map(insert_entries)
        .flatten()
        .filter(|row| is_mcp_client_row(row))
}

fn find_row_coords(ops: &[Value], store_name: &str) -> Option<(usize, usize)> {
    for (op_i, op) in ops.iter().enumerate() {
        let Some(entries) = insert_entries(op) else {
            continue;
        };
        for (row_i, row) in entries.iter().enumerate() {
            if row_matches(row, store_name) {
                return Some((op_i, row_i));
            }
        }
    }
    None
}

fn last_mcp_insert_index(ops: &[Value]) -> Option<usize> {
    ops.iter().enumerate().rev().find_map(|(i, op)| {
        insert_entries(op)?
            .iter()
            .any(is_mcp_client_row)
            .then_some(i)
    })
}

fn read_patch_strict(path: &Path) -> Result<Vec<Value>> {
    if !path.exists() {
        return Ok(Vec::new());
    }
    let content = std::fs::read_to_string(path).with_context(|| {
        format!(
            "Failed to read {}. Refusing to rewrite it — check the file's permissions, then retry.",
            path.display()
        )
    })?;
    parse_patch(&content).with_context(|| {
        format!(
            "Invalid YAML in {}. Refusing to overwrite it — fix or move the file, then retry.",
            path.display()
        )
    })
}

fn parse_patch(content: &str) -> Result<Vec<Value>> {
    let content = content.trim_start_matches('\u{FEFF}');
    if content.trim().is_empty() {
        return Ok(Vec::new());
    }
    let value: Value = serde_yaml::from_str(content)?;
    match value {
        Value::Sequence(seq) => Ok(seq),
        _ => bail!("Expected a YAML sequence of Cordis patch operations at the document root"),
    }
}

fn write_yaml(path: &Path, value: &Value) -> Result<()> {
    let out = serde_yaml::to_string(value).context("Failed to serialize YAML config")?;
    atomic_write(path, out.as_bytes())
        .with_context(|| format!("Failed to write {}", path.display()))
}

fn yaml_str(s: &str) -> Value {
    Value::String(s.to_string())
}

fn yaml_map<'a>(obj: &'a Mapping, key: &str) -> Option<&'a Mapping> {
    obj.get(Value::String(key.into()))
        .and_then(Value::as_mapping)
}

fn yaml_map_str(obj: &Mapping, key: &str) -> Option<String> {
    obj.get(Value::String(key.into()))
        .and_then(Value::as_str)
        .map(str::to_string)
}

fn yaml_string_map(map: &Mapping) -> BTreeMap<String, String> {
    map.iter()
        .filter_map(|(k, v)| {
            let key = k.as_str()?;
            let val = v.as_str()?;
            Some((key.to_string(), val.to_string()))
        })
        .collect()
}

fn yaml_string_mapping(map: &BTreeMap<String, String>) -> Value {
    let mut out = Mapping::new();
    for (k, v) in map {
        out.insert(yaml_str(k), yaml_str(v));
    }
    Value::Mapping(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mcp::tests_targets::TempDir;

    fn stdio(name: &str) -> McpServerEntry {
        let mut e = blank_entry(name, "stdio");
        e.command = Some("npx".into());
        e.args = vec!["-y".into(), "example-mcp".into()];
        e.env.insert("API_KEY".into(), "secret".into());
        e.timeout_ms = Some(45_000);
        e
    }

    fn http(name: &str) -> McpServerEntry {
        let mut e = blank_entry(name, "http");
        e.url = Some("https://example.com/mcp".into());
        e.headers
            .insert("Authorization".into(), "Bearer xxx".into());
        e
    }

    #[test]
    fn upsert_writes_a_cordis_insert_for_the_mcp_client_plugin() {
        let dir = TempDir::new("dsh-upsert");
        let path = dir.path().join("cordis.patch.yml");
        dsh_upsert(&path, &stdio("codegraph")).unwrap();

        let content = std::fs::read_to_string(&path).unwrap();
        let root: Value = serde_yaml::from_str(&content).unwrap();
        let row = root
            .as_sequence()
            .unwrap()
            .iter()
            .find_map(|op| {
                op.get("insert")?
                    .as_sequence()?
                    .iter()
                    .find(|row| row.get("id").and_then(Value::as_str) == Some("mcp-codegraph"))
            })
            .unwrap();
        assert_eq!(row.get("name").and_then(Value::as_str), Some(PLUGIN_NAME));
        let config = row.get("config").unwrap();
        assert_eq!(
            config.get("serverName").and_then(Value::as_str),
            Some("codegraph")
        );
        assert_eq!(
            config.get("transport").and_then(Value::as_str),
            Some("stdio")
        );
        assert!(config.get("type").is_none(), "{config:?}");
        assert_eq!(
            config.get("toolCallTimeoutMs").and_then(Value::as_u64),
            Some(45_000)
        );
        assert_eq!(
            config.get("env").and_then(Value::as_mapping).unwrap()
                [&Value::String("API_KEY".into())],
            Value::String("secret".into())
        );
    }

    #[test]
    fn http_uses_streamable_http_and_a_url() {
        let dir = TempDir::new("dsh-http");
        let path = dir.path().join("cordis.patch.yml");
        dsh_upsert(&path, &http("github")).unwrap();
        let read = read_dsh_entries(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(read[0].name, "github");
        assert_eq!(read[0].transport, "http");
        assert_eq!(read[0].url.as_deref(), Some("https://example.com/mcp"));
        assert_eq!(
            read[0].headers.get("Authorization").map(String::as_str),
            Some("Bearer xxx")
        );

        let root: Value = serde_yaml::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        let config = root.as_sequence().unwrap()[0]
            .get("insert")
            .unwrap()
            .as_sequence()
            .unwrap()[0]
            .get("config")
            .unwrap();
        assert_eq!(
            config.get("transport").and_then(Value::as_str),
            Some(STREAMABLE_HTTP)
        );
    }

    #[test]
    fn remove_drops_the_mcp_row_but_keeps_sibling_plugins() {
        let dir = TempDir::new("dsh-remove");
        let path = dir.path().join("cordis.patch.yml");
        std::fs::write(
            &path,
            "- insert:\n    - id: hello\n      name: dsh-hello-plugin\n    - id: mcp-gone\n      name: '@deepseek-ai/dsh-mcp-client'\n      config:\n        serverName: gone\n        transport: stdio\n        command: npx\n    - id: mcp-keep\n      name: '@deepseek-ai/dsh-mcp-client'\n      config:\n        serverName: keep\n        transport: stdio\n        command: uvx\n",
        )
        .unwrap();

        dsh_remove(&path, "gone").unwrap();
        let content = std::fs::read_to_string(&path).unwrap();
        assert!(!content.contains("mcp-gone"));
        assert!(content.contains("mcp-keep"));
        assert!(content.contains("dsh-hello-plugin"));
        let names: Vec<_> = read_dsh_entries(&content)
            .unwrap()
            .into_iter()
            .map(|e| e.name)
            .collect();
        assert_eq!(names, vec!["keep".to_string()]);
    }

    #[test]
    fn a_non_sequence_root_is_refused() {
        let dir = TempDir::new("dsh-malformed");
        let path = dir.path().join("cordis.patch.yml");
        let original = "mcp_servers:\n  x:\n    command: npx\n";
        std::fs::write(&path, original).unwrap();
        let err = dsh_upsert(&path, &stdio("x")).unwrap_err();
        assert!(
            err.to_string().contains("Refusing") || err.to_string().contains("sequence"),
            "{err}"
        );
        assert_eq!(std::fs::read_to_string(&path).unwrap(), original);
    }

    #[test]
    fn a_long_store_name_round_trips_via_plugin_id_while_server_name_stays_32() {
        let dir = TempDir::new("dsh-long-name");
        let path = dir.path().join("cordis.patch.yml");
        let name = "io-github-modelcontextprotocol-server-filesystem";
        assert!(name.len() > SERVER_NAME_MAX);
        dsh_upsert(&path, &stdio(name)).unwrap();

        let content = std::fs::read_to_string(&path).unwrap();
        let read = read_dsh_entries(&content).unwrap();
        assert_eq!(read[0].name, name);

        let root: Value = serde_yaml::from_str(&content).unwrap();
        let config = root.as_sequence().unwrap()[0]
            .get("insert")
            .unwrap()
            .as_sequence()
            .unwrap()[0]
            .get("config")
            .unwrap();
        let server_name = config.get("serverName").and_then(Value::as_str).unwrap();
        assert_eq!(server_name.len(), SERVER_NAME_MAX);
        assert_eq!(server_name, &name[..SERVER_NAME_MAX]);
    }

    /// Two long registry names can share their first `SERVER_NAME_MAX`
    /// characters. The truncated `serverName` is then identical for both, so
    /// matching on it would make the second upsert overwrite the first row and
    /// the first remove delete the wrong one. The `mcp-<name>` plugin id is the
    /// only key that tells them apart.
    #[test]
    fn two_long_names_sharing_the_truncated_server_name_do_not_collide() {
        let dir = TempDir::new("dsh-collision");
        let path = dir.path().join("cordis.patch.yml");
        let a = "io-github-modelcontextprotocol-server-filesystem";
        let b = "io-github-modelcontextprotocol-server-everything";
        assert_eq!(
            dsh_server_name(a),
            dsh_server_name(b),
            "the premise: both names truncate to the same serverName"
        );

        dsh_upsert(&path, &stdio(a)).unwrap();
        dsh_upsert(&path, &stdio(b)).unwrap();

        let content = std::fs::read_to_string(&path).unwrap();
        let names: Vec<String> = read_dsh_entries(&content)
            .unwrap()
            .into_iter()
            .map(|entry| entry.name)
            .collect();
        assert_eq!(names, vec![a.to_string(), b.to_string()], "{content}");
        assert_eq!(count_dsh_mcp(&content), 2);

        dsh_remove(&path, a).unwrap();
        let names: Vec<String> = read_dsh_entries(&std::fs::read_to_string(&path).unwrap())
            .unwrap()
            .into_iter()
            .map(|entry| entry.name)
            .collect();
        assert_eq!(names, vec![b.to_string()]);
    }

    /// The `serverName` fallback still exists, but only for a hand-written row
    /// that carries none of our `mcp-` ids. Tightening the match must not turn
    /// an adopted row into a duplicate.
    #[test]
    fn a_hand_written_row_without_our_id_is_still_adopted() {
        let dir = TempDir::new("dsh-adopt");
        let path = dir.path().join("cordis.patch.yml");
        std::fs::write(
            &path,
            "- insert:\n    - name: '@deepseek-ai/dsh-mcp-client'\n      config:\n        serverName: codegraph\n        transport: stdio\n        command: npx\n",
        )
        .unwrap();

        dsh_upsert(&path, &stdio("codegraph")).unwrap();

        let content = std::fs::read_to_string(&path).unwrap();
        assert_eq!(
            count_dsh_mcp(&content),
            1,
            "the hand-written row must be adopted, not duplicated: {content}"
        );
        assert!(content.contains("mcp-codegraph"), "{content}");
    }
}
