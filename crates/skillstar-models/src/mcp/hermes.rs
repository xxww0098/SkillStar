//! Hermes Agent MCP projection — `$HERMES_HOME/config.yaml`.
//!
//! Hermes is the one public target that is YAML rather than JSON/TOML. It
//! stores servers under top-level `mcp_servers` and only exposes their tools
//! when `platform_toolsets.cli` lists `mcp-<name>`. Both keys are touched on
//! write; a parse failure is refused rather than rewritten.

use anyhow::{bail, Context, Result};
use serde_yaml::{Mapping, Value};
use std::collections::BTreeMap;
use std::path::Path;

use super::{blank_entry, McpServerEntry};

const MCP_SERVERS: &str = "mcp_servers";
const PLATFORM_TOOLSETS: &str = "platform_toolsets";
const CLI: &str = "cli";

/// Hermes YAML value for one server: `command`/`args`/`env` locally, `url` /
/// `headers` remotely. `enabled: true` is always written because SkillStar
/// removes an entry it should not project rather than flipping Hermes' toggle.
/// `timeout` is seconds when the store has a millisecond value.
pub(crate) fn hermes_spec(entry: &McpServerEntry) -> Value {
    let mut map = Mapping::new();
    match entry.transport.as_str() {
        "http" | "sse" => {
            if let Some(url) = &entry.url {
                map.insert(yaml_str("url"), yaml_str(url));
            }
            if !entry.headers.is_empty() {
                let mut headers = Mapping::new();
                for (k, v) in &entry.headers {
                    headers.insert(yaml_str(k), yaml_str(v));
                }
                map.insert(yaml_str("headers"), Value::Mapping(headers));
            }
        }
        _ => {
            if let Some(cmd) = &entry.command {
                map.insert(yaml_str("command"), yaml_str(cmd));
            }
            if !entry.args.is_empty() {
                map.insert(
                    yaml_str("args"),
                    Value::Sequence(entry.args.iter().map(|s| yaml_str(s)).collect()),
                );
            }
            if !entry.env.is_empty() {
                let mut env = Mapping::new();
                for (k, v) in &entry.env {
                    env.insert(yaml_str(k), yaml_str(v));
                }
                map.insert(yaml_str("env"), Value::Mapping(env));
            }
            if let Some(cwd) = &entry.cwd {
                map.insert(yaml_str("cwd"), yaml_str(cwd));
            }
        }
    }
    map.insert(yaml_str("enabled"), Value::Bool(true));
    if let Some(ms) = entry.timeout_ms.filter(|&ms| ms > 0) {
        let secs = (ms.div_ceil(1000)).max(1);
        map.insert(
            yaml_str("timeout"),
            Value::Number(serde_yaml::Number::from(secs as i64)),
        );
    }
    Value::Mapping(map)
}

pub(crate) fn count_hermes_mcp(content: &str) -> usize {
    parse_mapping(content)
        .ok()
        .and_then(|root| {
            root.get(Value::String(MCP_SERVERS.into()))
                .and_then(Value::as_mapping)
                .map(Mapping::len)
        })
        .unwrap_or(0)
}

pub(crate) fn read_hermes_entries(content: &str) -> Result<Vec<McpServerEntry>> {
    let root = parse_mapping(content)?;
    let Some(servers) = root
        .get(Value::String(MCP_SERVERS.into()))
        .and_then(Value::as_mapping)
    else {
        return Ok(Vec::new());
    };
    let mut out = Vec::new();
    for (key, val) in servers {
        let Some(name) = key.as_str() else { continue };
        if let Some(entry) = entry_from_hermes(name, val) {
            out.push(entry);
        }
    }
    Ok(out)
}

