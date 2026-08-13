//! Source registry, URL building and multi-source merge coverage.
//!
//! Network-touching behaviour stays behind the `#[ignore]` smoke test; the
//! merge and URL logic is pure and covered unconditionally.

use super::merge::{SourceServers, merge_catalogs};
use super::sources::{GITHUB_SOURCE_ID, OFFICIAL_SOURCE_ID, builtin_sources};
use super::*;
use crate::mcp_models::{
    McpCursorStyle, McpRegistryPackageSummary, McpRegistryRemoteSummary, McpRegistryServer,
    McpServerKind, McpServerStatus,
};

fn server(namespace: &str, id: &str) -> McpRegistryServer {
    McpRegistryServer {
        id: id.into(),
        name: namespace.rsplit('/').next().unwrap_or(namespace).into(),
        namespace: namespace.into(),
        ..Default::default()
    }
}

#[test]
fn encodes_cursor_safely() {
    assert_eq!(fetch::urlencoding_minimal("abc-123_X.~"), "abc-123_X.~");
    assert_eq!(fetch::urlencoding_minimal("a b/c="), "a%20b%2Fc%3D");
}

/// The `/v0` GitHub endpoint answers with `Deprecation: true`, and the official
/// registry documents `/v0.1` as current. Both built-ins must be on `/v0.1`.
#[test]
fn builtin_sources_target_v0_1_and_declare_their_licence() {
    let sources = builtin_sources();
    let official = sources
        .iter()
        .find(|s| s.id == OFFICIAL_SOURCE_ID)
        .expect("official source");
    assert_eq!(
        official.base_url,
        "https://registry.modelcontextprotocol.io/v0.1/servers"
    );
    assert_eq!(official.cursor_style, McpCursorStyle::Camel);
    assert_eq!(official.license, McpSourceLicense::Cc0);
    assert!(
        official.mirrorable,
        "CC0 is the only licence that lets us keep a long-lived local mirror"
    );

    let github = sources
        .iter()
        .find(|s| s.id == GITHUB_SOURCE_ID)
        .expect("github source");
    assert!(github.base_url.contains("/v0.1/servers"));
    assert!(!github.base_url.contains("/v0/servers"));
    assert_eq!(github.cursor_style, McpCursorStyle::Snake);
    assert!(
        !github.mirrorable,
        "GitHub's mirror publishes no redistribution terms"
    );

    // The official registry outranks the mirror in the merge.
    assert!(official.priority < github.priority);
    assert!(sources.iter().all(|s| !s.requires_key));
}

#[test]
fn page_url_carries_limit_extra_query_and_encoded_cursor() {
    let sources = builtin_sources();
    let official = sources.iter().find(|s| s.id == OFFICIAL_SOURCE_ID).unwrap();
    let url = fetch::test_page_url(official, Some("acme/x:1.0.1"), true);
    assert_eq!(
        url,
        "https://registry.modelcontextprotocol.io/v0.1/servers?limit=100&version=latest&cursor=acme%2Fx%3A1.0.1"
    );
    // The fallback path drops the extra query but keeps everything else.
    let fallback = fetch::test_page_url(official, None, false);
    assert_eq!(
        fallback,
        "https://registry.modelcontextprotocol.io/v0.1/servers?limit=100"
    );
}

#[test]
fn source_host_is_extracted_for_diagnostics() {
    let sources = builtin_sources();
    let github = sources.iter().find(|s| s.id == GITHUB_SOURCE_ID).unwrap();
    assert_eq!(github.source_host(), "api.mcp.github.com");
}

/// The same server from two sources becomes one row: the official registry
/// defines the spec, GitHub contributes the enrichment it alone carries.
#[test]
fn merge_keys_on_reverse_dns_name_and_keeps_each_source_contribution() {
    let mut official = server("io.github.acme/tool", "uuid-official");
    official.description = "Official description.".into();
    official.version = Some("1.2.0".into());
    official.status = McpServerStatus::Deprecated;
    official.remotes = vec![McpRegistryRemoteSummary {
        transport: "http".into(),
        url: "https://acme.example/mcp".into(),
        ..Default::default()
    }];

    let mut github = server("io.github.acme/tool", "gh-123");
    github.stars = 4200;
    github.license = Some("Apache-2.0".into());
    github.readme = Some("# Tool".into());
    github.description = "Mirror description.".into();

    let merged = merge_catalogs(vec![
        SourceServers {
            source_id: "official".into(),
            priority: 0,
            servers: vec![official],
        },
        SourceServers {
            source_id: "github".into(),
            priority: 10,
            servers: vec![github],
        },
    ]);

    assert_eq!(merged.len(), 1, "one server, not two");
    let row = &merged[0];
    // Identity is the reverse-DNS name, so it survives a source's id churn.
    assert_eq!(row.id, "io.github.acme/tool");
    assert_eq!(row.registry_source.as_deref(), Some("official"));
    assert_eq!(
        row.contributing_sources,
        vec!["official".to_string(), "github".to_string()]
    );
    // Official wins the fields it owns…
    assert_eq!(row.description, "Official description.");
    assert_eq!(row.status, McpServerStatus::Deprecated);
    assert_eq!(row.kind, McpServerKind::Remote);
    // …GitHub fills the ones only it has.
    assert_eq!(row.stars, 4200);
    assert_eq!(row.license.as_deref(), Some("Apache-2.0"));
    assert_eq!(row.readme.as_deref(), Some("# Tool"));
}

