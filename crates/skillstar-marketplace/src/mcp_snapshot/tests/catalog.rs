//! Catalog round-trip, publisher bucketing and FTS coverage.

use super::*;

#[test]
fn replace_then_load_and_search_roundtrip() {
    let conn = test_conn();
    let servers = vec![
        sample("1", "filesystem", 100, McpServerKind::Stdio),
        sample("2", "postgres", 50, McpServerKind::Both),
    ];
    replace_servers(&conn, &servers).unwrap();
    assert_eq!(count_servers(&conn).unwrap(), 2);

    // curated recommendations lead, then registry rows ordered by stars desc.
    // AdsPower is recommended → first; then the 4 BigModel curated rows
    // (priority 0..3); then registry rows by stars.
    let cards = load_cards(&conn).unwrap();
    assert_eq!(cards[0].name, "adspower-local-api");
    assert!(cards[0].recommended);
    // All curated servers sit before the registry rows: 1 adspower +
    // 4 bigmodel + 4 anthropic + 2 microsoft + 3 saas + 2 cn-ai +
    // 3 cloudflare + 1 brave + 2 google + 1 supabase + 2 x = 25.
    let registry_start = cards
        .iter()
        .position(|c| c.id == "1")
        .expect("registry filesystem card present");
    assert_eq!(registry_start, 25);
    assert_eq!(cards[registry_start].name, "filesystem");
    assert_eq!(cards[registry_start + 1].name, "postgres");
    assert_eq!(cards[registry_start].kind, McpServerKind::Stdio);

    // FTS search — "postgres" also matches the Supabase curated server
    // ("Postgres 数据库"), so filter to just the registry hit by id.
    let hits = search_cards(&conn, "postgres", 10).unwrap();
    let pg_registry = hits
        .iter()
        .find(|h| h.id == "2")
        .expect("registry postgres hit");
    assert_eq!(pg_registry.name, "postgres");

    // empty query → all, truncated
    let all = search_cards(&conn, "   ", 1).unwrap();
    assert_eq!(all.len(), 1);

    // detail + full server (raw json preserved)
    let full = load_full_server(&conn, "1").unwrap().unwrap();
    assert_eq!(full.packages[0].identifier, "@acme/filesystem");
    assert_eq!(full.raw_server_json, "{\"name\":\"acme/filesystem\"}");
    assert_eq!(full.to_detail().entry.name, "filesystem");

    let curated = load_full_server(&conn, "adspower-local-api")
        .unwrap()
        .unwrap();
    assert!(curated.recommended);
    assert_eq!(curated.packages[0].identifier, "local-api-mcp-typescript");
    // AdsPower is now its own publisher bucket.
    assert_eq!(
        curated.to_detail().entry.source.as_deref(),
        Some("adspower")
    );
}

#[test]
fn replace_is_a_full_swap() {
    let conn = test_conn();
    replace_servers(&conn, &[sample("1", "old", 1, McpServerKind::Stdio)]).unwrap();
    replace_servers(&conn, &[sample("2", "new", 1, McpServerKind::Stdio)]).unwrap();
    assert_eq!(count_servers(&conn).unwrap(), 1);
    assert!(load_full_server(&conn, "1").unwrap().is_none());
    assert!(load_full_server(&conn, "2").unwrap().is_some());
    // FTS swapped too
    assert!(search_cards(&conn, "old", 10).unwrap().is_empty());
    assert_eq!(search_cards(&conn, "new", 10).unwrap().len(), 1);
}

#[test]
fn sync_state_freshness_transitions() {
    let conn = test_conn();
    assert!(read_sync_state(&conn).unwrap().is_none());

    mark_success(&conn).unwrap();
    let state = read_sync_state(&conn).unwrap();
    assert!(state.is_some());
    assert!(is_fresh(&state)); // next_refresh is in the future

    mark_error(&conn, "boom").unwrap();
    let state = read_sync_state(&conn).unwrap().unwrap();
    assert_eq!(state.last_error.as_deref(), Some("boom"));
    assert!(state.last_success_at.is_some()); // success preserved on error
}

