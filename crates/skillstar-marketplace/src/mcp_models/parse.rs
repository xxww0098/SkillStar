//! Wire → snapshot parsing for MCP registry list responses.
//!
//! One parser serves every source we consume, because they all speak the same
//! envelope: `{ servers: [{ server, _meta, x-github? }], metadata: { … } }`.
//! The two things that genuinely differ are the pagination key's spelling
//! (`nextCursor` on the official registry, `next_cursor` on GitHub's) and which
//! optional `_meta` blocks are populated — both handled here rather than by
//! forking the parser per source.

use serde_json::Value;
use ts_rs::TS;

use super::raw::{bool_field, obj_field, str_field};
use super::spec::{McpIcon, McpServerStatus, parse_packages, parse_remotes};
use super::{McpRegistryServer, McpServerKind};

/// Registry-hosted metadata block (`_meta` → status / isLatest / timestamps).
const OFFICIAL_META_KEY: &str = "io.modelcontextprotocol.registry/official";
/// Publisher-supplied metadata block; GitHub's mirror hangs its `github`
/// enrichment (stars, license, readme) off it.
const PUBLISHER_META_KEY: &str = "io.modelcontextprotocol.registry/publisher-provided";

/// Which spelling of the pagination cursor a source uses.
///
/// The official registry returns `metadata.nextCursor` and GitHub's mirror
/// returns `metadata.next_cursor`. Reading only one spelling silently stops
/// after page 1 — the catalog looks complete and is not.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "McpCursorStyle.ts")]
pub enum McpCursorStyle {
    /// `metadata.nextCursor` (official registry, `2025-12-11` era).
    #[default]
    Camel,
    /// `metadata.next_cursor` (GitHub MCP Registry).
    Snake,
}

impl McpCursorStyle {
    /// Preferred key first, the other spelling as a tolerated fallback: a
    /// source that changes its casing degrades to "still works" instead of
    /// "silently returns one page".
    fn keys(self) -> [&'static str; 2] {
        match self {
            McpCursorStyle::Camel => ["nextCursor", "next_cursor"],
            McpCursorStyle::Snake => ["next_cursor", "nextCursor"],
        }
    }
}

/// One parsed page of a registry listing.
#[derive(Debug, Clone, Default)]
pub struct McpRegistryPage {
    pub servers: Vec<McpRegistryServer>,
    pub next_cursor: Option<String>,
    /// `metadata.total` when the source reports it (GitHub does; the official
    /// registry reports only `count`). Used to detect truncation.
    pub total: Option<u64>,
}

/// Last `/`-separated segment of a registry name, used as the default config key.
fn clean_name(namespace: &str) -> String {
    namespace
        .rsplit('/')
        .next()
        .unwrap_or(namespace)
        .trim()
        .to_string()
}

/// Extract the `server` object from a registry list element. The API has
/// shipped both `{ server: {...}, _meta }` and the bare `server.json`.
fn server_object(element: &Value) -> Option<&Value> {
    match element.get("server") {
        Some(inner) if inner.is_object() => Some(inner),
        _ if element.is_object() => Some(element),
        _ => None,
    }
}

/// The registry-hosted `_meta` block, wherever this source puts it.
fn official_meta<'a>(element: &'a Value, server: &'a Value) -> Option<&'a Value> {
    for root in [element, server] {
        if let Some(meta) = root.get("_meta").and_then(|m| m.get(OFFICIAL_META_KEY)) {
            return Some(meta);
        }
    }
    None
}

/// GitHub's enrichment block, in any of the shapes it has shipped in.
fn github_meta<'a>(element: &'a Value, server: &'a Value) -> Option<&'a Value> {
    element
        .get("x-github")
        .or_else(|| server.get("x-github"))
        .or_else(|| {
            server
                .get("_meta")
                .and_then(|m| m.get(PUBLISHER_META_KEY))
                .and_then(|p| p.get("github"))
        })
        .or_else(|| {
            element
                .get("_meta")
                .and_then(|m| m.get(PUBLISHER_META_KEY))
                .and_then(|p| p.get("github"))
        })
        .or_else(|| official_meta(element, server))
}

fn parse_stars_and_license(element: &Value, server: &Value) -> (u32, Option<String>) {
    let gh = github_meta(element, server);
    let stars = gh
        .and_then(|g| {
            g.get("stars")
                .or_else(|| g.get("github_stars"))
                .or_else(|| g.get("stargazers_count"))
                .or_else(|| g.get("stargazerCount"))
        })
        .and_then(Value::as_u64)
        .unwrap_or(0) as u32;
    let license = gh.and_then(|g| {
        g.get("license").and_then(|l| {
            l.as_str()
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(str::to_string)
                .or_else(|| str_field(l, &["spdxId", "name", "key"]))
        })
    });
    (stars, license)
}

/// Parse one registry list element into a normalized [`McpRegistryServer`].
/// Returns `None` only when the element lacks a usable name.
pub fn parse_registry_element(element: &Value) -> Option<McpRegistryServer> {
    parse_registry_element_from(element, None)
}

