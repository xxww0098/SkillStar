//! The parameterized card query: filters, sorting, pagination, totals.

use super::*;
use crate::mcp_models::McpServerStatus;
use crate::mcp_snapshot::filters::{McpServerQuery, McpSortKey};

fn seeded() -> Connection {
    let conn = test_conn();
    let mut deprecated = sample("d1", "legacy", 500, McpServerKind::Stdio);
    deprecated.status = McpServerStatus::Deprecated;
    deprecated.license = Some("Apache-2.0".into());
    deprecated.updated_at = Some("2024-01-01T00:00:00Z".into());

    let mut superseded = sample("d2", "superseded", 10, McpServerKind::Remote);
    superseded.is_latest = false;
    superseded.runtimes = vec![];
    superseded.updated_at = Some("2025-06-01T00:00:00Z".into());

    let mut docker = sample("d3", "containerized", 250, McpServerKind::Both);
    docker.runtimes = vec!["docker".into()];
    docker.license = Some("mit".into());
    docker.updated_at = Some("2026-03-01T00:00:00Z".into());

    replace_servers(&conn, &[deprecated, superseded, docker]).unwrap();
    conn
}

/// The default query must reproduce the historical listing exactly, or every
/// existing screen shifts under the user.
#[test]
fn default_query_matches_legacy_load_cards() {
    let conn = seeded();
    let page = query_cards(&conn, &McpServerQuery::default()).unwrap();
    let legacy = load_cards(&conn).unwrap();
    assert_eq!(page.items, legacy);
    assert_eq!(page.total as usize, legacy.len());
    assert_eq!(page.offset, 0);
    assert!(page.limit.is_none());
}

#[test]
fn filters_by_kind_status_license_runtime_and_stars() {
    let conn = seeded();

    let stdio = query_cards(
        &conn,
        &McpServerQuery {
            publisher_id: Some("github".into()),
            kinds: vec![McpServerKind::Stdio],
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(stdio.items.len(), 1);
    assert_eq!(stdio.items[0].id, "d1");

    let deprecated = query_cards(
        &conn,
        &McpServerQuery {
            statuses: vec![McpServerStatus::Deprecated],
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(deprecated.items.len(), 1);
    assert_eq!(deprecated.items[0].id, "d1");

    // License matching is case-insensitive: registries disagree on casing.
    let mit = query_cards(
        &conn,
        &McpServerQuery {
            publisher_id: Some("github".into()),
            licenses: vec!["MIT".into()],
            ..Default::default()
        },
    )
    .unwrap();
    let mit_ids: Vec<&str> = mit.items.iter().map(|c| c.id.as_str()).collect();
    assert!(mit_ids.contains(&"d2"));
    assert!(mit_ids.contains(&"d3"));
    assert!(!mit_ids.contains(&"d1"));

    let docker = query_cards(
        &conn,
        &McpServerQuery {
            publisher_id: Some("github".into()),
            runtimes: vec!["docker".into()],
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(docker.items.len(), 1);
    assert_eq!(docker.items[0].id, "d3");

    let starred = query_cards(
        &conn,
        &McpServerQuery {
            publisher_id: Some("github".into()),
            min_stars: Some(100),
            max_stars: Some(400),
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(starred.items.len(), 1);
    assert_eq!(starred.items[0].id, "d3");

    let latest = query_cards(
        &conn,
        &McpServerQuery {
            publisher_id: Some("github".into()),
            latest_only: true,
            ..Default::default()
        },
    )
    .unwrap();
    assert!(latest.items.iter().all(|c| c.id != "d2"));

    let recommended = query_cards(
        &conn,
        &McpServerQuery {
            recommended_only: true,
            ..Default::default()
        },
    )
    .unwrap();
    assert!(recommended.items.iter().all(|c| c.recommended));
    assert_eq!(recommended.items[0].id, "adspower-local-api");
}

#[test]
fn pagination_reports_the_pre_pagination_total() {
    let conn = seeded();
    let all = query_cards(&conn, &McpServerQuery::default()).unwrap();
    let total = all.total;
    assert!(total > 5);

    let first = query_cards(
        &conn,
        &McpServerQuery {
            limit: Some(5),
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(first.items.len(), 5);
    assert_eq!(first.total, total);

    let second = query_cards(
        &conn,
        &McpServerQuery {
            limit: Some(5),
            offset: Some(5),
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(second.total, total);
    assert_eq!(second.offset, 5);
    assert_eq!(second.items, all.items[5..10].to_vec());

    // OFFSET without LIMIT still skips (SQLite needs the `LIMIT -1` form).
    let skipped = query_cards(
        &conn,
        &McpServerQuery {
            offset: Some(5),
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(skipped.items.len() as u32, total - 5);
}

#[test]
fn sort_keys_are_whitelisted_and_directional() {
    let conn = seeded();
    let registry_only = |sort, descending| McpServerQuery {
        publisher_id: Some("github".into()),
        sort,
        descending,
        ..Default::default()
    };

    let by_stars = query_cards(&conn, &registry_only(McpSortKey::Stars, None)).unwrap();
    assert_eq!(
        by_stars
            .items
            .iter()
            .map(|c| c.id.as_str())
            .collect::<Vec<_>>(),
        vec!["d1", "d3", "d2"]
    );

    let by_stars_asc = query_cards(&conn, &registry_only(McpSortKey::Stars, Some(false))).unwrap();
    assert_eq!(by_stars_asc.items[0].id, "d2");

    let by_name = query_cards(&conn, &registry_only(McpSortKey::Name, None)).unwrap();
    assert_eq!(by_name.items[0].name, "containerized");

    let by_updated = query_cards(&conn, &registry_only(McpSortKey::Updated, None)).unwrap();
    assert_eq!(by_updated.items[0].id, "d3");
}

#[test]
fn search_combines_with_filters_and_keeps_rank_order() {
    let conn = seeded();
    let hits = query_cards(
        &conn,
        &McpServerQuery {
            search: Some("legacy".into()),
            publisher_id: Some("github".into()),
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(hits.items.len(), 1);
    assert_eq!(hits.items[0].id, "d1");
    assert_eq!(hits.total, 1);

    // A filter that excludes the only hit yields an honest empty page.
    let filtered_out = query_cards(
        &conn,
        &McpServerQuery {
            search: Some("legacy".into()),
            publisher_id: Some("github".into()),
            statuses: vec![McpServerStatus::Active],
            ..Default::default()
        },
    )
    .unwrap();
    assert!(filtered_out.items.is_empty());
    assert_eq!(filtered_out.total, 0);

    // Legacy `search_cards` keeps behaving like the old hardcoded query.
    let legacy = search_cards(&conn, "legacy", 10).unwrap();
    assert_eq!(legacy.len(), 1);
}

/// Filter values are bound, never interpolated — a quote in a license name
/// must not be able to reach the SQL text.
#[test]
fn filter_values_are_bound_not_interpolated() {
    let conn = seeded();
    let page = query_cards(
        &conn,
        &McpServerQuery {
            licenses: vec!["MIT'); DROP TABLE mcp_registry_server; --".into()],
            ..Default::default()
        },
    )
    .unwrap();
    assert!(page.items.is_empty());
    assert_eq!(count_servers(&conn).unwrap(), 3, "table survived");
}
