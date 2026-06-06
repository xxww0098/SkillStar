use super::*;

pub(crate) fn migrate_schema(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS marketplace_skill (
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
        CREATE INDEX IF NOT EXISTS idx_skill_source ON marketplace_skill(source);
        CREATE INDEX IF NOT EXISTS idx_skill_publisher ON marketplace_skill(publisher_name);
        CREATE INDEX IF NOT EXISTS idx_skill_installs ON marketplace_skill(installs DESC);
        CREATE INDEX IF NOT EXISTS idx_skill_name ON marketplace_skill(name);

        CREATE TABLE IF NOT EXISTS marketplace_skill_detail (
            skill_key TEXT PRIMARY KEY REFERENCES marketplace_skill(skill_key) ON DELETE CASCADE,
            summary TEXT,
            readme TEXT,
            weekly_installs TEXT,
            github_stars INTEGER,
            first_seen TEXT,
            security_audits_json TEXT,
            last_detail_sync_at TEXT
        );

        CREATE TABLE IF NOT EXISTS marketplace_publisher (
            publisher_name TEXT PRIMARY KEY,
            repo_count INTEGER NOT NULL DEFAULT 0,
            skill_count INTEGER NOT NULL DEFAULT 0,
            url TEXT NOT NULL DEFAULT '',
            updated_at TEXT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS marketplace_repo (
            source TEXT PRIMARY KEY,
            publisher_name TEXT NOT NULL,
            repo_name TEXT NOT NULL,
            skill_count INTEGER NOT NULL DEFAULT 0,
            installs INTEGER NOT NULL DEFAULT 0,
            installs_label TEXT NOT NULL DEFAULT '',
            url TEXT NOT NULL DEFAULT '',
            updated_at TEXT NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_repo_publisher ON marketplace_repo(publisher_name);

        CREATE TABLE IF NOT EXISTS marketplace_repo_skill (
            source TEXT NOT NULL,
            skill_key TEXT NOT NULL,
            installs INTEGER NOT NULL DEFAULT 0,
            rank INTEGER,
            updated_at TEXT NOT NULL,
            PRIMARY KEY (source, skill_key)
        );

        CREATE TABLE IF NOT EXISTS marketplace_listing (
            listing_type TEXT NOT NULL,
            skill_key TEXT NOT NULL,
            rank INTEGER NOT NULL,
            updated_at TEXT NOT NULL,
            PRIMARY KEY (listing_type, skill_key)
        );
        CREATE INDEX IF NOT EXISTS idx_listing_type_rank ON marketplace_listing(listing_type, rank);

        CREATE TABLE IF NOT EXISTS marketplace_sync_state (
            scope TEXT PRIMARY KEY,
            last_success_at TEXT,
            last_attempt_at TEXT,
            last_error TEXT,
            next_refresh_at TEXT,
            schema_version INTEGER NOT NULL DEFAULT 1
        );

        CREATE VIRTUAL TABLE IF NOT EXISTS marketplace_skill_fts USING fts5(
            skill_key,
            name,
            description,
            summary,
            publisher_name,
            repo_name,
            tokenize='unicode61'
        );",
    )
    .context("Failed to initialize marketplace snapshot schema")?;

    let version: i64 = conn
        .pragma_query_value(None, "user_version", |row| row.get(0))
        .context("Failed to read marketplace user_version")?;

    if version < SNAPSHOT_SCHEMA_VERSION {
        if version < 1 {
            migrate_v0_to_v1(conn)?;
        }
        if version < 2 {
            migrate_v1_to_v2(conn)?;
        }
        if version < 3 {
            migrate_v2_to_v3(conn)?;
        }
        if version < 4 {
            migrate_v3_to_v4(conn)?;
        }
        if version < 5 {
            migrate_v4_to_v5(conn)?;
        }
        if version < 6 {
            migrate_v5_to_v6(conn)?;
        }
        if version < 7 {
            migrate_v6_to_v7(conn)?;
        }
        if version < 8 {
            migrate_v7_to_v8(conn)?;
        }
        if version < 9 {
            migrate_v8_to_v9(conn)?;
        }
        conn.pragma_update(None, "user_version", SNAPSHOT_SCHEMA_VERSION)
            .context("Failed to update marketplace user_version")?;
    }

    let legacy_path = legacy_cache_path();
    if legacy_path.exists() {
        let _ = std::fs::remove_file(legacy_path);
    }

    Ok(())
}

pub(crate) fn migrate_v0_to_v1(conn: &Connection) -> Result<()> {
    if !table_exists(conn, "marketplace_cache")? {
        return Ok(());
    }

    let mut stmt = conn
        .prepare("SELECT key, description, updated_at FROM marketplace_cache")
        .context("Failed to prepare legacy marketplace_cache query")?;

    let rows = stmt
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
            ))
        })
        .context("Failed to read legacy marketplace_cache rows")?;

    let tx = conn
        .unchecked_transaction()
        .context("Failed to start legacy marketplace migration transaction")?;

    for row in rows {
        let (key, description, updated_at) = row.context("Failed to decode legacy cache row")?;
        let Some((source, name)) = parse_skill_key(&key) else {
            continue;
        };

        let (publisher_name, repo_name) = split_source(&source);
        let git_url = format!("https://github.com/{source}");

        tx.execute(
            "INSERT INTO marketplace_skill (
                skill_key,
                source,
                name,
                git_url,
                author,
                publisher_name,
                repo_name,
                description,
                installs,
                last_seen_remote_at,
                last_list_sync_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, 0, ?9, ?9)
            ON CONFLICT(skill_key) DO UPDATE SET
                source = excluded.source,
                name = excluded.name,
                git_url = excluded.git_url,
                author = excluded.author,
                publisher_name = excluded.publisher_name,
                repo_name = excluded.repo_name,
                description = CASE
                    WHEN excluded.description <> '' THEN excluded.description
                    ELSE marketplace_skill.description
                END,
                last_seen_remote_at = COALESCE(excluded.last_seen_remote_at, marketplace_skill.last_seen_remote_at),
                last_list_sync_at = COALESCE(excluded.last_list_sync_at, marketplace_skill.last_list_sync_at)",
            params![
                key,
                source,
                name,
                git_url,
                source,
                publisher_name,
                repo_name,
                description,
                updated_at
            ],
        )
        .context("Failed to migrate legacy marketplace description")?;

        refresh_fts_entry_in_tx(&tx, &key)?;
    }

    tx.execute("DROP TABLE IF EXISTS marketplace_cache", [])
        .context("Failed to drop legacy marketplace_cache table")?;
    tx.commit()
        .context("Failed to commit legacy marketplace migration")?;

    Ok(())
}

