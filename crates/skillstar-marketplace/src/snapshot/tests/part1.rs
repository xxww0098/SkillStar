use super::*;
use crate::models::*;
use crate::snapshot::*;

#[test]
fn migrates_legacy_marketplace_cache_into_snapshot_schema() {
    with_temp_data_root(|temp_root| {
        let path = temp_root.join("marketplace.db");
        let conn = open_raw_conn(&path);
        conn.execute_batch(
            "CREATE TABLE marketplace_cache (
                key TEXT PRIMARY KEY,
                description TEXT NOT NULL,
                updated_at TEXT NOT NULL
            );
            INSERT INTO marketplace_cache (key, description, updated_at)
            VALUES ('openai/skills/screenshot', 'Capture screenshots', '2026-01-01T00:00:00Z');",
        )
        .expect("seed legacy marketplace cache");
        drop(conn);

        let conn = create_connection().expect("create migrated marketplace connection");
        let version: i64 = conn
            .pragma_query_value(None, "user_version", |row| row.get(0))
            .expect("read user_version");
        assert_eq!(version, SNAPSHOT_SCHEMA_VERSION);

        let description: String = conn
            .query_row(
                "SELECT description FROM marketplace_skill WHERE skill_key = 'openai/skills/screenshot'",
                [],
                |row| row.get(0),
            )
            .expect("read migrated description");
        assert_eq!(description, "Capture screenshots");
    });
}

#[test]
fn curated_registry_fresh_schema_seeds_default_skills_sh_source() {
    with_temp_data_root(|_| {
        let conn = create_connection().expect("create marketplace connection");
        let version: i64 = conn
            .pragma_query_value(None, "user_version", |row| row.get(0))
            .expect("read user_version");
        assert_eq!(version, SNAPSHOT_SCHEMA_VERSION);

        let entries = list_curated_registries().expect("list curated registries");
        assert_eq!(entries.len(), 1);
        let entry = &entries[0];
        assert_eq!(entry.id, "skills_sh");
        assert_eq!(entry.name, "skills.sh");
        assert_eq!(entry.kind, CuratedRegistryKind::SkillsSh);
        assert_eq!(entry.endpoint, "https://skills.sh");
        assert!(entry.enabled);
        assert_eq!(entry.priority, 0);
        assert_eq!(entry.trust, "official");
    });
}

#[test]
fn multi_source_fresh_schema_creates_observation_table() {
    with_temp_data_root(|_| {
        let conn = create_connection().expect("create marketplace connection");
        let version: i64 = conn
            .pragma_query_value(None, "user_version", |row| row.get(0))
            .expect("read user_version");
        assert_eq!(version, SNAPSHOT_SCHEMA_VERSION);

        let count: i64 = conn
            .query_row(
                "SELECT COUNT(1) FROM marketplace_skill_source_observation",
                [],
                |row| row.get(0),
            )
            .expect("count source observations");
        assert_eq!(count, 0);
    });
}

#[test]
fn multi_source_migrates_v3_database_preserving_curated_registry() {
    with_temp_data_root(|temp_root| {
        let path = temp_root.join("marketplace.db");
        let conn = open_raw_conn(&path);
        conn.execute_batch(
            "CREATE TABLE marketplace_curated_registry (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                kind TEXT NOT NULL,
                endpoint TEXT NOT NULL DEFAULT '',
                enabled INTEGER NOT NULL DEFAULT 1,
                priority INTEGER NOT NULL DEFAULT 100,
                trust TEXT NOT NULL DEFAULT '',
                last_sync_at TEXT,
                last_error TEXT,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL
            );
            INSERT INTO marketplace_curated_registry (
                id, name, kind, endpoint, enabled, priority, trust, created_at, updated_at
            ) VALUES (
                'team_source', 'Team Source', 'custom', 'file:///tmp/team.json', 1, 5, 'team',
                '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z'
            );
            PRAGMA user_version = 3;",
        )
        .expect("seed v3 schema marker");
        drop(conn);

        let conn = create_connection().expect("create migrated marketplace connection");
        let version: i64 = conn
            .pragma_query_value(None, "user_version", |row| row.get(0))
            .expect("read user_version");
        assert_eq!(version, SNAPSHOT_SCHEMA_VERSION);

        let entries = list_curated_registries().expect("list curated registries");
        assert!(entries.iter().any(|entry| entry.id == "team_source"));
        assert!(entries.iter().any(|entry| entry.id == "skills_sh"));
        conn.query_row(
            "SELECT COUNT(1) FROM marketplace_skill_source_observation",
            [],
            |row| row.get::<_, i64>(0),
        )
        .expect("source observation table exists");
    });
}

