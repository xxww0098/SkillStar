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
    } else if scope.starts_with("search_seed:") {
        Some(Duration::hours(SEARCH_SEED_TTL_HOURS))
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

/// Sync-state key for "we have asked the remote search API about this query".
///
/// Case-folded as well as trimmed so `Rust` and `rust` cannot each keep their
/// own seed record — the search itself is case-insensitive, so two rows would
/// only ever disagree about whether the same question has been asked.
/// Round-trips through [`parse_scope`], which trims but does not re-fold.
pub(crate) fn search_seed_scope(query: &str) -> String {
    format!("search_seed:{}", query.trim().to_ascii_lowercase())
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

/// Recorded in `degraded_reason` for a scope written from a knowingly
/// incomplete payload.
pub(crate) const DEGRADED_SNAPSHOT_REASON: &str = "Written from a lossy fallback payload (upstream parsing failed); \
     kept stale so the next refresh can replace it";

/// Record a successful sync together with the content-addressing metadata of
/// the payload that was written. `None` for any field keeps the previous value
/// (e.g. a 304 no-change refresh preserves the stored hash/etag).
///
/// Snapshot quality is written here, in the same transaction as the rows, and
/// nowhere else:
///
/// * `meta.degraded` → `next_refresh_at` stays NULL (the scope reads stale
///   immediately) and `degraded_reason` says why. `last_error` stays NULL: the
///   sync *did* succeed, and a row with both set reads as a failure to every
///   diagnostics consumer.
/// * a complete payload → TTL applied and `degraded_reason` cleared. That is
///   the only exit from degraded.
/// * `fetched_unchanged` (304 / identical bytes) → the stored rows did not
///   change, so neither does their quality: the previous `degraded_reason` is
///   preserved. Without this a 304 over a degraded scope would hand the
///   fallback rows a full TTL and let them pass as fresh.
///
/// # The `source_host` / `payload_sha256` / `etag` invariant
///
/// All three describe **the same payload** — the one whose rows are currently
/// stored. Nothing else in the store can detect a row where they disagree, so
/// the two write paths keep it by construction:
///
/// * a rewrite takes all three from *this* fetch, `etag` included. A 200 that
///   carries no `ETag` header therefore clears the stored one instead of
///   keeping it: an inherited validator would describe the *previous* payload
///   (possibly served by a different host) while `payload_sha256` and
///   `source_host` describe the new one. With a mirror configured and the two
///   hosts' content diverged, sending that stale token could be answered `304`
///   and pin the divergent body in place — freshness silently lagging behind.
/// * an unchanged fetch keeps the stored `source_host` and `payload_sha256`
///   (no rows changed, so the payload they describe is still the stored one)
///   and takes a *rotated* validator via `COALESCE(new, old)`. That is not a
///   mismatch: a 200 whose body is byte-identical is the same payload, and the
///   validator the server just issued is the current name for it. Keeping the
///   old one there meant every later request sent a token the server no longer
///   recognized — no 304 ever again, full transfer every time. A 304 either
///   echoes the same validator or none, so the COALESCE is a no-op there.
pub(crate) fn mark_scope_success_with_meta_in_tx(
    tx: &Transaction<'_>,
    scope: &str,
    meta: &FetchMeta,
    fetched_unchanged: bool,
) -> Result<()> {
    let now = Utc::now();
    let degraded_reason = meta.degraded.then_some(DEGRADED_SNAPSHOT_REASON);
    let next_refresh_at = if meta.degraded {
        None
    } else {
        next_refresh_at_for_scope(scope, now)
    };
    tx.execute(
        "INSERT INTO marketplace_sync_state (
            scope,
            last_success_at,
            last_attempt_at,
            last_error,
            next_refresh_at,
            schema_version,
            source_host,
            payload_sha256,
            etag,
            degraded_reason
        ) VALUES (?1, ?2, ?2, NULL, ?3, ?4, ?5, ?6, ?7, ?10)
        ON CONFLICT(scope) DO UPDATE SET
            last_success_at = excluded.last_success_at,
            last_attempt_at = excluded.last_attempt_at,
            last_error = NULL,
            next_refresh_at = CASE
                WHEN ?9 THEN NULL
                WHEN ?8 AND marketplace_sync_state.degraded_reason IS NOT NULL THEN NULL
                ELSE excluded.next_refresh_at END,
            schema_version = excluded.schema_version,
            source_host = CASE WHEN ?8 THEN marketplace_sync_state.source_host ELSE excluded.source_host END,
            payload_sha256 = CASE WHEN ?8 THEN marketplace_sync_state.payload_sha256 ELSE excluded.payload_sha256 END,
            etag = CASE
                WHEN ?8 THEN COALESCE(excluded.etag, marketplace_sync_state.etag)
                ELSE excluded.etag END,
            degraded_reason = CASE
                WHEN ?9 THEN excluded.degraded_reason
                WHEN ?8 THEN marketplace_sync_state.degraded_reason
                ELSE NULL END",
        params![
            scope,
            now.to_rfc3339(),
            next_refresh_at,
            SNAPSHOT_SCHEMA_VERSION,
            meta.source_host,
            meta.payload_sha256,
            meta.etag,
            fetched_unchanged,
            meta.degraded,
            degraded_reason,
        ],
    )
    .with_context(|| format!("Failed to mark marketplace scope success: {scope}"))?;
    Ok(())
}

#[cfg_attr(not(test), allow(dead_code))]
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
            schema_version = excluded.schema_version,
            degraded_reason = NULL",
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
        "SELECT scope, last_success_at, last_attempt_at, last_error, next_refresh_at, schema_version,
                source_host, payload_sha256, etag, degraded_reason
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
                source_host: row.get(6)?,
                payload_sha256: row.get(7)?,
                etag: row.get(8)?,
                degraded_reason: row.get(9)?,
            })
        },
    )
    .optional()
    .context("Failed to load marketplace sync state")
}