/// A knowingly incomplete catalog has to stay knowable after a restart — the
/// truncation marker was previously computed and then dropped on the floor.
#[test]
fn degraded_reason_is_persisted_and_cleared() {
    let conn = test_conn();
    let meta = crate::remote::FetchMeta {
        payload_sha256: "abc".into(),
        source_host: "registry.modelcontextprotocol.io".into(),
        etag: Some("W/\"v1\"".into()),
        degraded: true,
    };
    mark_success_with_meta(&conn, &meta, false, Some("official: page cap")).unwrap();
    let state = read_sync_state(&conn).unwrap().unwrap();
    assert_eq!(state.degraded_reason.as_deref(), Some("official: page cap"));
    assert_eq!(state.payload_sha256.as_deref(), Some("abc"));
    assert!(state.last_error.is_none(), "degraded is not an error");

    // A later complete sync must clear it, or the UI warns forever.
    let complete = crate::remote::FetchMeta {
        payload_sha256: "def".into(),
        degraded: false,
        ..meta.clone()
    };
    mark_success_with_meta(&conn, &complete, false, None).unwrap();
    let state = read_sync_state(&conn).unwrap().unwrap();
    assert!(state.degraded_reason.is_none());

    // An unchanged refresh preserves the fingerprint but still records the
    // (absent) degraded verdict for this run.
    mark_success_with_meta(&conn, &meta, true, None).unwrap();
    let state = read_sync_state(&conn).unwrap().unwrap();
    assert_eq!(state.payload_sha256.as_deref(), Some("def"));
}

#[test]
fn per_source_sync_states_are_separately_addressable() {
    let conn = test_conn();
    let meta = crate::remote::FetchMeta {
        payload_sha256: "hash-official".into(),
        source_host: "registry.modelcontextprotocol.io".into(),
        etag: Some("etag-official".into()),
        degraded: false,
    };
    mark_scope_success(&conn, &source_scope("official"), &meta, false, None).unwrap();
    mark_scope_error(&conn, &source_scope("github"), "429 rate limited").unwrap();

    let states = read_source_states(&conn).unwrap();
    assert_eq!(states.len(), 2);
    let github = states
        .iter()
        .find(|s| s.scope == "mcp_registry:github")
        .unwrap();
    assert_eq!(github.last_error.as_deref(), Some("429 rate limited"));
    let official = states
        .iter()
        .find(|s| s.scope == "mcp_registry:official")
        .unwrap();
    assert_eq!(official.etag.as_deref(), Some("etag-official"));

    // The aggregate scope is untouched by per-source bookkeeping.
    assert!(read_sync_state(&conn).unwrap().is_none());
}

#[test]
fn fts_match_builder_is_injection_safe() {
    assert!(build_fts_match("   ").is_none());
    assert_eq!(build_fts_match("github").as_deref(), Some("\"github\"*"));
    // punctuation stripped, terms ANDed
    assert_eq!(
        build_fts_match("file system!").as_deref(),
        Some("\"file\"* \"system\"*")
    );
}