#[test]
fn ratings_and_reviews_migrate_v4_database_preserving_existing_tables() {
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
            CREATE TABLE marketplace_skill_source_observation (
                source_id TEXT NOT NULL,
                source_skill_id TEXT NOT NULL,
                skill_key TEXT NOT NULL REFERENCES marketplace_skill(skill_key) ON DELETE CASCADE,
                source_url TEXT NOT NULL DEFAULT '',
                repo_url TEXT NOT NULL DEFAULT '',
                version TEXT,
                sha TEXT,
                metadata_json TEXT,
                fetched_at TEXT,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL,
                PRIMARY KEY (source_id, source_skill_id)
            );
            INSERT INTO marketplace_skill (skill_key, source, name, description)
            VALUES ('openai/skills/search', 'openai/skills', 'search', 'desc');
            INSERT INTO marketplace_skill_source_observation (
                source_id, source_skill_id, skill_key, created_at, updated_at
            ) VALUES (
                'skills_sh', 'search', 'openai/skills/search',
                '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z'
            );
            PRAGMA user_version = 4;",
        )
        .expect("seed v4 schema marker");
        drop(conn);

        let conn = create_connection().expect("create migrated marketplace connection");
        let version: i64 = conn
            .pragma_query_value(None, "user_version", |row| row.get(0))
            .expect("read user_version");
        assert_eq!(version, SNAPSHOT_SCHEMA_VERSION);

        let obs_count: i64 = conn
            .query_row(
                "SELECT COUNT(1) FROM marketplace_skill_source_observation",
                [],
                |row| row.get(0),
            )
            .expect("count source observations");
        assert_eq!(obs_count, 1);

        let rating_count: i64 = conn
            .query_row(
                "SELECT COUNT(1) FROM marketplace_rating_summary",
                [],
                |row| row.get(0),
            )
            .expect("count rating summaries");
        assert_eq!(rating_count, 0);

        let review_count: i64 = conn
            .query_row("SELECT COUNT(1) FROM marketplace_review", [], |row| {
                row.get(0)
            })
            .expect("count reviews");
        assert_eq!(review_count, 0);
    });
}

#[test]
fn category_tag_fresh_schema_creates_metadata_tables() {
    with_temp_data_root(|_| {
        let conn = create_connection().expect("create marketplace connection");
        let version: i64 = conn
            .pragma_query_value(None, "user_version", |row| row.get(0))
            .expect("read user_version");
        assert_eq!(version, SNAPSHOT_SCHEMA_VERSION);

        for table in [
            "marketplace_category",
            "marketplace_skill_category",
            "marketplace_tag",
            "marketplace_skill_tag",
        ] {
            conn.query_row(
                "SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = ?1",
                [table],
                |_| Ok(()),
            )
            .unwrap_or_else(|_| panic!("{table} table exists"));
        }
    });
}

#[test]
fn update_notification_fresh_schema_creates_table() {
    with_temp_data_root(|_| {
        let conn = create_connection().expect("create marketplace connection");
        let version: i64 = conn
            .pragma_query_value(None, "user_version", |row| row.get(0))
            .expect("read user_version");
        assert_eq!(version, SNAPSHOT_SCHEMA_VERSION);

        let count: i64 = conn
            .query_row(
                "SELECT COUNT(1) FROM marketplace_update_notification",
                [],
                |row| row.get(0),
            )
            .expect("update notification table exists");
        assert_eq!(count, 0);
    });
}

