use super::*;

pub(crate) fn now_rfc3339() -> String {
    Utc::now().to_rfc3339()
}

pub(crate) fn scope_ttl(scope: &str) -> Option<Duration> {
    if scope.starts_with("leaderboard_") {
        Some(Duration::hours(LEADERBOARD_TTL_HOURS))
    } else if scope == "official_publishers"
        || scope.starts_with("publisher_repos:")
        || scope.starts_with("repo_skills:")
    {
        Some(Duration::hours(PUBLISHER_TTL_HOURS))
    } else if scope.starts_with("skill_detail:") {
        Some(Duration::hours(DETAIL_TTL_HOURS))
    } else {
        None
    }
}

pub(crate) fn leaderboard_scope(category: &str) -> String {
    match category {
        "trending" => "leaderboard_trending".to_string(),
        "hot" => "leaderboard_hot".to_string(),
        _ => "leaderboard_all".to_string(),
    }
}

pub(crate) fn skill_detail_scope(source: &str, name: &str) -> Option<String> {
    Some(format!("skill_detail:{}", build_skill_key(source, name)?))
}

pub(crate) fn parse_scope(scope: &str) -> Result<ScopeSpec> {
    if let Some(category) = scope.strip_prefix("leaderboard_") {
        let normalized = match category {
            "hot" => "hot",
            "trending" => "trending",
            "all" | "popular" => "all",
            other => other,
        };
        return Ok(ScopeSpec::Leaderboard {
            category: normalized.to_string(),
        });
    }

    if scope == "official_publishers" {
        return Ok(ScopeSpec::OfficialPublishers);
    }

    if let Some(value) = scope.strip_prefix("publisher_repos:") {
        return Ok(ScopeSpec::PublisherRepos {
            publisher_name: value.trim().to_ascii_lowercase(),
        });
    }

    if let Some(value) = scope.strip_prefix("repo_skills:") {
        let source = normalize_source(value)
            .ok_or_else(|| anyhow!("Invalid repo_skills scope source: {value}"))?;
        return Ok(ScopeSpec::RepoSkills { source });
    }

    if let Some(value) = scope.strip_prefix("skill_detail:") {
        let (source, name) =
            parse_skill_key(value).ok_or_else(|| anyhow!("Invalid skill_detail scope key"))?;
        return Ok(ScopeSpec::SkillDetail { source, name });
    }

    if let Some(query) = scope.strip_prefix("search_seed:") {
        return Ok(ScopeSpec::SearchSeed {
            query: query.trim().to_string(),
        });
    }

    Err(anyhow!("Unsupported marketplace scope: {scope}"))
}

pub(crate) fn next_refresh_at_for_scope(scope: &str, now: DateTime<Utc>) -> Option<String> {
    scope_ttl(scope).map(|ttl| (now + ttl).to_rfc3339())
}

pub(crate) fn mark_scope_attempt_in_tx(tx: &Transaction<'_>, scope: &str) -> Result<()> {
    let now = now_rfc3339();
    tx.execute(
        "INSERT INTO marketplace_sync_state (
            scope,
            last_success_at,
            last_attempt_at,
            last_error,
            next_refresh_at,
            schema_version
        ) VALUES (?1, NULL, ?2, NULL, NULL, ?3)
        ON CONFLICT(scope) DO UPDATE SET
            last_attempt_at = excluded.last_attempt_at,
            last_error = NULL,
            schema_version = excluded.schema_version",
        params![scope, now, SNAPSHOT_SCHEMA_VERSION],
    )
    .with_context(|| format!("Failed to mark marketplace scope attempt: {scope}"))?;
    Ok(())
}

pub(crate) fn mark_scope_success_in_tx(tx: &Transaction<'_>, scope: &str) -> Result<()> {
    let now = Utc::now();
    tx.execute(
        "INSERT INTO marketplace_sync_state (
            scope,
            last_success_at,
            last_attempt_at,
            last_error,
            next_refresh_at,
            schema_version
        ) VALUES (?1, ?2, ?2, NULL, ?3, ?4)
        ON CONFLICT(scope) DO UPDATE SET
            last_success_at = excluded.last_success_at,
            last_attempt_at = excluded.last_attempt_at,
            last_error = NULL,
            next_refresh_at = excluded.next_refresh_at,
            schema_version = excluded.schema_version",
        params![
            scope,
            now.to_rfc3339(),
            next_refresh_at_for_scope(scope, now),
            SNAPSHOT_SCHEMA_VERSION
        ],
    )
    .with_context(|| format!("Failed to mark marketplace scope success: {scope}"))?;
    Ok(())
}

