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

    // Curated recommendations lead (`is_recommended DESC`), then the rest of
    // the curated catalog in seed order (`priority ASC`), then registry rows
    // by stars desc.
    let cards = load_cards(&conn).unwrap();
    assert_eq!(cards[0].name, "filesystem");
    assert!(cards[0].recommended);
    // All 13 curated servers (the recommended shortlist) sit before the
    // registry rows.
    let registry_start = cards
        .iter()
        .position(|c| c.id == "1")
        .expect("registry filesystem card present");
    assert_eq!(registry_start, 13);
    assert_eq!(cards[registry_start].name, "filesystem");
    assert_eq!(cards[registry_start + 1].name, "postgres");
    assert_eq!(cards[registry_start].kind, McpServerKind::Stdio);

    // FTS search — "postgres" matches only the registry fixture now that no
    // curated row mentions it; find it by id anyway so a future curated hit
    // can't break the assertion.
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

    // A curated stdio row carries both the launcher and the extra arguments
    // the package needs after its identifier.
    let codegraph = load_full_server(&conn, "codegraph").unwrap().unwrap();
    assert_eq!(codegraph.packages[0].identifier, "@colbymchenry/codegraph");
    assert_eq!(codegraph.packages[0].runtime, "npx");
    let extra_args: Vec<&str> = codegraph.packages[0]
        .package_arguments
        .iter()
        .filter_map(|arg| arg.input.value.as_deref())
        .collect();
    assert_eq!(extra_args, ["serve", "--mcp"]);

    // serena is the row that needs a *runtime* argument: `uvx --from <source>`
    // selects the git source because the executable (`serena`) is not the
    // package name, then `start-mcp-server` runs as its subcommand.
    let serena = load_full_server(&conn, "serena").unwrap().unwrap();
    assert_eq!(serena.packages[0].runtime, "uvx");
    assert_eq!(serena.packages[0].identifier, "serena");
    let from = &serena.packages[0].runtime_arguments[0];
    assert_eq!(from.name.as_deref(), Some("--from"));
    assert_eq!(
        from.input.value.as_deref(),
        Some("git+https://github.com/oraios/serena")
    );
    let serena_args: Vec<&str> = serena.packages[0]
        .package_arguments
        .iter()
        .filter_map(|arg| arg.input.value.as_deref())
        .collect();
    assert_eq!(serena_args, ["start-mcp-server", "--project-from-cwd"]);

    // A PyPI-backed row must launch through uvx, not npx — `git` is one of the
    // three rows that used to point at an archived npm package.
    let git = load_full_server(&conn, "git").unwrap().unwrap();
    assert_eq!(git.packages[0].runtime, "uvx");
    assert_eq!(git.packages[0].identifier, "mcp-server-git");

    let curated = load_full_server(&conn, "context7").unwrap().unwrap();
    assert!(curated.recommended);
    assert_eq!(curated.packages[0].identifier, "@upstash/context7-mcp");
    assert_eq!(
        curated.to_detail().entry.source.as_deref(),
        Some("context")
    );

    // A token-bearing remote asks for its header; an OAuth remote asks for
    // nothing up front and lets the first connection drive the auth flow.
    let github = load_full_server(&conn, "github").unwrap().unwrap();
    assert_eq!(github.remotes[0].url, "https://api.githubcopilot.com/mcp/");
    assert_eq!(github.remotes[0].required_headers, ["Authorization"]);
    let deepwiki = load_full_server(&conn, "deepwiki").unwrap().unwrap();
    assert_eq!(deepwiki.remotes[0].url, "https://mcp.deepwiki.com/mcp");
    assert!(
        deepwiki.remotes[0].required_headers.is_empty(),
        "an OAuth remote must not pre-ask for a header"
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

    // 4 curated shelves + GitHub (0 registry rows seeded yet) = 5.
    assert_eq!(publishers.len(), 5);
    // CURATED_ORDER dictates summary order; GitHub always last.
    assert_eq!(publishers[0].id, "core");
    assert_eq!(publishers[0].name, "Core");
    assert_eq!(publishers[0].server_count, 3);
    assert_eq!(publishers[1].id, "context");
    assert_eq!(publishers[1].name, "Context");
    assert_eq!(publishers[1].server_count, 4);
    assert_eq!(publishers[2].id, "browser");
    assert_eq!(publishers[2].name, "Browser");
    assert_eq!(publishers[2].server_count, 2);
    assert_eq!(publishers[3].id, "creative");
    assert_eq!(publishers[3].name, "Creative");
    assert_eq!(publishers[3].server_count, 4);
    assert_eq!(publishers[4].id, "github");
    assert_eq!(publishers[4].server_count, 0);

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
fn seed_drops_curated_rows_that_left_the_code_registry() {
    let conn = test_conn();
    conn.execute(
        "INSERT INTO mcp_curated_server (id, name, namespace, fetched_at, source)
         VALUES ('orphan-mcp', 'orphan', 'orphan', '2026-01-01T00:00:00Z', 'bigmodel')",
        [],
    )
    .unwrap();
    super::super::seeding::seed_default_curated_mcp_servers(&conn).unwrap();
    assert!(load_full_server(&conn, "orphan-mcp").unwrap().is_none());
    assert!(
        load_cards_by_publisher(&conn, "bigmodel")
            .unwrap()
            .is_empty()
    );
    assert!(
        load_publishers(&conn)
            .unwrap()
            .iter()
            .all(|publisher| publisher.id != "bigmodel")
    );
}

#[test]
fn prune_retired_curated_rows_respects_keep_list() {
    let conn = test_conn();

    // An empty keep-list wipes the entire curated catalog (table and FTS).
    super::super::seeding::prune_retired_curated_rows(&conn, &[]).unwrap();
    assert_eq!(
        conn.query_row("SELECT COUNT(*) FROM mcp_curated_server", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        0,
        "empty keep-list must wipe curated rows"
    );
    assert_eq!(
        conn.query_row("SELECT COUNT(*) FROM mcp_curated_server_fts", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        0,
        "empty keep-list must wipe curated FTS rows"
    );

    // Reseed, then keep only one real seed id: every other curated row and its
    // FTS row must be pruned, leaving exactly the kept server.
    super::super::seeding::seed_default_curated_mcp_servers(&conn).unwrap();
    let keep = "context7".to_string();
    super::super::seeding::prune_retired_curated_rows(&conn, &[keep.clone()]).unwrap();
    let remaining: i64 = conn
        .query_row("SELECT COUNT(*) FROM mcp_curated_server", [], |r| {
            r.get::<_, i64>(0)
        })
        .unwrap();
    assert_eq!(remaining, 1, "only the kept id should survive");
    assert_eq!(
        conn.query_row("SELECT id FROM mcp_curated_server", [], |r| r
            .get::<_, String>(0))
            .unwrap(),
        keep,
        "the surviving row is the kept id"
    );
    let fts_remaining: i64 = conn
        .query_row("SELECT COUNT(*) FROM mcp_curated_server_fts", [], |r| {
            r.get::<_, i64>(0)
        })
        .unwrap();
    assert_eq!(fts_remaining, 1, "its FTS row must survive too");
}

#[test]
fn publisher_cards_split_curated_and_registry() {
    let conn = test_conn();
    replace_servers(
        &conn,
        &[sample("r1", "filesystem", 10, McpServerKind::Stdio)],
    )
    .unwrap();

    // A curated shelf returns only its own rows, in catalog order.
    let core = load_cards_by_publisher(&conn, "core").unwrap();
    assert_eq!(core.len(), 3);
    assert_eq!(core[0].id, "filesystem");
    assert_eq!(core[0].source.as_deref(), Some("core"));
    // The shelf mixes local and hosted shapes: filesystem/git are stdio,
    // github is a remote.
    assert_eq!(
        core.iter().filter(|c| c.kind == McpServerKind::Stdio).count(),
        2
    );
    assert_eq!(
        core.iter().filter(|c| c.kind == McpServerKind::Remote).count(),
        1
    );

    let context = load_cards_by_publisher(&conn, "context").unwrap();
    assert_eq!(context.len(), 4);
    assert_eq!(context[0].id, "context7");

    // GitHub's remote row must ask for its Authorization header — a token,
    // marked secret so the install form masks it.
    let github_row = load_full_server(&conn, "github").unwrap().unwrap();
    assert_eq!(github_row.remotes[0].required_headers, ["Authorization"]);
    assert!(github_row.remotes[0].headers[0].input.is_secret);

    let browser = load_cards_by_publisher(&conn, "browser").unwrap();
    assert_eq!(browser.len(), 2);
    assert!(browser.iter().all(|c| c.kind == McpServerKind::Stdio));

    let creative = load_cards_by_publisher(&conn, "creative").unwrap();
    assert_eq!(creative.len(), 4);
    assert_eq!(creative[0].id, "figma");
    assert_eq!(
        creative
            .iter()
            .filter(|c| c.kind == McpServerKind::Remote)
            .count(),
        2
    );

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