/// Same as [`parse_registry_element`], stamping which source produced the row.
pub fn parse_registry_element_from(
    element: &Value,
    source_id: Option<&str>,
) -> Option<McpRegistryServer> {
    let server = server_object(element)?;
    let namespace = str_field(server, &["name"])?;
    let name = clean_name(&namespace);
    if name.is_empty() {
        return None;
    }

    let id = str_field(server, &["id"]).unwrap_or_else(|| namespace.clone());
    let description = str_field(server, &["description"]).unwrap_or_default();
    let repository = obj_field(server, &["repository"]);
    let repo_url = repository
        .and_then(|r| str_field(r, &["url"]))
        .unwrap_or_default();
    let meta = official_meta(element, server);
    let gh = github_meta(element, server);
    let readme = repository
        .and_then(|r| str_field(r, &["readme"]))
        .or_else(|| gh.and_then(|g| str_field(g, &["readme"])));
    let version = obj_field(server, &["version_detail", "versionDetail"])
        .and_then(|v| str_field(v, &["version"]))
        .or_else(|| str_field(server, &["version"]));

    let published_at = meta.and_then(|m| str_field(m, &["publishedAt", "published_at"]));
    let updated_at = meta
        .and_then(|m| str_field(m, &["updatedAt", "updated_at"]))
        .or_else(|| str_field(server, &["updated_at", "updatedAt"]))
        .or_else(|| str_field(server, &["created_at", "createdAt"]))
        .or_else(|| published_at.clone());

    let status = str_field(server, &["status"])
        .or_else(|| meta.and_then(|m| str_field(m, &["status"])))
        .map(|s| McpServerStatus::from_db_str(&s))
        .unwrap_or_default();
    // Absent `isLatest` means "the source does not version its rows" (GitHub's
    // mirror, our curated seeds) — those rows are the only ones we have, so the
    // safe default is `true`. A `false` is only ever set by a source that
    // explicitly said so, which is what the dedup pass keys on.
    let is_latest = meta
        .and_then(|m| bool_field(m, &["isLatest", "is_latest"]))
        .unwrap_or(true);

    let packages = parse_packages(server);
    let remotes = parse_remotes(server);
    let kind = match (packages.is_empty(), remotes.is_empty()) {
        (false, false) => McpServerKind::Both,
        (false, true) => McpServerKind::Stdio,
        (true, false) => McpServerKind::Remote,
        (true, true) => McpServerKind::Unknown,
    };
    let mut runtimes: Vec<String> = Vec::new();
    for pkg in &packages {
        if !pkg.runtime.is_empty() && !runtimes.contains(&pkg.runtime) {
            runtimes.push(pkg.runtime.clone());
        }
    }

    let (stars, license) = parse_stars_and_license(element, server);
    let raw_server_json = serde_json::to_string(server).unwrap_or_default();

    Some(McpRegistryServer {
        id,
        name,
        namespace,
        description,
        repo_url,
        stars,
        license,
        version,
        kind,
        runtimes,
        readme,
        updated_at,
        packages,
        remotes,
        raw_server_json,
        recommended: false,
        source: None,
        title: str_field(server, &["title"]),
        website_url: str_field(server, &["websiteUrl", "website_url"]),
        icons: McpIcon::parse_list(server),
        status,
        is_latest,
        published_at,
        registry_source: source_id.map(str::to_string),
        contributing_sources: source_id.map(|s| vec![s.to_string()]).unwrap_or_default(),
    })
}

/// Parse one `/servers` page, honouring the source's cursor spelling.
pub fn parse_servers_page(
    body: &str,
    cursor_style: McpCursorStyle,
    source_id: Option<&str>,
) -> anyhow::Result<McpRegistryPage> {
    let root: Value = serde_json::from_str(body)
        .map_err(|e| anyhow::anyhow!("Failed to parse MCP registry response: {e}"))?;

    // `servers` is the documented envelope; a bare array is what a hand-made
    // local directory file most naturally looks like, so accept both.
    let elements: Vec<Value> = match root.get("servers").and_then(Value::as_array) {
        Some(arr) => arr.clone(),
        None => root.as_array().cloned().unwrap_or_default(),
    };

    let servers: Vec<McpRegistryServer> = elements
        .iter()
        .filter_map(|element| parse_registry_element_from(element, source_id))
        .collect();

    let metadata = root.get("metadata");
    let next_cursor = metadata.and_then(|m| {
        let keys = cursor_style.keys();
        str_field(m, &keys)
    });
    let total = metadata.and_then(|m| {
        m.get("total")
            .or_else(|| m.get("totalCount"))
            .or_else(|| m.get("total_count"))
            .and_then(Value::as_u64)
    });

    Ok(McpRegistryPage {
        servers,
        next_cursor,
        total,
    })
}

/// Parse a full `/servers` response body into normalized servers plus the
/// pagination cursor. Tolerant of either cursor spelling.
pub fn parse_servers_response(
    body: &str,
) -> anyhow::Result<(Vec<McpRegistryServer>, Option<String>)> {
    let page = parse_servers_page(body, McpCursorStyle::Snake, None)?;
    Ok((page.servers, page.next_cursor))
}