pub(crate) fn mark_scope_error(scope: &str, error: &str) -> Result<()> {
    with_conn(|conn| {
        let now = now_rfc3339();
        conn.execute(
            "INSERT INTO marketplace_sync_state (
                scope,
                last_success_at,
                last_attempt_at,
                last_error,
                next_refresh_at,
                schema_version
            ) VALUES (?1, NULL, ?2, ?3, NULL, ?4)
            ON CONFLICT(scope) DO UPDATE SET
                last_attempt_at = excluded.last_attempt_at,
                last_error = excluded.last_error,
                schema_version = excluded.schema_version",
            params![scope, now, truncate_error(error), SNAPSHOT_SCHEMA_VERSION],
        )
        .with_context(|| format!("Failed to mark marketplace scope error: {scope}"))?;
        Ok(())
    })
}

pub(crate) fn truncate_error(error: &str) -> String {
    error.chars().take(500).collect()
}

pub(crate) fn scope_sync_state(conn: &Connection, scope: &str) -> Result<Option<SyncStateEntry>> {
    conn.query_row(
        "SELECT scope, last_success_at, last_attempt_at, last_error, next_refresh_at, schema_version
         FROM marketplace_sync_state
         WHERE scope = ?1",
        [scope],
        |row| {
            Ok(SyncStateEntry {
                scope: row.get(0)?,
                last_success_at: row.get(1)?,
                last_attempt_at: row.get(2)?,
                last_error: row.get(3)?,
                next_refresh_at: row.get(4)?,
                schema_version: row.get(5)?,
            })
        },
    )
    .optional()
    .context("Failed to load marketplace sync state")
}

pub(crate) fn scope_updated_at(conn: &Connection, scope: &str) -> Result<Option<String>> {
    Ok(scope_sync_state(conn, scope)?.and_then(|entry| entry.last_success_at))
}

pub(crate) fn is_scope_fresh_conn(conn: &Connection, scope: &str) -> Result<bool> {
    let Some(state) = scope_sync_state(conn, scope)? else {
        return Ok(false);
    };

    let Some(next_refresh_at) = state.next_refresh_at else {
        return Ok(false);
    };

    let next_refresh = DateTime::parse_from_rfc3339(&next_refresh_at)
        .map(|value| value.with_timezone(&Utc))
        .ok();
    Ok(next_refresh.is_some_and(|value| value > Utc::now()))
}

pub fn is_scope_stale(scope: &str) -> Result<bool> {
    with_conn(|conn| {
        let Some(state) = scope_sync_state(conn, scope)? else {
            return Ok(false);
        };
        Ok(state.last_success_at.is_some() && !is_scope_fresh_conn(conn, scope)?)
    })
}

pub(crate) fn sync_seed_state(conn: &Connection, scope: &str) -> Result<ScopeSeedState> {
    Ok(if scope_sync_state(conn, scope)?.is_some() {
        ScopeSeedState::Synced
    } else {
        ScopeSeedState::NeverSynced
    })
}

pub fn get_marketplace_sync_states() -> Result<Vec<SyncStateEntry>> {
    with_conn(|conn| {
        let mut stmt = conn
            .prepare(
                "SELECT scope, last_success_at, last_attempt_at, last_error, next_refresh_at, schema_version
                 FROM marketplace_sync_state
                 ORDER BY scope ASC",
            )
            .context("Failed to prepare marketplace sync-state query")?;

        let rows = stmt
            .query_map([], |row| {
                Ok(SyncStateEntry {
                    scope: row.get(0)?,
                    last_success_at: row.get(1)?,
                    last_attempt_at: row.get(2)?,
                    last_error: row.get(3)?,
                    next_refresh_at: row.get(4)?,
                    schema_version: row.get(5)?,
                })
            })
            .context("Failed to read marketplace sync-state rows")?;

        let mut entries = Vec::new();
        for row in rows {
            entries.push(row.context("Failed to decode marketplace sync-state row")?);
        }
        Ok(entries)
    })
}

pub(crate) fn mark_scope_attempt(scope: &str) -> Result<()> {
    with_conn(|conn| {
        let tx = conn
            .unchecked_transaction()
            .context("Failed to start scope-attempt transaction")?;
        mark_scope_attempt_in_tx(&tx, scope)?;
        tx.commit()
            .context("Failed to commit scope-attempt transaction")?;
        Ok(())
    })
}