/// The official registry publishes one row per version. Only the newest may
/// reach the catalog, following the documented aggregator rules.
#[test]
fn merge_collapses_versions_within_one_source() {
    let mut old = server("com.example/srv", "v1");
    old.version = Some("1.0.0".into());
    old.is_latest = false;
    old.published_at = Some("2026-01-01T00:00:00Z".into());
    old.description = "old".into();

    let mut newer = server("com.example/srv", "v2");
    newer.version = Some("2.0.1".into());
    newer.is_latest = false;
    newer.published_at = Some("2026-05-01T00:00:00Z".into());
    newer.description = "new".into();

    let merged = merge_catalogs(vec![SourceServers {
        source_id: "official".into(),
        priority: 0,
        servers: vec![old, newer],
    }]);
    assert_eq!(merged.len(), 1);
    assert_eq!(merged[0].version.as_deref(), Some("2.0.1"));
    assert_eq!(merged[0].description, "new");
}

/// Rule 1 of the aggregator version rules: an explicit `isLatest` beats a
/// higher semver, because the registry knows something we don't.
#[test]
fn is_latest_outranks_a_higher_semver() {
    let mut flagged = server("com.example/srv", "flagged");
    flagged.version = Some("1.0.0".into());
    flagged.is_latest = true;

    let mut higher = server("com.example/srv", "higher");
    higher.version = Some("9.9.9".into());
    higher.is_latest = false;

    let merged = merge_catalogs(vec![SourceServers {
        source_id: "official".into(),
        priority: 0,
        servers: vec![higher, flagged],
    }]);
    assert_eq!(merged[0].version.as_deref(), Some("1.0.0"));
    assert!(merged[0].is_latest);
}

/// Rule 4: a parseable semver beats an unparseable version string; with
/// neither parseable, `publishedAt` decides.
#[test]
fn version_ordering_falls_back_to_semver_then_published_at() {
    let mut semver = server("com.example/srv", "semver");
    semver.version = Some("0.0.1".into());
    semver.is_latest = false;
    let mut garbage = server("com.example/srv", "garbage");
    garbage.version = Some("nightly".into());
    garbage.is_latest = false;
    let merged = merge_catalogs(vec![SourceServers {
        source_id: "official".into(),
        priority: 0,
        servers: vec![garbage, semver],
    }]);
    assert_eq!(merged[0].version.as_deref(), Some("0.0.1"));

    let mut older = server("com.example/other", "older");
    older.version = Some("nightly".into());
    older.is_latest = false;
    older.published_at = Some("2026-01-01T00:00:00Z".into());
    let mut newer = server("com.example/other", "newer");
    newer.version = Some("nightly".into());
    newer.is_latest = false;
    newer.published_at = Some("2026-07-01T00:00:00Z".into());
    let merged = merge_catalogs(vec![SourceServers {
        source_id: "official".into(),
        priority: 0,
        servers: vec![older, newer],
    }]);
    assert_eq!(merged[0].id, "com.example/other");
    assert_eq!(
        merged[0].published_at.as_deref(),
        Some("2026-07-01T00:00:00Z")
    );
}

/// A pre-release must not outrank its own final release.
#[test]
fn prerelease_loses_to_its_final_release() {
    let mut rc = server("com.example/srv", "rc");
    rc.version = Some("2.0.0-rc.1".into());
    rc.is_latest = false;
    let mut final_release = server("com.example/srv", "final");
    final_release.version = Some("2.0.0".into());
    final_release.is_latest = false;

    let merged = merge_catalogs(vec![SourceServers {
        source_id: "official".into(),
        priority: 0,
        servers: vec![rc, final_release],
    }]);
    assert_eq!(merged[0].version.as_deref(), Some("2.0.0"));
}

