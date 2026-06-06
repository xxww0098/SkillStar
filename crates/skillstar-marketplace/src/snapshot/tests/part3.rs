use super::*;
use crate::models::*;
use crate::snapshot::*;

#[test]
fn phase3_metadata_coexists_with_canonical_search_and_listing() {
    with_temp_data_root(|_| {
        let conn = create_connection().expect("create marketplace connection");
        let tx = conn.unchecked_transaction().expect("start tx");
        let synced_at = now_rfc3339();
        let scope = leaderboard_scope("all");
        let skill_key =
            upsert_skill_identity_in_tx(&tx, "openai/skills", "search", 77, &synced_at)
                .expect("upsert canonical skill")
                .expect("skill key");
        tx.execute(
            "UPDATE marketplace_skill SET description = 'Search helper for AI agents' WHERE skill_key = ?1",
            [skill_key.as_str()],
        )
        .expect("update canonical description");
        crate::snapshot::refresh_fts_entry_in_tx(&tx, &skill_key).expect("refresh fts");
        tx.execute(
            "INSERT INTO marketplace_listing (listing_type, skill_key, rank, updated_at)
             VALUES (?1, ?2, 1, ?3)",
            rusqlite::params![scope, skill_key, synced_at],
        )
        .expect("insert leaderboard row");
        mark_scope_success_in_tx(&tx, &scope).expect("mark scope success");
        tx.commit().expect("commit canonical fixtures");

        upsert_curated_registry(CuratedRegistryUpsert {
            id: "Team.Registry".to_string(),
            name: "Team Registry".to_string(),
            kind: CuratedRegistryKind::GitHub,
            endpoint: "https://github.com/team/registry".to_string(),
            enabled: true,
            priority: 5,
            trust: "team".to_string(),
            last_sync_at: Some("2026-04-01T00:00:00Z".to_string()),
            last_error: None,
        })
        .expect("upsert curated registry");
        crate::snapshot::upsert_source_observation(MarketplaceSourceObservationUpsert {
            source_id: "Team.Registry".to_string(),
            source_skill_id: "Search".to_string(),
            skill_key: skill_key.clone(),
            source_url: "https://registry.example/skills/search".to_string(),
            repo_url: "https://github.com/openai/skills".to_string(),
            version: Some("1.2.3".to_string()),
            sha: Some("abc123".to_string()),
            metadata_json: Some("{\"quality\":\"curated\"}".to_string()),
            fetched_at: Some("2026-04-01T00:00:00Z".to_string()),
        })
        .expect("upsert source observation");
        upsert_rating_summary(MarketplaceRatingSummaryUpsert {
            skill_key: skill_key.clone(),
            source_id: Some("Team.Registry".to_string()),
            rating_avg: 4.8,
            rating_count: 12,
            review_count: 3,
            last_review_at: Some("2026-04-02T00:00:00Z".to_string()),
        })
        .expect("upsert rating summary");
        upsert_review(MarketplaceReviewUpsert {
            review_id: "phase3-review-1".to_string(),
            skill_key: skill_key.clone(),
            source_id: Some("Team.Registry".to_string()),
            author_hash: Some("reviewer".to_string()),
            rating: 5,
            title: Some("Reliable".to_string()),
            body: Some("Works well with canonical search".to_string()),
            locale: Some("en".to_string()),
            status: Some("published".to_string()),
            reviewed_at: Some("2026-04-02T00:00:00Z".to_string()),
        })
        .expect("upsert review");
        upsert_category(MarketplaceCategoryUpsert {
            label: "AI Agents".to_string(),
            slug: None,
            parent_id: None,
            position: 1,
        })
        .expect("upsert category");
        crate::snapshot::assign_categories_to_skill(MarketplaceSkillCategoryAssignmentInput {
            skill_key: skill_key.clone(),
            category_ids: vec!["ai-agents".to_string()],
        })
        .expect("assign category");
        upsert_tag(MarketplaceTagUpsert {
            label: "Search Tools".to_string(),
            slug: None,
        })
        .expect("upsert tag");
        crate::snapshot::assign_tags_to_skill(MarketplaceSkillTagAssignmentInput {
            skill_key: skill_key.clone(),
            tag_slugs: vec!["search-tools".to_string()],
            source_id: Some("Team.Registry".to_string()),
        })
        .expect("assign tag");
        upsert_update_notification(MarketplaceUpdateNotificationUpsert {
            skill_key: skill_key.clone(),
            source_id: "Team.Registry".to_string(),
            installed_version: Some("1.2.3".to_string()),
            available_version: Some("1.3.0".to_string()),
            installed_hash: Some("old".to_string()),
            available_hash: Some("new".to_string()),
            detected_at: Some("2026-04-03T00:00:00Z".to_string()),
            message: Some("Team registry update available".to_string()),
            metadata_json: Some("{\"severity\":\"info\"}".to_string()),
        })
        .expect("upsert update notification");

        let registries = list_curated_registries().expect("list curated registries");
        assert!(registries.iter().any(|entry| entry.id == "team_registry"));
        assert_eq!(
            crate::snapshot::list_source_observations_for_skill(&skill_key)
                .expect("list observations")
                .len(),
            2
        );
        assert_eq!(
            list_rating_summaries_for_skill(&skill_key).expect("list rating summaries")[0]
                .source_id
                .as_deref(),
            Some("team_registry")
        );
        assert_eq!(
            list_reviews_for_skill(&skill_key).expect("list reviews")[0].review_id,
            "phase3-review-1"
        );
        assert_eq!(
            crate::snapshot::list_categories_for_skill(&skill_key).expect("list skill categories")[0]
                .category_id,
            "ai-agents"
        );
        assert_eq!(
            crate::snapshot::list_tags_for_skill(&skill_key).expect("list skill tags")[0].tag_slug,
            "search-tools"
        );
        assert_eq!(
            list_update_notifications_for_skill(&skill_key, false)
                .expect("list skill notifications")[0]
                .source_id,
            "team_registry"
        );

        let search_results = load_search_snapshot(&conn, "search", 10)
            .expect("run search snapshot")
            .0;
        assert_eq!(search_results.len(), 1);
        assert_eq!(search_results[0].name, "search");
        assert_eq!(search_results[0].source.as_deref(), Some("openai/skills"));

        let listing = load_leaderboard_snapshot(&conn, &scope).expect("load leaderboard");
        assert_eq!(listing.len(), 1);
        assert_eq!(listing[0].name, "search");
        assert_eq!(listing[0].source.as_deref(), Some("openai/skills"));
        assert_eq!(listing[0].rank, Some(1));
    });
}