pub(crate) fn hermes_upsert(path: &Path, name: &str, spec: Value) -> Result<()> {
    let mut root = read_yaml_mapping_strict(path)?;
    let servers = root
        .entry(yaml_str(MCP_SERVERS))
        .or_insert_with(|| Value::Mapping(Mapping::new()));
    let map = servers.as_mapping_mut().with_context(|| {
        format!(
            "Expected `{MCP_SERVERS}` to be a YAML mapping in {}. Refusing to overwrite the existing value.",
            path.display()
        )
    })?;
    map.insert(yaml_str(name), spec);
    upsert_cli_toolset(&mut root, name);
    write_yaml(path, &Value::Mapping(root))
}

pub(crate) fn hermes_remove(path: &Path, name: &str) -> Result<()> {
    if !path.exists() {
        return Ok(());
    }
    let mut root = read_yaml_mapping_strict(path)?;
    if let Some(servers) = root.get_mut(Value::String(MCP_SERVERS.into())) {
        let map = servers.as_mapping_mut().with_context(|| {
            format!(
                "Expected `{MCP_SERVERS}` to be a YAML mapping in {}. Refusing to overwrite the existing value.",
                path.display()
            )
        })?;
        map.remove(Value::String(name.into()));
    }
    remove_cli_toolset(&mut root, name);
    write_yaml(path, &Value::Mapping(root))
}

fn entry_from_hermes(name: &str, spec: &Value) -> Option<McpServerEntry> {
    let obj = spec.as_mapping()?;
    let url = yaml_map_str(obj, "url");
    let transport = if url.is_some() { "http" } else { "stdio" };
    let mut entry = blank_entry(name, transport);
    if let Some(url) = url {
        entry.url = Some(url);
        if let Some(headers) = yaml_map(obj, "headers") {
            entry.headers = yaml_string_map(headers);
        }
        entry.url.as_ref()?;
    } else {
        entry.command = yaml_map_str(obj, "command");
        if let Some(Value::Sequence(args)) = obj.get(Value::String("args".into())) {
            entry.args = args
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect();
        }
        if let Some(env) = yaml_map(obj, "env") {
            entry.env = yaml_string_map(env);
        }
        entry.cwd = yaml_map_str(obj, "cwd");
        entry.command.as_ref()?;
    }
    if let Some(secs) = obj
        .get(Value::String("timeout".into()))
        .and_then(Value::as_u64)
    {
        entry.timeout_ms = Some(secs.saturating_mul(1000));
    }
    Some(entry)
}

fn upsert_cli_toolset(root: &mut Mapping, name: &str) {
    let token = format!("mcp-{name}");
    let toolsets = root
        .entry(yaml_str(PLATFORM_TOOLSETS))
        .or_insert_with(|| Value::Mapping(Mapping::new()));
    let Some(map) = toolsets.as_mapping_mut() else {
        return;
    };
    let cli = map
        .entry(yaml_str(CLI))
        .or_insert_with(|| Value::Sequence(Vec::new()));
    let Some(list) = cli.as_sequence_mut() else {
        return;
    };
    if list
        .iter()
        .any(|item| item.as_str() == Some(token.as_str()))
    {
        return;
    }
    list.push(yaml_str(&token));
}

fn remove_cli_toolset(root: &mut Mapping, name: &str) {
    let token = format!("mcp-{name}");
    let Some(toolsets) = root.get_mut(Value::String(PLATFORM_TOOLSETS.into())) else {
        return;
    };
    let Some(map) = toolsets.as_mapping_mut() else {
        return;
    };
    let Some(cli) = map.get_mut(Value::String(CLI.into())) else {
        return;
    };
    let Some(list) = cli.as_sequence_mut() else {
        return;
    };
    list.retain(|item| item.as_str() != Some(token.as_str()));
}

fn read_yaml_mapping_strict(path: &Path) -> Result<Mapping> {
    if !path.exists() {
        return Ok(Mapping::new());
    }
    let content = std::fs::read_to_string(path).with_context(|| {
        format!(
            "Failed to read {}. Refusing to rewrite it — check the file's permissions, then retry.",
            path.display()
        )
    })?;
    parse_mapping(&content).with_context(|| {
        format!(
            "Invalid YAML in {}. Refusing to overwrite it — fix or move the file, then retry.",
            path.display()
        )
    })
}

