//! Card listing / search / filtering over the two symmetric MCP tables.
//!
//! One builder produces every card read. The curated and registry branches are
//! `UNION ALL`-ed with an identical projection — if the two column lists ever
//! drift the query fails at prepare time, which is why they are emitted from
//! the same constants rather than written out twice by hand.

use anyhow::{Context, Result};
use rusqlite::types::Value as SqlValue;
use rusqlite::{Connection, params_from_iter};

use crate::mcp_models::{McpIcon, McpMarketEntry, McpServerKind, McpServerStatus};
use crate::mcp_snapshot::filters::{McpServerPage, McpServerQuery};

/// Projection returned to the caller. `sort_priority` / `search_rank` are
/// ordering-only and deliberately excluded here.
const CARD_COLUMNS: &str = "id, name, namespace, description, repo_url, stars, license, version, \
     kind, runtimes_json, updated_at, recommended, source, title, website_url, icons_json, \
     status, is_latest, published_at, registry_source";

/// bm25 weights: id ignored, name ≫ namespace ≫ description.
const BM25_WEIGHTS: &str = "0.0, 8.0, 4.0, 2.0";

fn curated_branch(ranked: bool, publisher: Option<&str>, params: &mut Vec<SqlValue>) -> String {
    let rank = if ranked {
        format!("bm25(mcp_curated_server_fts, {BM25_WEIGHTS})")
    } else {
        "0.0".to_string()
    };
    let mut sql = format!(
        "SELECT c.id, c.name, c.namespace, c.description, c.repo_url, c.stars, c.license, c.version,
                c.kind, c.runtimes_json, c.updated_at,
                c.is_recommended AS recommended, c.source,
                c.title, c.website_url, c.icons_json, c.status, c.is_latest, c.published_at,
                c.registry_source,
                c.priority AS sort_priority,
                {rank} AS search_rank
         FROM mcp_curated_server c"
    );
    let mut conditions: Vec<String> = Vec::new();
    if ranked {
        sql.push_str(" JOIN mcp_curated_server_fts fts ON fts.id = c.id");
        conditions.push("mcp_curated_server_fts MATCH ?".to_string());
    }
    if let Some(publisher) = publisher {
        conditions.push("c.source = ?".to_string());
        // Bound after the MATCH expression, matching the emission order above.
        params.push(SqlValue::Text(publisher.to_string()));
    }
    if !conditions.is_empty() {
        sql.push_str(" WHERE ");
        sql.push_str(&conditions.join(" AND "));
    }
    sql
}

fn registry_branch(ranked: bool) -> String {
    let rank = if ranked {
        format!("bm25(mcp_registry_server_fts, {BM25_WEIGHTS})")
    } else {
        "0.0".to_string()
    };
    let mut sql = format!(
        "SELECT s.id, s.name, s.namespace, s.description, s.repo_url, s.stars, s.license, s.version,
                s.kind, s.runtimes_json, s.updated_at,
                0 AS recommended, NULL AS source,
                s.title, s.website_url, s.icons_json, s.status, s.is_latest, s.published_at,
                s.registry_source,
                100000 AS sort_priority,
                {rank} AS search_rank
         FROM mcp_registry_server s"
    );
    if ranked {
        sql.push_str(" JOIN mcp_registry_server_fts fts ON fts.id = s.id");
    }
    // Curated ids win: the same server must not appear twice in one list.
    sql.push_str(" WHERE s.id NOT IN (SELECT id FROM mcp_curated_server)");
    if ranked {
        sql.push_str(" AND mcp_registry_server_fts MATCH ?");
    }
    sql
}