/// Same name, same version, same authority → the record carrying more usable
/// information wins ("同名取信息更全者").
#[test]
fn ties_are_broken_by_completeness() {
    let mut sparse = server("com.example/srv", "sparse");
    sparse.version = Some("1.0.0".into());
    let mut rich = server("com.example/srv", "rich");
    rich.version = Some("1.0.0".into());
    rich.description = "described".into();
    rich.repo_url = "https://example.com/repo".into();
    rich.packages = vec![McpRegistryPackageSummary {
        runtime: "npx".into(),
        identifier: "@example/srv".into(),
        ..Default::default()
    }];

    let merged = merge_catalogs(vec![SourceServers {
        source_id: "custom:local".into(),
        priority: 50,
        servers: vec![sparse, rich],
    }]);
    assert_eq!(merged.len(), 1);
    assert_eq!(merged[0].description, "described");
    assert_eq!(merged[0].kind, McpServerKind::Stdio);
}

/// A deprecation reported by any source is kept: showing a deprecated server
/// as healthy is the worse failure.
#[test]
fn deprecation_from_any_source_survives_the_merge() {
    let active = server("com.example/srv", "a");
    let mut deprecated = server("com.example/srv", "b");
    deprecated.status = McpServerStatus::Deprecated;

    let merged = merge_catalogs(vec![
        SourceServers {
            source_id: "official".into(),
            priority: 0,
            servers: vec![active],
        },
        SourceServers {
            source_id: "github".into(),
            priority: 10,
            servers: vec![deprecated],
        },
    ]);
    assert_eq!(merged[0].status, McpServerStatus::Deprecated);
}

#[test]
fn merge_output_is_sorted_and_stable() {
    let merged = merge_catalogs(vec![SourceServers {
        source_id: "official".into(),
        priority: 0,
        servers: vec![
            server("z.example/last", "1"),
            server("a.example/first", "2"),
            server("m.example/middle", "3"),
        ],
    }]);
    let ids: Vec<&str> = merged.iter().map(|s| s.id.as_str()).collect();
    assert_eq!(
        ids,
        vec!["a.example/first", "m.example/middle", "z.example/last"]
    );
}

#[test]
fn degraded_reason_summarizes_failures_and_truncation() {
    let outcomes = vec![
        McpSourceOutcome {
            source_id: "official".into(),
            display_name: "Official".into(),
            source_host: "registry.modelcontextprotocol.io".into(),
            payload_sha256: Some("abc".into()),
            etag: None,
            unchanged: false,
            server_count: 10,
            error: None,
            degraded_reason: Some("official: pagination hit the cap".into()),
        },
        McpSourceOutcome {
            source_id: "github".into(),
            display_name: "GitHub".into(),
            source_host: "api.mcp.github.com".into(),
            payload_sha256: None,
            etag: None,
            unchanged: false,
            server_count: 0,
            error: Some("429".into()),
            degraded_reason: None,
        },
    ];
    let reason = degraded_reason(&outcomes).expect("degraded");
    assert!(reason.contains("official: pagination hit the cap"));
    assert!(reason.contains("github unavailable: 429"));

    let clean = vec![McpSourceOutcome {
        error: None,
        degraded_reason: None,
        ..outcomes[0].clone()
    }];
    assert!(degraded_reason(&clean).is_none());
}

/// End-to-end smoke test against every live source. Network-gated (run
/// explicitly with `--ignored`): `cargo test -p skillstar-marketplace
/// -- --ignored fetch_real_registry`.
#[tokio::test]
#[ignore = "hits the network; run with --ignored"]
async fn fetch_real_registry_returns_many_servers() {
    let fetched = fetch_mcp_catalog(&std::collections::HashMap::new())
        .await
        .expect("fetch live MCP catalog");
    for outcome in &fetched.outcomes {
        println!(
            "source={} host={} servers={} unchanged={} error={:?} degraded={:?}",
            outcome.source_id,
            outcome.source_host,
            outcome.server_count,
            outcome.unchanged,
            outcome.error,
            outcome.degraded_reason
        );
    }
    println!("merged={} servers", fetched.servers.len());
    let deprecated = fetched
        .servers
        .iter()
        .filter(|s| s.status == McpServerStatus::Deprecated)
        .count();
    let multi_source = fetched
        .servers
        .iter()
        .filter(|s| s.contributing_sources.len() > 1)
        .count();
    println!("deprecated={deprecated} merged_from_multiple_sources={multi_source}");

    let (servers, meta) = (fetched.servers, fetched.meta);
    assert!(
        servers.len() > 20,
        "expected >20 servers, got {}",
        servers.len()
    );
    assert!(
        servers.iter().any(|s| !s.packages.is_empty()),
        "expected at least one stdio-package server"
    );
    assert!(
        servers
            .iter()
            .all(|s| !s.name.is_empty() && !s.raw_server_json.is_empty()),
        "every server should have a name and raw json for install mapping"
    );
    assert!(
        meta.payload_sha256.len() == 64,
        "content-addressed payload hash should be a 64-char sha256"
    );
}