/// Every scope whose rows land in the shared `marketplace_skill` /
/// `marketplace_listing` tables — the tables search reads.
///
/// Derived from [`leaderboard_scope`] rather than spelled out so the two cannot
/// drift apart.
pub(crate) fn shared_skill_table_scopes() -> [String; 3] {
    [
        leaderboard_scope("all"),
        leaderboard_scope("hot"),
        leaderboard_scope("trending"),
    ]
}

/// Is *any* of the rows now sitting in the shared skill tables known to have
/// come from a lossy payload?
///
/// Asked separately from freshness because the two are not the same question:
/// a scope can be inside its TTL and still hold fallback rows, and the read
/// paths that have no TTL of their own (search) still must not call those rows
/// fresh. See `docs/features/marketplace/README.md`.
///
/// Search has no scope of its own: it reads whatever filled `marketplace_skill`.
/// Asking only `leaderboard_all` was too narrow — `hot` and `trending` write
/// their fallback rows into that same table through `upsert_skill_in_tx`, so a
/// user who only ever opened the hot tab got those rows back from search
/// labelled `Fresh`. One `EXISTS` over the three scopes, never a query per
/// scope: this runs on every search keystroke.
pub(crate) fn shared_skill_table_is_degraded(conn: &Connection) -> Result<bool> {
    let [all, hot, trending] = shared_skill_table_scopes();
    conn.query_row(
        "SELECT EXISTS (
             SELECT 1 FROM marketplace_sync_state
             WHERE scope IN (?1, ?2, ?3) AND degraded_reason IS NOT NULL
         )",
        params![all, hot, trending],
        |row| row.get(0),
    )
    .context("Failed to read shared-table degraded state")
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

/// A scope needs a background refresh when it has been touched before but is
/// not currently fresh. A row whose `last_success_at` is NULL counts as stale:
/// it was attempted and failed, so the startup refresh is exactly what should
/// retry it. Requiring a previous success here is what used to leave a user
/// whose very first sync failed with no recovery path at all.
pub fn is_scope_stale(scope: &str) -> Result<bool> {
    with_conn(|conn| {
        if scope_sync_state(conn, scope)?.is_none() {
            return Ok(false);
        }
        Ok(!is_scope_fresh_conn(conn, scope)?)
    })
}

/// Has this scope ever been seeded successfully?
///
/// Judged by `last_success_at`, never by mere row existence:
/// `mark_scope_attempt` inserts the row *before* the network request, so a
/// row-existence check reports "synced" after the first failure and the
/// local-first readers then return `Miss` forever without retrying.
pub(crate) fn sync_seed_state(conn: &Connection, scope: &str) -> Result<ScopeSeedState> {
    Ok(match scope_sync_state(conn, scope)? {
        Some(state) if state.last_success_at.is_some() => ScopeSeedState::Synced,
        _ => ScopeSeedState::NeverSynced,
    })
}

pub fn get_marketplace_sync_states() -> Result<Vec<SyncStateEntry>> {
    with_conn(|conn| {
        let mut stmt = conn
            .prepare(
                "SELECT scope, last_success_at, last_attempt_at, last_error, next_refresh_at, schema_version,
                        source_host, payload_sha256, etag, degraded_reason
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
                    source_host: row.get(6)?,
                    payload_sha256: row.get(7)?,
                    etag: row.get(8)?,
                    degraded_reason: row.get(9)?,
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