fn parse_mapping(content: &str) -> Result<Mapping> {
    let content = content.trim_start_matches('\u{FEFF}');
    if content.trim().is_empty() {
        return Ok(Mapping::new());
    }
    let value: Value = serde_yaml::from_str(content)?;
    match value {
        Value::Mapping(map) => Ok(map),
        _ => bail!("Expected a YAML mapping at the document root"),
    }
}

fn write_yaml(path: &Path, value: &Value) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("Failed to create directory {}", parent.display()))?;
    }
    let out = serde_yaml::to_string(value).context("Failed to serialize YAML config")?;
    std::fs::write(path, out).with_context(|| format!("Failed to write {}", path.display()))?;
    Ok(())
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mcp::tests_targets::TempDir;

    fn stdio(name: &str) -> McpServerEntry {
        let mut e = blank_entry(name, "stdio");
        e.command = Some("npx".into());
        e.args = vec!["-y".into(), "example-mcp".into()];
        e
    }

    #[test]
    fn upsert_writes_mcp_servers_and_the_cli_toolset_token() {
        let dir = TempDir::new("hermes-upsert");
        let path = dir.path().join("config.yaml");
        hermes_upsert(&path, "codegraph", hermes_spec(&stdio("codegraph"))).unwrap();

        let content = std::fs::read_to_string(&path).unwrap();
        let root: Value = serde_yaml::from_str(&content).unwrap();
        let servers = root.get("mcp_servers").and_then(Value::as_mapping).unwrap();
        let entry = servers
            .get(Value::String("codegraph".into()))
            .unwrap()
            .as_mapping()
            .unwrap();
        assert_eq!(entry.get(Value::String("command".into())).unwrap(), "npx");
        assert!(entry.get(Value::String("type".into())).is_none());
        assert_eq!(
            entry.get(Value::String("enabled".into())),
            Some(&Value::Bool(true))
        );

        let cli = root
            .get("platform_toolsets")
            .and_then(Value::as_mapping)
            .unwrap()
            .get(Value::String("cli".into()))
            .and_then(Value::as_sequence)
            .unwrap();
        assert!(cli.iter().any(|v| v.as_str() == Some("mcp-codegraph")));
    }

    #[test]
    fn remove_drops_the_server_and_its_toolset_token_but_keeps_siblings() {
        let dir = TempDir::new("hermes-remove");
        let path = dir.path().join("config.yaml");
        std::fs::write(
            &path,
            "other: 1\nmcp_servers:\n  keep:\n    command: uvx\n  gone:\n    command: npx\nplatform_toolsets:\n  cli:\n    - hermes-cli\n    - mcp-keep\n    - mcp-gone\n",
        )
        .unwrap();

        hermes_remove(&path, "gone").unwrap();
        let content = std::fs::read_to_string(&path).unwrap();
        assert!(!content.contains("mcp-gone"));
        assert!(content.contains("mcp-keep"));
        assert!(content.contains("hermes-cli"));
        assert!(content.contains("keep:"));
        assert!(content.contains("other:"));
    }

    #[test]
    fn a_non_mapping_root_is_refused() {
        let dir = TempDir::new("hermes-malformed");
        let path = dir.path().join("config.yaml");
        let original = "- not\n- a\n- mapping\n";
        std::fs::write(&path, original).unwrap();
        let err = hermes_upsert(&path, "x", hermes_spec(&stdio("x"))).unwrap_err();
        assert!(
            err.to_string().contains("Refusing") || err.to_string().contains("mapping"),
            "{err}"
        );
        assert_eq!(std::fs::read_to_string(&path).unwrap(), original);
    }
}
