pub mod featured;
pub mod registry;
pub mod snapshot;

use crate::error::McpHubResult;
use crate::formats::paste::sanitize_slug;
use crate::library;
use crate::model::{
    ConflictStrategy, LibraryDelta, LibrarySnapshot, McpServer, StoreInstallRequest, StoreItem,
    StoreSearchResponse,
};
use std::collections::BTreeSet;

/// Query featured servers, embedded `market_servers.json` snapshot, and (when `online = true`)
/// the live official MCP Registry with graceful offline fallback.
pub async fn query_store(
    query: &str,
    online: bool,
    limit: usize,
) -> McpHubResult<StoreSearchResponse> {
    let existing_servers = tokio::task::spawn_blocking(|| {
        library::read_library_unlocked()
            .map(|l| l.servers)
            .unwrap_or_default()
    })
    .await
    .unwrap_or_default();

    let limit = limit.clamp(1, 100);
    let q = query.trim();
    let q_lower = q.to_lowercase();

    let mut out: Vec<StoreItem> = Vec::new();
    let mut seen_ids: BTreeSet<String> = BTreeSet::new();

    // 1. Featured curated external servers
    for mut item in featured::featured_servers() {
        let matches = q_lower.is_empty()
            || item.id.to_lowercase().contains(&q_lower)
            || item.title.to_lowercase().contains(&q_lower)
            || item.description.to_lowercase().contains(&q_lower)
            || item.publisher.to_lowercase().contains(&q_lower)
            || item.homepage.to_lowercase().contains(&q_lower);
        if matches && seen_ids.insert(item.id.clone()) {
            attach_have_marker(&mut item, &existing_servers);
            out.push(item);
        }
    }

    // 2. Embedded offline snapshot (`market_servers.json`)
    if out.len() < limit {
        for mut item in snapshot::search_snapshot(q, limit) {
            if seen_ids.insert(item.id.clone())
                && !out
                    .iter()
                    .any(|f| f.name == item.name && f.featured)
            {
                attach_have_marker(&mut item, &existing_servers);
                out.push(item);
                if out.len() >= limit {
                    break;
                }
            }
        }
    }

    // 3. Optional live online MCP Registry search (also runs GitHub/PyPI/URL/command fallback)
    let mut online_used = false;
    let mut online_error = None;
    if online && !q.is_empty() && out.len() < limit {
        match registry::search_online_outcome(q, limit).await {
            Ok(outcome) => {
                online_used = outcome.online_used;
                online_error = outcome.online_error;
                for mut item in outcome.items {
                    if seen_ids.insert(item.id.clone()) {
                        attach_have_marker(&mut item, &existing_servers);
                        out.push(item);
                        if out.len() >= limit {
                            break;
                        }
                    }
                }
            }
            Err(err) => {
                online_error = Some(err.to_string());
            }
        }
    }

    // 4. Offline-capable direct address / command fallback (`npx`, `uvx`, `npm:`, `custom_at` URL)
    // Always evaluate even in offline mode or when Registry errored/skipped.
    if !q.is_empty()
        && out.len() < limit
        && let Some(mut direct) = registry::resolve_offline_direct_address(q)
    {
        let already_present = out.iter().any(|existing| {
            existing.id == direct.id
                || (!direct.template.url.is_empty()
                    && existing.template.url == direct.template.url)
                || (!direct.template.command.is_empty()
                    && existing.template.command == direct.template.command
                    && existing.template.args == direct.template.args)
        });
        if !already_present && seen_ids.insert(direct.id.clone()) {
            attach_have_marker(&mut direct, &existing_servers);
            out.push(direct);
        }
    }

    out.truncate(limit);
    Ok(StoreSearchResponse {
        items: out,
        online_used,
        online_error,
    })
}

fn attach_have_marker(item: &mut StoreItem, installed: &[McpServer]) {
    if let Some(found) = installed.iter().find(|s| {
        s.market_id.as_deref() == Some(item.id.as_str())
            || s.name == item.name
            || (!item.template.url.is_empty() && s.url == item.template.url)
    }) {
        item.have = Some(found.name.clone());
    }
}