#[test]
fn update_notifications_migrate_v6_database_preserving_categories_and_tags() {
    with_temp_data_root(|temp_root| {
        let path = temp_root.join("marketplace.db");
        let conn = open_raw_conn(&path);
        conn.execute_batch(
            "CREATE TABLE marketplace_category (
                id TEXT PRIMARY KEY,
                label TEXT NOT NULL,
                slug TEXT NOT NULL UNIQUE,
                parent_id TEXT REFERENCES marketplace_category(id) ON DELETE SET NULL,
                position INTEGER NOT NULL DEFAULT 0,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL
            );
            CREATE TABLE marketplace_tag (
                slug TEXT PRIMARY KEY,
                label TEXT NOT NULL,
                usage_count INTEGER NOT NULL DEFAULT 0,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL
            );
            INSERT INTO marketplace_category (id, label, slug, position, created_at, updated_at)
            VALUES ('ai-agents', 'AI Agents', 'ai-agents', 1, '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z');
            INSERT INTO marketplace_tag (slug, label, usage_count, created_at, updated_at)
            VALUES ('rust-tools', 'Rust Tools', 1, '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z');
            PRAGMA user_version = 6;",
        )
        .expect("seed v6 schema marker");
        drop(conn);

        let conn = create_connection().expect("create migrated marketplace connection");
        let version: i64 = conn
            .pragma_query_value(None, "user_version", |row| row.get(0))
            .expect("read user_version");
        assert_eq!(version, SNAPSHOT_SCHEMA_VERSION);

        let category_count: i64 = conn
            .query_row("SELECT COUNT(1) FROM marketplace_category", [], |row| {
                row.get(0)
            })
            .expect("count categories");
        assert_eq!(category_count, 1);

        let tag_count: i64 = conn
            .query_row("SELECT COUNT(1) FROM marketplace_tag", [], |row| row.get(0))
            .expect("count tags");
        assert_eq!(tag_count, 1);

        conn.query_row(
            "SELECT COUNT(1) FROM marketplace_update_notification",
            [],
            |row| row.get::<_, i64>(0),
        )
        .expect("update notification table exists");
    });
}

#[test]
fn categories_and_tags_migrate_v5_database_preserving_ratings() {
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
            CREATE TABLE marketplace_rating_summary (
                skill_key TEXT NOT NULL REFERENCES marketplace_skill(skill_key) ON DELETE CASCADE,
                source_id TEXT NOT NULL DEFAULT '',
                rating_avg REAL NOT NULL DEFAULT 0,
                rating_count INTEGER NOT NULL DEFAULT 0,
                review_count INTEGER NOT NULL DEFAULT 0,
                last_review_at TEXT,
                updated_at TEXT NOT NULL,
                PRIMARY KEY (skill_key, source_id)
            );
            CREATE TABLE marketplace_review (
                review_id TEXT PRIMARY KEY,
                skill_key TEXT NOT NULL REFERENCES marketplace_skill(skill_key) ON DELETE CASCADE,
                source_id TEXT NOT NULL DEFAULT '',
                author_hash TEXT,
                rating INTEGER NOT NULL,
                title TEXT,
                body TEXT,
                locale TEXT,
                status TEXT NOT NULL DEFAULT 'published',
                reviewed_at TEXT,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL
            );
            INSERT INTO marketplace_skill (skill_key, source, name, description)
            VALUES ('openai/skills/search', 'openai/skills', 'search', 'desc');
            INSERT INTO marketplace_rating_summary (
                skill_key, source_id, rating_avg, rating_count, review_count, updated_at
            ) VALUES ('openai/skills/search', '', 4.5, 2, 1, '2026-01-01T00:00:00Z');
            PRAGMA user_version = 5;",
        )
        .expect("seed v5 schema marker");
        drop(conn);

        let conn = create_connection().expect("create migrated marketplace connection");
        let version: i64 = conn
            .pragma_query_value(None, "user_version", |row| row.get(0))
            .expect("read user_version");
        assert_eq!(version, SNAPSHOT_SCHEMA_VERSION);

        let rating_count: i64 = conn
            .query_row(
                "SELECT COUNT(1) FROM marketplace_rating_summary",
                [],
                |row| row.get(0),
            )
            .expect("count rating summaries");
        assert_eq!(rating_count, 1);

        let category_count: i64 = conn
            .query_row("SELECT COUNT(1) FROM marketplace_category", [], |row| {
                row.get(0)
            })
            .expect("category table exists");
        assert_eq!(category_count, 0);

        let tag_count: i64 = conn
            .query_row("SELECT COUNT(1) FROM marketplace_tag", [], |row| row.get(0))
            .expect("tag table exists");
        assert_eq!(tag_count, 0);
    });
}