#[test]
fn publishers_aggregate_curated_sources_and_github() {
    let conn = test_conn();
    // Curated seeds are written by `create_mcp_registry_tables`.
    let publishers = load_publishers(&conn).unwrap();

    // 11 curated publishers + GitHub (0 registry rows seeded yet) = 12.
    assert_eq!(publishers.len(), 12);
    // CURATED_ORDER dictates grid order; GitHub always last.
    assert_eq!(publishers[0].id, "adspower");
    assert_eq!(publishers[0].name, "AdsPower");
    assert_eq!(publishers[0].server_count, 1);
    assert_eq!(publishers[1].id, "bigmodel");
    assert_eq!(publishers[1].name, "BigModel");
    assert_eq!(publishers[1].server_count, 4);
    assert_eq!(publishers[2].id, "anthropic");
    assert_eq!(publishers[2].name, "Anthropic");
    assert_eq!(publishers[2].server_count, 4);
    assert_eq!(publishers[3].id, "microsoft");
    assert_eq!(publishers[3].name, "Microsoft");
    assert_eq!(publishers[3].server_count, 2);
    assert_eq!(publishers[4].id, "saas");
    assert_eq!(publishers[4].server_count, 3);
    assert_eq!(publishers[5].id, "cn-ai");
    assert_eq!(publishers[5].server_count, 2);
    assert_eq!(publishers[6].id, "cloudflare");
    assert_eq!(publishers[6].name, "Cloudflare");
    assert_eq!(publishers[6].server_count, 3);
    assert_eq!(publishers[7].id, "brave");
    assert_eq!(publishers[7].server_count, 1);
    assert_eq!(publishers[8].id, "google");
    assert_eq!(publishers[8].server_count, 2);
    assert_eq!(publishers[9].id, "supabase");
    assert_eq!(publishers[9].server_count, 1);
    assert_eq!(publishers[10].id, "x");
    assert_eq!(publishers[10].name, "X");
    assert_eq!(publishers[10].server_count, 2);
    assert_eq!(publishers[11].id, "github");
    assert_eq!(publishers[11].server_count, 0);

    // After we add registry rows, GitHub's count climbs.
    replace_servers(
        &conn,
        &[
            sample("1", "filesystem", 100, McpServerKind::Stdio),
            sample("2", "postgres", 50, McpServerKind::Both),
        ],
    )
    .unwrap();
    let publishers = load_publishers(&conn).unwrap();
    let github = publishers.iter().find(|p| p.id == "github").unwrap();
    assert_eq!(github.server_count, 2);
}

#[test]
fn publisher_cards_split_curated_and_registry() {
    let conn = test_conn();
    replace_servers(
        &conn,
        &[sample("r1", "filesystem", 10, McpServerKind::Stdio)],
    )
    .unwrap();

    // Curated publisher returns only its bucket.
    let adspower = load_cards_by_publisher(&conn, "adspower").unwrap();
    assert_eq!(adspower.len(), 1);
    assert_eq!(adspower[0].id, "adspower-local-api");
    assert_eq!(adspower[0].source.as_deref(), Some("adspower"));

    let bigmodel = load_cards_by_publisher(&conn, "bigmodel").unwrap();
    assert_eq!(bigmodel.len(), 4);
    // Ordered by priority (seed order): vision, search, reader, zread.
    assert_eq!(bigmodel[0].id, "bigmodel-vision");
    assert_eq!(bigmodel[1].id, "bigmodel-search");
    assert_eq!(bigmodel[2].id, "bigmodel-reader");
    assert_eq!(bigmodel[3].id, "bigmodel-zread");
    assert_eq!(bigmodel[0].kind, McpServerKind::Stdio);
    assert_eq!(bigmodel[1].kind, McpServerKind::Remote);
    // BigModel remote servers carry their endpoint URL on the detail row.
    let vision_full = load_full_server(&conn, "bigmodel-vision").unwrap().unwrap();
    assert_eq!(vision_full.packages[0].identifier, "@z_ai/mcp-server");
    assert!(
        vision_full.packages[0]
            .required_env
            .iter()
            .any(|e| e == "Z_AI_API_KEY")
    );
    let search_full = load_full_server(&conn, "bigmodel-search").unwrap().unwrap();
    assert_eq!(search_full.remotes.len(), 1);
    assert_eq!(
        search_full.remotes[0].url,
        "https://open.bigmodel.cn/api/mcp/web_search_prime/mcp"
    );
    assert!(
        search_full.remotes[0]
            .required_headers
            .iter()
            .any(|h| h == "Authorization")
    );

    // New curated publishers are filtered by their source bucket too.
    let anthropic = load_cards_by_publisher(&conn, "anthropic").unwrap();
    assert_eq!(anthropic.len(), 4);
    assert_eq!(anthropic[0].id, "anthropic-filesystem");
    assert!(
        anthropic
            .iter()
            .all(|c| c.source.as_deref() == Some("anthropic"))
    );

    let microsoft = load_cards_by_publisher(&conn, "microsoft").unwrap();
    assert_eq!(microsoft.len(), 2);

    let saas = load_cards_by_publisher(&conn, "saas").unwrap();
    assert_eq!(saas.len(), 3);
    // All SaaS entries are remote streamable-http.
    assert!(saas.iter().all(|c| c.kind == McpServerKind::Remote));

    let cn_ai = load_cards_by_publisher(&conn, "cn-ai").unwrap();
    assert_eq!(cn_ai.len(), 2);
    // Firecrawl requires an API key env var.
    let fc = load_full_server(&conn, "extra-firecrawl").unwrap().unwrap();
    assert!(
        fc.packages[0]
            .required_env
            .iter()
            .any(|e| e == "FIRECRAWL_API_KEY")
    );

    // Second batch of curated publishers.
    let cloudflare = load_cards_by_publisher(&conn, "cloudflare").unwrap();
    assert_eq!(cloudflare.len(), 3);
    assert!(cloudflare.iter().all(|c| c.kind == McpServerKind::Remote));

    let brave = load_cards_by_publisher(&conn, "brave").unwrap();
    assert_eq!(brave.len(), 1);
    assert_eq!(brave[0].kind, McpServerKind::Stdio);

    let google = load_cards_by_publisher(&conn, "google").unwrap();
    assert_eq!(google.len(), 2);

    let supabase = load_cards_by_publisher(&conn, "supabase").unwrap();
    assert_eq!(supabase.len(), 1);

    // GitHub publisher returns registry rows, excluding curated ids.
    let github = load_cards_by_publisher(&conn, "github").unwrap();
    assert_eq!(github.len(), 1);
    assert_eq!(github[0].id, "r1");
    assert!(github[0].source.is_none());
}