pub(crate) fn migrate_v1_to_v2(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS marketplace_pack (
            pack_key TEXT PRIMARY KEY,
            source TEXT NOT NULL,
            name TEXT NOT NULL,
            description TEXT NOT NULL DEFAULT '',
            skill_count INTEGER NOT NULL DEFAULT 0,
            author TEXT,
            git_url TEXT NOT NULL DEFAULT '',
            installs INTEGER NOT NULL DEFAULT 0,
            last_seen_at TEXT
        );

        CREATE TABLE IF NOT EXISTS marketplace_pack_skill (
            pack_key TEXT NOT NULL REFERENCES marketplace_pack(pack_key) ON DELETE CASCADE,
            skill_key TEXT NOT NULL REFERENCES marketplace_skill(skill_key) ON DELETE CASCADE,
            skill_name TEXT NOT NULL,
            PRIMARY KEY (pack_key, skill_key)
        );

        CREATE VIRTUAL TABLE IF NOT EXISTS marketplace_pack_fts USING fts5(
            pack_key,
            name,
            description,
            author,
            content=marketplace_pack,
            content_rowid=rowid
        );",
    )
    .context("Failed to create marketplace pack tables (v2)")?;
    Ok(())
}

pub(crate) fn migrate_v2_to_v3(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS marketplace_curated_registry (
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
        CREATE INDEX IF NOT EXISTS idx_curated_registry_enabled_priority
            ON marketplace_curated_registry(enabled, priority, name);",
    )
    .context("Failed to create curated marketplace registry tables (v3)")?;
    seed_default_curated_registry(conn)?;
    Ok(())
}

pub(crate) fn migrate_v3_to_v4(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS marketplace_skill_source_observation (
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
        CREATE INDEX IF NOT EXISTS idx_skill_source_observation_skill_key
            ON marketplace_skill_source_observation(skill_key);
        CREATE INDEX IF NOT EXISTS idx_skill_source_observation_source_id
            ON marketplace_skill_source_observation(source_id, fetched_at DESC);",
    )
    .context("Failed to create marketplace source observation tables (v4)")?;
    Ok(())
}

pub(crate) fn migrate_v4_to_v5(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS marketplace_rating_summary (
            skill_key TEXT NOT NULL REFERENCES marketplace_skill(skill_key) ON DELETE CASCADE,
            source_id TEXT NOT NULL DEFAULT '',
            rating_avg REAL NOT NULL DEFAULT 0,
            rating_count INTEGER NOT NULL DEFAULT 0,
            review_count INTEGER NOT NULL DEFAULT 0,
            last_review_at TEXT,
            updated_at TEXT NOT NULL,
            PRIMARY KEY (skill_key, source_id)
        );
        CREATE INDEX IF NOT EXISTS idx_rating_summary_skill_key
            ON marketplace_rating_summary(skill_key);
        CREATE INDEX IF NOT EXISTS idx_rating_summary_source_id
            ON marketplace_rating_summary(source_id, updated_at DESC);

        CREATE TABLE IF NOT EXISTS marketplace_review (
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
        CREATE INDEX IF NOT EXISTS idx_marketplace_review_skill_key
            ON marketplace_review(skill_key, source_id, reviewed_at DESC, updated_at DESC);
        CREATE INDEX IF NOT EXISTS idx_marketplace_review_source_id
            ON marketplace_review(source_id, reviewed_at DESC);",
    )
    .context("Failed to create marketplace rating/review tables (v5)")?;
    Ok(())
}