fn materialize_store_server(req: StoreInstallRequest) -> McpHubResult<McpServer> {
    let mut srv = req.item.template.clone();
    if let Some(custom_name) = req.name.as_deref().filter(|s| !s.trim().is_empty()) {
        srv.name = sanitize_slug(custom_name);
        if srv.name.is_empty() {
            srv.name = req.item.name.clone();
        }
    }
    srv.market_id = Some(req.item.id.clone());
    srv.agents = req.agents;

    for input in &req.item.inputs {
        let val = req
            .values
            .get(&input.key)
            .map(String::as_str)
            .unwrap_or("")
            .trim();
        if val.is_empty() {
            if input.required {
                let label = if input.label.trim().is_empty() {
                    input.key.as_str()
                } else {
                    input.label.as_str()
                };
                return Err(crate::error::McpHubError::InvalidServer(format!(
                    "missing required store input: {label} ({})",
                    input.key
                )));
            }
            continue;
        }
        let formatted = if !input.format.is_empty() && input.format.contains("{}") {
            input.format.replace("{}", val)
        } else {
            val.to_string()
        };
        match input.where_field.as_str() {
            "env" => {
                srv.env.insert(input.key.clone(), formatted);
            }
            "header" => {
                srv.headers.insert(input.key.clone(), formatted);
            }
            "arg" => {
                for tok in formatted.split_whitespace() {
                    srv.args.push(tok.to_string());
                }
            }
            _ => {}
        }
    }

    Ok(srv)
}

/// Apply user-filled `StoreInputSpec` values (`env` / `header` / `arg`) to a store template,
/// save it to `library.json`, and sync the chosen agents.
pub fn install_store_server(req: StoreInstallRequest) -> McpHubResult<LibrarySnapshot> {
    let srv = materialize_store_server(req)?;
    library::save_server(srv, None, ConflictStrategy::Rename)
}

/// Apply store template inputs, save, and return only `{ servers, warnings }`.
pub fn install_store_server_delta(req: StoreInstallRequest) -> McpHubResult<LibraryDelta> {
    let srv = materialize_store_server(req)?;
    library::save_server_delta(srv, None, ConflictStrategy::Rename)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{StoreInputSpec, Transport};
    use std::collections::BTreeMap;

    #[test]
    fn install_rejects_empty_required_input_and_allows_present_input() {
        let item = StoreItem {
            id: "test:secret".into(),
            name: "secret-srv".into(),
            title: "Secret Server".into(),
            description: String::new(),
            publisher: "Test".into(),
            icon: String::new(),
            homepage: String::new(),
            transport: Transport::Stdio,
            runs: "npx".into(),
            sign_in: false,
            featured: false,
            match_by_addr: false,
            source: "featured".into(),
            stars: None,
            have: None,
            inputs: vec![StoreInputSpec {
                key: "API_KEY".into(),
                where_field: "env".into(),
                label: "API Key".into(),
                description: String::new(),
                placeholder: String::new(),
                secret: true,
                required: true,
                format: String::new(),
            }],
            template: McpServer {
                name: "secret-srv".into(),
                transport: Transport::Stdio,
                command: "npx".into(),
                args: vec!["-y".into(), "secret-srv".into()],
                ..Default::default()
            },
        };

        let missing = StoreInstallRequest {
            item: item.clone(),
            name: None,
            values: BTreeMap::from([("API_KEY".into(), "   ".into())]),
            agents: vec![],
        };
        let err = install_store_server(missing).unwrap_err();
        assert!(err.to_string().contains("API Key"), "{err}");

        let present = StoreInstallRequest {
            item,
            name: None,
            values: BTreeMap::from([("API_KEY".into(), "sk-123".into())]),
            agents: vec![],
        };
        let srv = materialize_store_server(present).unwrap();
        assert_eq!(srv.env.get("API_KEY").map(String::as_str), Some("sk-123"));
    }

    #[tokio::test]
    async fn query_store_offline_resolves_custom_url_npm_npx_and_uvx() {
        for q in [
            "https://offline-mcp.example.com/mcp",
            "npm:my-mcp-pkg",
            "npx -y @org/demo-server",
            "uvx mcp-server-demo",
        ] {
            let res = query_store(q, false, 10).await.unwrap();
            assert!(!res.online_used, "offline query {q}");
            assert!(!res.items.is_empty(), "expected fallback item for {q}");
        }
    }
}