/// Build the `UNION ALL` sub-select and push its bound parameters, in the
/// exact order the placeholders appear.
fn build_inner(
    query: &McpServerQuery,
    match_expr: Option<&str>,
    params: &mut Vec<SqlValue>,
) -> String {
    let ranked = match_expr.is_some();
    let publisher = query.publisher_id.as_deref();
    match publisher {
        // The GitHub publisher is the remote registry table.
        Some("github") => {
            if let Some(expr) = match_expr {
                params.push(SqlValue::Text(expr.to_string()));
            }
            registry_branch(ranked)
        }
        Some(curated) => {
            if let Some(expr) = match_expr {
                params.push(SqlValue::Text(expr.to_string()));
            }
            curated_branch(ranked, Some(curated), params)
        }
        None => {
            if let Some(expr) = match_expr {
                params.push(SqlValue::Text(expr.to_string()));
            }
            let curated = curated_branch(ranked, None, params);
            if let Some(expr) = match_expr {
                params.push(SqlValue::Text(expr.to_string()));
            }
            let registry = registry_branch(ranked);
            format!("{curated} UNION ALL {registry}")
        }
    }
}

/// Outer `WHERE` over the unioned rows, plus its parameters.
fn build_filters(query: &McpServerQuery, params: &mut Vec<SqlValue>) -> String {
    let mut conditions: Vec<String> = Vec::new();

    if !query.kinds.is_empty() {
        conditions.push(format!("kind IN ({})", placeholders(query.kinds.len())));
        for kind in &query.kinds {
            params.push(SqlValue::Text(kind.as_db_str().to_string()));
        }
    }
    if !query.statuses.is_empty() {
        conditions.push(format!(
            "status IN ({})",
            placeholders(query.statuses.len())
        ));
        for status in &query.statuses {
            params.push(SqlValue::Text(status.as_db_str().to_string()));
        }
    }
    if !query.licenses.is_empty() {
        conditions.push(format!(
            "LOWER(COALESCE(license, '')) IN ({})",
            placeholders(query.licenses.len())
        ));
        for license in &query.licenses {
            params.push(SqlValue::Text(license.trim().to_lowercase()));
        }
    }
    if !query.runtimes.is_empty() {
        // `runtimes_json` is a JSON array of quoted strings; matching on the
        // quoted token keeps `npx` from matching a hypothetical `npx-foo`.
        let clause = query
            .runtimes
            .iter()
            .map(|_| "runtimes_json LIKE ?".to_string())
            .collect::<Vec<_>>()
            .join(" OR ");
        conditions.push(format!("({clause})"));
        for runtime in &query.runtimes {
            params.push(SqlValue::Text(format!("%\"{}\"%", runtime.trim())));
        }
    }
    if query.recommended_only {
        conditions.push("recommended != 0".to_string());
    }
    if query.latest_only {
        conditions.push("is_latest != 0".to_string());
    }
    if let Some(min) = query.min_stars {
        conditions.push("stars >= ?".to_string());
        params.push(SqlValue::Integer(min as i64));
    }
    if let Some(max) = query.max_stars {
        conditions.push("stars <= ?".to_string());
        params.push(SqlValue::Integer(max as i64));
    }

    if conditions.is_empty() {
        String::new()
    } else {
        format!(" WHERE {}", conditions.join(" AND "))
    }
}

fn placeholders(count: usize) -> String {
    std::iter::repeat_n("?", count)
        .collect::<Vec<_>>()
        .join(", ")
}

/// Run a card query, returning the page plus the pre-pagination total.
pub(crate) fn query_cards(conn: &Connection, query: &McpServerQuery) -> Result<McpServerPage> {
    let match_expr = query.search_terms().and_then(build_fts_match);
    let mut inner_params: Vec<SqlValue> = Vec::new();
    let inner = build_inner(query, match_expr.as_deref(), &mut inner_params);
    let mut filter_params: Vec<SqlValue> = Vec::new();
    let filters = build_filters(query, &mut filter_params);

    let mut count_params = inner_params.clone();
    count_params.extend(filter_params.iter().cloned());
    let total: i64 = conn
        .query_row(
            &format!("SELECT COUNT(*) FROM ({inner}){filters}"),
            params_from_iter(count_params.iter()),
            |row| row.get(0),
        )
        .context("Failed to count MCP marketplace cards")?;

    let order = query
        .sort
        .order_by(query.is_descending(), match_expr.is_some());
    let mut sql = format!("SELECT {CARD_COLUMNS} FROM ({inner}){filters} ORDER BY {order}");
    let mut list_params = inner_params;
    list_params.extend(filter_params);
    if let Some(limit) = query.limit {
        sql.push_str(" LIMIT ?");
        list_params.push(SqlValue::Integer(limit as i64));
        // SQLite only honours OFFSET together with LIMIT.
        sql.push_str(" OFFSET ?");
        list_params.push(SqlValue::Integer(query.offset.unwrap_or(0) as i64));
    } else if let Some(offset) = query.offset.filter(|o| *o > 0) {
        sql.push_str(" LIMIT -1 OFFSET ?");
        list_params.push(SqlValue::Integer(offset as i64));
    }

    let mut stmt = conn
        .prepare(&sql)
        .context("Failed to prepare MCP marketplace card query")?;
    let rows = stmt
        .query_map(params_from_iter(list_params.iter()), row_to_card)
        .context("Failed to run MCP marketplace card query")?;
    let mut items = Vec::new();
    for row in rows {
        items.push(row.context("Failed to decode MCP marketplace card")?);
    }

    Ok(McpServerPage {
        items,
        total: total.max(0) as u32,
        offset: query.offset.unwrap_or(0),
        limit: query.limit,
    })
}