pub(crate) fn migrate_v5_to_v6(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS marketplace_category (
            id TEXT PRIMARY KEY,
            label TEXT NOT NULL,
            slug TEXT NOT NULL UNIQUE,
            parent_id TEXT REFERENCES marketplace_category(id) ON DELETE SET NULL,
            position INTEGER NOT NULL DEFAULT 0,
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_marketplace_category_parent_position
            ON marketplace_category(parent_id, position, label);
        CREATE INDEX IF NOT EXISTS idx_marketplace_category_slug
            ON marketplace_category(slug);

        CREATE TABLE IF NOT EXISTS marketplace_skill_category (
            skill_key TEXT NOT NULL REFERENCES marketplace_skill(skill_key) ON DELETE CASCADE,
            category_id TEXT NOT NULL REFERENCES marketplace_category(id) ON DELETE CASCADE,
            assigned_at TEXT NOT NULL,
            PRIMARY KEY (skill_key, category_id)
        );
        CREATE INDEX IF NOT EXISTS idx_marketplace_skill_category_category
            ON marketplace_skill_category(category_id, skill_key);

        CREATE TABLE IF NOT EXISTS marketplace_tag (
            slug TEXT PRIMARY KEY,
            label TEXT NOT NULL,
            usage_count INTEGER NOT NULL DEFAULT 0,
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_marketplace_tag_usage
            ON marketplace_tag(usage_count DESC, label);

        CREATE TABLE IF NOT EXISTS marketplace_skill_tag (
            skill_key TEXT NOT NULL REFERENCES marketplace_skill(skill_key) ON DELETE CASCADE,
            tag_slug TEXT NOT NULL REFERENCES marketplace_tag(slug) ON DELETE CASCADE,
            source_id TEXT NOT NULL DEFAULT '',
            assigned_at TEXT NOT NULL,
            PRIMARY KEY (skill_key, tag_slug, source_id)
        );
        CREATE INDEX IF NOT EXISTS idx_marketplace_skill_tag_skill
            ON marketplace_skill_tag(skill_key, tag_slug);
        CREATE INDEX IF NOT EXISTS idx_marketplace_skill_tag_tag
            ON marketplace_skill_tag(tag_slug, skill_key);",
    )
    .context("Failed to create marketplace category/tag tables (v6)")?;
    Ok(())
}

pub(crate) fn migrate_v6_to_v7(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS marketplace_update_notification (
            skill_key TEXT NOT NULL REFERENCES marketplace_skill(skill_key) ON DELETE CASCADE,
            source_id TEXT NOT NULL,
            installed_version TEXT,
            available_version TEXT,
            installed_hash TEXT,
            available_hash TEXT,
            detected_at TEXT NOT NULL,
            dismissed_at TEXT,
            message TEXT,
            metadata_json TEXT,
            updated_at TEXT NOT NULL,
            PRIMARY KEY (skill_key, source_id)
        );
        CREATE INDEX IF NOT EXISTS idx_marketplace_update_notification_active
            ON marketplace_update_notification(dismissed_at, detected_at DESC);
        CREATE INDEX IF NOT EXISTS idx_marketplace_update_notification_source
            ON marketplace_update_notification(source_id, updated_at DESC);",
    )
    .context("Failed to create marketplace update notification table (v7)")?;
    Ok(())
}

/// v8: add the MCP registry marketplace snapshot tables (see `mcp_snapshot`).
pub(crate) fn migrate_v7_to_v8(conn: &Connection) -> Result<()> {
    crate::mcp_snapshot::create_mcp_registry_tables(conn)
        .context("Failed to create MCP registry tables (v8)")
}

/// v9: realign `marketplace_skill_fts.rowid` with the owning
/// `marketplace_skill.rowid`.
///
/// FTS rows were originally inserted with auto-assigned rowids and deleted by
/// the tokenized `skill_key` column. FTS5 cannot index `skill_key` for exact
/// lookup, so every `DELETE ... WHERE skill_key = ?` scans the whole FTS index.
/// Inside the leaderboard sync that delete runs once per skill, making the sync
/// O(skills × fts_rows); on a large catalog (~50k rows) it held the snapshot
/// write transaction open for minutes, starving every other writer (e.g. the
/// MCP registry sync failed with "database is locked") and ballooning the WAL.
///
/// Rebuilding the FTS so each row's rowid mirrors its skill lets
/// [`refresh_fts_entry_in_tx`] and [`cleanup_stale_skills_in_tx`] key on rowid
/// (O(log n)). This is a full rebuild of a derived index — safe to repeat.
pub(crate) fn migrate_v8_to_v9(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "DELETE FROM marketplace_skill_fts;
         INSERT INTO marketplace_skill_fts (
             rowid, skill_key, name, description, summary, publisher_name, repo_name
         )
         SELECT
             s.rowid,
             s.skill_key,
             s.name,
             s.description,
             COALESCE(d.summary, ''),
             COALESCE(s.publisher_name, ''),
             COALESCE(s.repo_name, '')
         FROM marketplace_skill s
         LEFT JOIN marketplace_skill_detail d ON d.skill_key = s.skill_key;",
    )
    .context("Failed to realign marketplace FTS rowids (v9)")
}

