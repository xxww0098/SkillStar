use super::*;
use crate::snapshot::*;

#[test]
fn curated_registry_migrates_v2_database_to_current_version() {
    with_temp_data_root(|temp_root| {
        let path = temp_root.join("marketplace.db");
        let conn = open_raw_conn(&path);
        conn.execute_batch(
            "CREATE TABLE marketplace_skill (
                skill_key TEXT PRIMARY KEY,
                source TEXT NOT NULL,
                name TEXT NOT NULL,
                git_url TEXT NOT NULL DEFAULT '',
                author TEXT,
                publisher_name TEXT,
                repo_name TEXT,
                description TEXT NOT NULL DEFAULT '',
                installs INTEGER NOT NULL DEFAULT 0,
                last_seen_remote_at TEXT,
                last_list_sync_at TEXT
            );
            PRAGMA user_version = 2;",
        )
        .expect("seed v2 schema marker");
        drop(conn);

        let conn = create_connection().expect("create migrated marketplace connection");
        let version: i64 = conn
            .pragma_query_value(None, "user_version", |row| row.get(0))
            .expect("read user_version");
        assert_eq!(version, SNAPSHOT_SCHEMA_VERSION);

        let count: i64 = conn
            .query_row(
                "SELECT COUNT(1) FROM marketplace_curated_registry",
                [],
                |row| row.get(0),
            )
            .expect("count curated registry rows");
        assert_eq!(count, 1);
    });
}

#[test]
fn leaderboard_round_trip_reads_inserted_rows() {
    with_temp_data_root(|_| {
        let conn = create_connection().expect("create marketplace connection");
        let tx = conn.unchecked_transaction().expect("start tx");
        let synced_at = now_rfc3339();
        let scope = leaderboard_scope("all");

        let skill_key =
            upsert_skill_identity_in_tx(&tx, "openai/skills", "screenshot", 42, &synced_at)
                .expect("upsert snapshot skill")
                .expect("skill key");
        tx.execute(
            "INSERT INTO marketplace_listing (listing_type, skill_key, rank, updated_at)
             VALUES (?1, ?2, 1, ?3)",
            rusqlite::params![scope, skill_key, synced_at],
        )
        .expect("insert leaderboard row");
        mark_scope_success_in_tx(&tx, &scope).expect("mark scope success");
        tx.commit().expect("commit leaderboard snapshot");

        let rows = load_leaderboard_snapshot(&conn, &scope).expect("load leaderboard snapshot");
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].name, "screenshot");
        assert_eq!(rows[0].rank, Some(1));
        assert!(
            scope_updated_at(&conn, &scope)
                .expect("scope updated at")
                .is_some()
        );
    });
}

#[test]
fn search_prefers_exact_name_match_before_description_only_match() {
    with_temp_data_root(|_| {
        let conn = create_connection().expect("create marketplace connection");
        let tx = conn.unchecked_transaction().expect("start tx");
        let synced_at = now_rfc3339();

        let exact_key = upsert_skill_identity_in_tx(&tx, "openai/skills", "search", 10, &synced_at)
            .expect("upsert exact")
            .expect("exact key");
        tx.execute(
            "UPDATE marketplace_skill SET description = 'Exact skill' WHERE skill_key = ?1",
            [exact_key.as_str()],
        )
        .expect("update exact description");
        crate::snapshot::refresh_fts_entry_in_tx(&tx, &exact_key).expect("refresh exact fts");

        let desc_key =
            upsert_skill_identity_in_tx(&tx, "openai/skills", "assistant", 999, &synced_at)
                .expect("upsert description")
                .expect("description key");
        tx.execute(
            "UPDATE marketplace_skill SET description = 'search helper utility' WHERE skill_key = ?1",
            [desc_key.as_str()],
        )
        .expect("update desc description");
        crate::snapshot::refresh_fts_entry_in_tx(&tx, &desc_key).expect("refresh desc fts");
        tx.commit().expect("commit search fixtures");

        let results = load_search_snapshot(&conn, "search", 10)
            .expect("run search snapshot")
            .0;
        assert_eq!(results.len(), 2);
        assert_eq!(results[0].name, "search");
    });
}