#[test]
fn invalid_rating_values_are_rejected() {
    with_temp_data_root(|_| {
        create_connection().expect("create marketplace connection");

        assert!(
            upsert_rating_summary(MarketplaceRatingSummaryUpsert {
                skill_key: "openai/skills/search".to_string(),
                source_id: None,
                rating_avg: 6.0,
                rating_count: 1,
                review_count: 1,
                last_review_at: None,
            })
            .is_err()
        );

        assert!(
            upsert_review(MarketplaceReviewUpsert {
                review_id: "review-bad".to_string(),
                skill_key: "openai/skills/search".to_string(),
                source_id: None,
                author_hash: None,
                rating: 0,
                title: None,
                body: None,
                locale: None,
                status: None,
                reviewed_at: None,
            })
            .is_err()
        );
    });
}

#[test]
fn empty_skill_key_lists_return_empty_vectors() {
    with_temp_data_root(|_| {
        create_connection().expect("create marketplace connection");

        assert!(
            list_rating_summaries_for_skill("")
                .expect("empty rating list")
                .is_empty()
        );
        assert!(
            list_reviews_for_skill("")
                .expect("empty review list")
                .is_empty()
        );
    });
}

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
fn curated_registry_upsert_updates_and_lists_by_priority() {
    with_temp_data_root(|_| {
        create_connection().expect("create marketplace connection");

        let custom = upsert_curated_registry(CuratedRegistryUpsert {
            id: "Team.Source".to_string(),
            name: "Team Source".to_string(),
            kind: CuratedRegistryKind::GitHub,
            endpoint: " https://github.com/acme/skills ".to_string(),
            enabled: true,
            priority: 10,
            trust: "team".to_string(),
            last_sync_at: Some("2026-04-01T00:00:00Z".to_string()),
            last_error: None,
        })
        .expect("upsert custom curated registry");
        assert_eq!(custom.id, "team_source");
        assert_eq!(custom.endpoint, "https://github.com/acme/skills");

        let updated = upsert_curated_registry(CuratedRegistryUpsert {
            id: "team_source".to_string(),
            name: "Team Source Disabled".to_string(),
            kind: CuratedRegistryKind::Custom,
            endpoint: "file:///tmp/registry.json".to_string(),
            enabled: false,
            priority: 1,
            trust: "internal".to_string(),
            last_sync_at: None,
            last_error: Some("paused".to_string()),
        })
        .expect("update custom curated registry");

        assert_eq!(updated.name, "Team Source Disabled");
        assert_eq!(updated.kind, CuratedRegistryKind::Custom);
        assert!(!updated.enabled);
        assert_eq!(updated.last_error.as_deref(), Some("paused"));

        let entries = list_curated_registries().expect("list curated registries");
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].id, "skills_sh");
        assert_eq!(entries[1].id, "team_source");
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

        let exact_key =
            upsert_skill_identity_in_tx(&tx, "openai/skills", "search", 10, &synced_at)
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

        let skill_key = upsert_skill_identity_in_tx(
            &tx,
            "openai/skills",
            "ui-design-system",
            12,
            &synced_at,
        )
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
fn multi_source_upsert_and_list_observations_for_one_skill() {
    with_temp_data_root(|_| {
        let conn = create_connection().expect("create marketplace connection");
        let tx = conn.unchecked_transaction().expect("start tx");
        let synced_at = now_rfc3339();
        let skill_key =
            upsert_skill_identity_in_tx(&tx, "openai/skills", "screenshot", 42, &synced_at)
                .expect("upsert canonical skill")
                .expect("skill key");
        tx.commit().expect("commit canonical skill");

        let custom = crate::snapshot::upsert_source_observation(MarketplaceSourceObservationUpsert {
            source_id: "Team.Registry".to_string(),
            source_skill_id: "Screenshot".to_string(),
            skill_key: skill_key.clone(),
            source_url: " file:///tmp/team.json ".to_string(),
            repo_url: " https://github.com/openai/skills ".to_string(),
            version: Some("1.2.3".to_string()),
            sha: Some("abc123".to_string()),
            metadata_json: Some("{\"trust\":\"team\"}".to_string()),
            fetched_at: Some("2026-04-01T00:00:00Z".to_string()),
        })
        .expect("upsert custom observation");
        assert_eq!(custom.source_id, "team_registry");
        assert_eq!(custom.source_skill_id, "screenshot");
        assert_eq!(custom.skill_key, skill_key);
        assert_eq!(custom.repo_url, "https://github.com/openai/skills");

        let observations = crate::snapshot::list_source_observations_for_skill(&skill_key)
            .expect("list observations for skill");
        assert_eq!(observations.len(), 2);
        assert_eq!(observations[0].source_id, "skills_sh");
        assert_eq!(observations[1].source_id, "team_registry");

        let sources = crate::snapshot::list_known_marketplace_sources().expect("list known sources");
        assert_eq!(sources.len(), 2);
        assert_eq!(sources[0].source_id, "skills_sh");
        assert_eq!(sources[0].observation_count, 1);
        assert_eq!(sources[1].source_id, "team_registry");
    });
}

#[test]
fn multi_source_canonical_search_compatibility_stays_intact() {
    with_temp_data_root(|_| {
        let conn = create_connection().expect("create marketplace connection");
        let tx = conn.unchecked_transaction().expect("start tx");
        let synced_at = now_rfc3339();

        let skill_key =
            upsert_skill_identity_in_tx(&tx, "openai/skills", "search", 10, &synced_at)
                .expect("upsert canonical skill")
                .expect("skill key");
        crate::snapshot::upsert_source_observation_in_tx(
            &tx,
            MarketplaceSourceObservationUpsert {
                source_id: "team_registry".to_string(),
                source_skill_id: "search".to_string(),
                skill_key: skill_key.clone(),
                source_url: "file:///tmp/team.json".to_string(),
                repo_url: "https://github.com/openai/skills".to_string(),
                version: None,
                sha: None,
                metadata_json: None,
                fetched_at: Some(synced_at.clone()),
            },
        )
        .expect("upsert extra observation");
        tx.commit().expect("commit fixtures");

        let results = load_search_snapshot(&conn, "search", 10)
            .expect("run search snapshot")
            .0;
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].name, "search");
        assert_eq!(results[0].source.as_deref(), Some("openai/skills"));
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

        let resolved = resolve_skill_sources_from_snapshot(
            &conn,
            &requests,
            &named_sources,
            &preferred_repos,
        )
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

        let resolved = resolve_skill_sources_from_snapshot(
            &conn,
            &requests,
            &HashMap::new(),
            &HashSet::new(),
        )
        .expect("resolve ambiguous snapshot");

        assert!(resolved.get("search").is_none());
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