pub(crate) fn row_to_card(row: &rusqlite::Row<'_>) -> rusqlite::Result<McpMarketEntry> {
    let runtimes_json: String = row.get("runtimes_json")?;
    let kind_str: String = row.get("kind")?;
    let status_str: String = row.get::<_, Option<String>>("status")?.unwrap_or_default();
    let icons_json: String = row
        .get::<_, Option<String>>("icons_json")?
        .unwrap_or_else(|| "[]".into());
    let recommended = row
        .get::<_, Option<i64>>("recommended")?
        .unwrap_or_default()
        != 0;
    let icon_url = serde_json::from_str::<Vec<McpIcon>>(&icons_json)
        .unwrap_or_default()
        .into_iter()
        .next()
        .map(|icon| icon.src);
    Ok(McpMarketEntry {
        id: row.get("id")?,
        name: row.get("name")?,
        namespace: row.get("namespace")?,
        description: row.get("description")?,
        repo_url: row.get("repo_url")?,
        stars: row.get::<_, i64>("stars")? as u32,
        license: row.get("license")?,
        version: row.get("version")?,
        kind: McpServerKind::from_db_str(&kind_str),
        runtimes: serde_json::from_str(&runtimes_json).unwrap_or_default(),
        updated_at: row.get("updated_at")?,
        recommended,
        source: row.get("source")?,
        title: row.get("title")?,
        website_url: row.get("website_url")?,
        icon_url,
        status: McpServerStatus::from_db_str(&status_str),
        is_latest: row.get::<_, Option<i64>>("is_latest")?.unwrap_or(1) != 0,
        registry_source: row.get("registry_source")?,
    })
}

/// Every card, in the historical order. Thin wrapper so the default listing
/// behaviour has one definition.
pub(crate) fn load_cards(conn: &Connection) -> Result<Vec<McpMarketEntry>> {
    Ok(query_cards(conn, &McpServerQuery::default())?.items)
}

/// Cards filtered to a single publisher.
pub(crate) fn load_cards_by_publisher(
    conn: &Connection,
    publisher_id: &str,
) -> Result<Vec<McpMarketEntry>> {
    // Curated publishers keep their seed order (`priority`), which the default
    // sort already encodes; GitHub keeps stars-first.
    Ok(query_cards(conn, &McpServerQuery::for_publisher(publisher_id))?.items)
}

pub(crate) fn search_cards(
    conn: &Connection,
    query: &str,
    limit: u32,
) -> Result<Vec<McpMarketEntry>> {
    Ok(query_cards(conn, &McpServerQuery::search_query(query, limit))?.items)
}

/// Build a safe FTS5 MATCH expression: keep alphanumeric tokens, quote each and
/// add a prefix wildcard, AND them together. Returns `None` for an empty query.
pub(crate) fn build_fts_match(query: &str) -> Option<String> {
    let terms: Vec<String> = query
        .split(|c: char| !c.is_alphanumeric())
        .filter(|t| !t.is_empty())
        .map(|t| format!("\"{}\"*", t.to_lowercase()))
        .collect();
    if terms.is_empty() {
        None
    } else {
        Some(terms.join(" "))
    }
}