#[test]
fn search_handles_hyphenated_query_tokens() {
    with_temp_data_root(|_| {
        let conn = create_connection().expect("create marketplace connection");
        let tx = conn.unchecked_transaction().expect("start tx");
        let synced_at = now_rfc3339();

        let skill_key =
            upsert_skill_identity_in_tx(&tx, "openai/skills", "ui-design-system", 12, &synced_at)
                .expect("upsert hyphenated skill")
                .expect("hyphenated skill key");
        tx.execute(
            "UPDATE marketplace_skill SET description = 'Design polished UI systems' WHERE skill_key = ?1",
            [skill_key.as_str()],
        )
        .expect("update hyphenated description");
        crate::snapshot::refresh_fts_entry_in_tx(&tx, &skill_key).expect("refresh hyphenated fts");
        tx.commit().expect("commit hyphenated fixtures");

        let results = load_search_snapshot(&conn, "ui-design", 10)
            .expect("run hyphenated search")
            .0;
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].name, "ui-design-system");
    });
}

#[test]
fn source_resolution_prefers_repo_affinity_before_popularity() {
    with_temp_data_root(|_| {
        let conn = create_connection().expect("create marketplace connection");
        let tx = conn.unchecked_transaction().expect("start tx");
        let synced_at = now_rfc3339();

        upsert_skill_identity_in_tx(&tx, "openai/skills", "search", 10, &synced_at)
            .expect("upsert preferred repo");
        upsert_skill_identity_in_tx(&tx, "vercel/ai", "search", 999, &synced_at)
            .expect("upsert popular repo");
        tx.commit().expect("commit source fixtures");

        let requests = vec![ResolveSkillRequest {
            original_name: "search".to_string(),
            normalized_name: "search".to_string(),
        }];
        let named_sources = HashMap::new();
        let preferred_repos = HashSet::from(["openai/skills".to_string()]);

        let resolved =
            resolve_skill_sources_from_snapshot(&conn, &requests, &named_sources, &preferred_repos)
                .expect("resolve from snapshot");

        assert_eq!(
            resolved.get("search"),
            Some(&"https://github.com/openai/skills".to_string())
        );
    });
}

#[test]
fn source_resolution_requires_unique_top_candidate_when_ambiguous() {
    with_temp_data_root(|_| {
        let conn = create_connection().expect("create marketplace connection");
        let tx = conn.unchecked_transaction().expect("start tx");
        let synced_at = now_rfc3339();

        upsert_skill_identity_in_tx(&tx, "openai/skills", "search", 100, &synced_at)
            .expect("upsert first candidate");
        upsert_skill_identity_in_tx(&tx, "vercel/ai", "search", 100, &synced_at)
            .expect("upsert second candidate");
        tx.commit().expect("commit ambiguous fixtures");

        let requests = vec![ResolveSkillRequest {
            original_name: "search".to_string(),
            normalized_name: "search".to_string(),
        }];

        let resolved =
            resolve_skill_sources_from_snapshot(&conn, &requests, &HashMap::new(), &HashSet::new())
                .expect("resolve ambiguous snapshot");

        assert!(!resolved.contains_key("search"));
    });
}

#[test]
fn detail_snapshot_reads_cached_payload() {
    with_temp_data_root(|_| {
        let conn = create_connection().expect("create marketplace connection");
        let tx = conn.unchecked_transaction().expect("start tx");
        let synced_at = now_rfc3339();
        let skill_key =
            upsert_skill_identity_in_tx(&tx, "openai/skills", "screenshot", 0, &synced_at)
                .expect("upsert detail identity")
                .expect("detail key");
        tx.execute(
            "INSERT INTO marketplace_skill_detail (
                skill_key,
                summary,
                readme,
                weekly_installs,
                github_stars,
                first_seen,
                security_audits_json,
                last_detail_sync_at
            ) VALUES (?1, 'summary', '# readme', '1.2K', 12, 'Apr 1, 2026', '[]', ?2)",
            rusqlite::params![skill_key, synced_at],
        )
        .expect("insert detail snapshot");
        tx.commit().expect("commit detail snapshot");

        let detail = load_skill_detail_snapshot(
            &conn,
            &build_skill_key("openai/skills", "screenshot").expect("detail key"),
        )
        .expect("load detail snapshot")
        .expect("detail exists");

        assert_eq!(detail.summary.as_deref(), Some("summary"));
        assert_eq!(detail.github_stars, Some(12));
    });
}