/// The new `2025-12-11` fields survive the write → read round trip on both
/// tables; before v13 they had nowhere to live.
#[test]
fn schema_v13_columns_round_trip() {
    let conn = test_conn();
    let mut server = sample("dep", "deprecated-thing", 7, McpServerKind::Stdio);
    server.title = Some("Deprecated Thing".into());
    server.website_url = Some("https://example.com".into());
    server.icons = vec![crate::mcp_models::McpIcon {
        src: "https://example.com/icon.png".into(),
        mime_type: Some("image/png".into()),
        ..Default::default()
    }];
    server.status = crate::mcp_models::McpServerStatus::Deprecated;
    server.is_latest = false;
    server.published_at = Some("2026-02-02T00:00:00Z".into());
    server.registry_source = Some("official".into());
    server.contributing_sources = vec!["official".into(), "github".into()];
    replace_servers(&conn, &[server]).unwrap();

    let card = load_cards_by_publisher(&conn, "github").unwrap().remove(0);
    assert_eq!(card.title.as_deref(), Some("Deprecated Thing"));
    assert_eq!(card.website_url.as_deref(), Some("https://example.com"));
    assert_eq!(
        card.icon_url.as_deref(),
        Some("https://example.com/icon.png")
    );
    assert_eq!(card.status, crate::mcp_models::McpServerStatus::Deprecated);
    assert!(!card.is_latest);
    assert_eq!(card.registry_source.as_deref(), Some("official"));

    let full = load_full_server(&conn, "dep").unwrap().unwrap();
    assert_eq!(full.published_at.as_deref(), Some("2026-02-02T00:00:00Z"));
    assert_eq!(
        full.contributing_sources,
        vec!["official".to_string(), "github".to_string()]
    );
    let detail = full.to_detail();
    assert_eq!(detail.icons.len(), 1);
}