#[test]
fn v11_migration_adds_content_addressing_columns() {
    with_temp_data_root(|temp_root| {
        // Seed a v10 schema (sync_state without the new columns) by creating
        // the v10-era table manually, then let the full migration run.
        let path = temp_root.join("marketplace.db");
        let conn = open_raw_conn(&path);
        conn.execute_batch(
            "CREATE TABLE marketplace_skill (
                skill_key TEXT PRIMARY KEY,
                source TEXT NOT NULL,
                name TEXT NOT NULL,
                git_url TEXT NOT NULL DEFAULT '',
                author TEXT,
                publisher_name TEXT,
                repo_name TEXT,
                description TEXT NOT NULL DEFAULT '',
                installs INTEGER NOT NULL DEFAULT 0,
                last_seen_remote_at TEXT,
                last_list_sync_at TEXT
            );
            CREATE TABLE marketplace_sync_state (
                scope TEXT PRIMARY KEY,
                last_success_at TEXT,
                last_attempt_at TEXT,
                last_error TEXT,
                next_refresh_at TEXT,
                schema_version INTEGER NOT NULL DEFAULT 1
            );
            PRAGMA user_version = 10;",
        )
        .expect("seed v10 schema marker");
        drop(conn);

        let conn = create_connection().expect("create migrated marketplace connection");
        let version: i64 = conn
            .pragma_query_value(None, "user_version", |row| row.get(0))
            .expect("read user_version");
        assert_eq!(version, SNAPSHOT_SCHEMA_VERSION);

        // New columns exist and are nullable.
        let columns: Vec<String> = conn
            .prepare("PRAGMA table_info(marketplace_sync_state)")
            .expect("pragma")
            .query_map([], |row| row.get(1))
            .expect("column names")
            .collect::<Result<_, _>>()
            .expect("collect columns");
        for expected in ["source_host", "payload_sha256", "etag"] {
            assert!(columns.iter().any(|c| c == expected), "missing {expected}");
        }
    });
}

#[test]
fn mark_scope_success_with_meta_records_and_preserves_fingerprint() {
    with_temp_data_root(|_| {
        let conn = create_connection().expect("create marketplace connection");
        let tx = conn.unchecked_transaction().expect("open tx");

        let meta = crate::remote::FetchMeta {
            payload_sha256: "a".repeat(64),
            source_host: "https://mirror.example/".into(),
            etag: Some("\"v1\"".into()),
            degraded: false,
        };
        mark_scope_success_with_meta_in_tx(&tx, "leaderboard_all", &meta, false)
            .expect("record changed fingerprint");
        tx.commit().expect("commit");

        let state = scope_sync_state(&conn, "leaderboard_all")
            .expect("read state")
            .expect("state exists");
        assert_eq!(
            state.payload_sha256.as_deref(),
            Some("a".repeat(64).as_str())
        );
        assert_eq!(
            state.source_host.as_deref(),
            Some("https://mirror.example/")
        );
        assert_eq!(state.etag.as_deref(), Some("\"v1\""));

        // No-change refresh preserves the stored fingerprint.
        let tx2 = conn.unchecked_transaction().expect("open tx2");
        let unchanged_meta = crate::remote::FetchMeta {
            payload_sha256: String::new(),
            source_host: "https://skills.sh".into(),
            etag: None,
            degraded: false,
        };
        mark_scope_success_with_meta_in_tx(&tx2, "leaderboard_all", &unchanged_meta, true)
            .expect("record unchanged");
        tx2.commit().expect("commit tx2");

        let state2 = scope_sync_state(&conn, "leaderboard_all")
            .expect("read state2")
            .expect("state2 exists");
        assert_eq!(
            state2.payload_sha256.as_deref(),
            Some("a".repeat(64).as_str()),
            "unchanged refresh must keep the old fingerprint"
        );
        assert_eq!(
            state2.source_host.as_deref(),
            Some("https://mirror.example/"),
            "unchanged refresh must keep the old source host"
        );
    });
}

#[test]
fn payload_unchanged_detects_identical_and_304_fetches() {
    let meta_changed = crate::remote::FetchMeta {
        payload_sha256: "b".repeat(64),
        source_host: "https://skills.sh".into(),
        etag: None,
        degraded: false,
    };
    assert!(!payload_unchanged(&meta_changed, &Some("a".repeat(64))));
    assert!(payload_unchanged(&meta_changed, &Some("b".repeat(64))));

    // Empty sha256 = 304 Not Modified.
    let meta_304 = crate::remote::FetchMeta {
        payload_sha256: String::new(),
        source_host: "https://skills.sh".into(),
        etag: None,
        degraded: false,
    };
    assert!(payload_unchanged(&meta_304, &None));
    assert!(payload_unchanged(&meta_304, &Some("b".repeat(64))));
}
