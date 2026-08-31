use super::*;

pub async fn sync_marketplace_scope(scope: &str) -> Result<()> {
    match parse_scope(scope)? {
        ScopeSpec::Leaderboard { category } => sync_scope_leaderboard(&category).await,
        ScopeSpec::OfficialPublishers => sync_scope_publishers().await,
        ScopeSpec::PublisherRepos { publisher_name } => {
            sync_scope_publisher_repos(&publisher_name).await
        }
        ScopeSpec::RepoSkills { source } => sync_scope_repo_skills(&source).await,
        ScopeSpec::SkillDetail { source, name } => sync_scope_skill_detail(&source, &name).await,
        ScopeSpec::SearchSeed { query } => seed_search_results(&query, SEARCH_SEED_LIMIT).await,
    }
}

/// What the stored snapshot for a scope looks like before a refresh writes:
/// the payload fingerprint used to detect a no-change refresh, and whether what
/// is stored came from a degraded payload.
#[derive(Debug, Clone, Default)]
pub(crate) struct StoredScopeState {
    pub(crate) payload_sha256: Option<String>,
    pub(crate) etag: Option<String>,
    pub(crate) degraded: bool,
}

/// Read [`StoredScopeState`] for a scope. A missing row answers "nothing
/// stored, not degraded", which forces the safe path (full re-fetch, rewrite).
///
/// A read *failure* answers the same, and that answer is not harmless: it
/// erases the persisted degraded marker, which is exactly the state a mutated
/// `degraded: false` here would produce — the self-lock this column exists to
/// prevent. It cannot be handled any better (there is nothing to fall back on),
/// so it is at least never silent.
pub(crate) fn stored_scope_state(scope: &str) -> StoredScopeState {
    match with_conn(|conn| scope_sync_state(conn, scope)) {
        Ok(Some(state)) => StoredScopeState {
            payload_sha256: state.payload_sha256,
            etag: state.etag,
            degraded: state.degraded_reason.is_some(),
        },
        Ok(None) => StoredScopeState::default(),
        Err(err) => {
            warn!(
                target: "marketplace_snapshot",
                scope,
                error = %err,
                "sync-state read failed; treating the scope as never written — a stored degraded marker is lost for this refresh"
            );
            StoredScopeState::default()
        }
    }
}

/// Commit a no-change refresh: update the scope timestamps but keep the stored
/// data and fingerprint. `meta` must carry the fingerprint from the fetch
/// (empty sha256 on a 304); the SQL preserves the previous values.
fn commit_unchanged(scope: &str, meta: &FetchMeta) -> Result<()> {
    with_conn(|conn| {
        let tx = conn
            .unchecked_transaction()
            .context("Failed to start no-change snapshot transaction")?;
        mark_scope_attempt_in_tx(&tx, scope)?;
        mark_scope_success_with_meta_in_tx(&tx, scope, meta, true)?;
        tx.commit()
            .context("Failed to commit no-change snapshot transaction")?;
        Ok(())
    })
}

/// True when the fetched payload is either a 304 (empty sha256) or byte-for-byte
/// identical to the last successful write.
pub(crate) fn payload_unchanged(meta: &FetchMeta, previous_sha256: &Option<String>) -> bool {
    meta.payload_sha256.is_empty()
        || previous_sha256.as_deref() == Some(meta.payload_sha256.as_str())
}

/// How many locally stored rows currently back a scope.
///
/// Content addressing skips the rewrite when upstream returns identical bytes;
/// that shortcut is only sound while the rows those bytes produced are still
/// there. If the local data was lost (wiped table, failed write, a parse that
/// yielded nothing), unchanged bytes must still trigger a full rewrite —
/// otherwise the store is pinned empty while reporting a successful sync.
pub(crate) fn scope_local_row_count(conn: &Connection, scope: &str) -> Result<i64> {
    let count: i64 = match parse_scope(scope)? {
        ScopeSpec::Leaderboard { .. } => conn.query_row(
            "SELECT COUNT(*) FROM marketplace_listing WHERE listing_type = ?1",
            [scope],
            |row| row.get(0),
        ),
        ScopeSpec::OfficialPublishers => {
            conn.query_row("SELECT COUNT(*) FROM marketplace_publisher", [], |row| {
                row.get(0)
            })
        }
        ScopeSpec::PublisherRepos { publisher_name } => conn.query_row(
            "SELECT COUNT(*) FROM marketplace_repo WHERE publisher_name = ?1",
            [publisher_name.as_str()],
            |row| row.get(0),
        ),
        ScopeSpec::RepoSkills { source } => conn.query_row(
            "SELECT COUNT(*) FROM marketplace_repo_skill WHERE source = ?1",
            [source.as_str()],
            |row| row.get(0),
        ),
        ScopeSpec::SkillDetail { source, name } => {
            let Some(skill_key) = build_skill_key(&source, &name) else {
                return Ok(0);
            };
            conn.query_row(
                "SELECT COUNT(*) FROM marketplace_skill_detail WHERE skill_key = ?1",
                [skill_key.as_str()],
                |row| row.get(0),
            )
        }
        // Search seeds are not content addressed.
        ScopeSpec::SearchSeed { .. } => return Ok(0),
    }
    .with_context(|| format!("Failed to count local rows for scope {scope}"))?;
    Ok(count)
}

/// Whether the scope still holds data locally. A read failure answers `false`,
/// which forces the safe path (full re-fetch and rewrite).
pub(crate) fn scope_has_local_rows(scope: &str) -> bool {
    match with_conn(|conn| scope_local_row_count(conn, scope)) {
        Ok(count) => count > 0,
        Err(err) => {
            warn!(
                target: "marketplace_snapshot",
                scope,
                error = %err,
                "local row count failed; treating the scope as empty and forcing a full rewrite"
            );
            false
        }
    }
}

/// Conditional-request token for a refresh, withheld in the two cases where a
/// 304 would be useless or harmful:
///
/// * no local rows left — there is nothing a "nothing changed" answer could
///   preserve, so the server must send a full body;
/// * the stored snapshot is degraded — a 304 carries no payload to re-parse,
///   and re-parsing is the whole recovery path out of degraded (the bytes that
///   defeated the old parser are typically still the same bytes). Asking for
///   them unconditionally is what lets [`plan_refresh`] see a real payload and
///   rebuild the scope.
pub(crate) fn conditional_etag(
    previous_etag: &Option<String>,
    has_local_rows: bool,
    stored_is_degraded: bool,
) -> Option<&str> {
    if has_local_rows && !stored_is_degraded {
        previous_etag.as_deref()
    } else {
        None
    }
}

/// What a completed fetch is allowed to do to the stored snapshot.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RefreshPlan {
    /// Upstream could only be parsed through a lossy fallback while the stored
    /// snapshot is a complete one. Keep what is stored and fail the refresh.
    RejectDegraded,
    /// Identical bytes *and* the rows they produced are still there.
    SkipRewrite,
    /// Write the payload.
    Rewrite,
}

/// The whole post-fetch decision, as one pure function.
///
/// It lives here rather than inline in the five `sync_scope_*` bodies because
/// both invariants it encodes are otherwise untestable without HTTP, and both
/// were silently broken before:
///
/// * `has_local_rows &&` in front of [`payload_unchanged`] — unchanged bytes
///   must not excuse a scope that lost its rows, or the store stays pinned
///   empty while reporting a successful sync.
/// * degraded entry/exit — a degraded payload may not overwrite a *complete*
///   snapshot, but it must be allowed to replace an equally degraded one.
///   Refusing both ways is what made the first fallback seed self-locking:
///   rows existed, so every later refresh failed, forever, with no way to
///   either accept the fallback data or stop the error.
/// * degraded exit beats content addressing — see the third guard below.
pub(crate) fn plan_refresh(
    meta: &FetchMeta,
    previous_sha256: &Option<String>,
    has_local_rows: bool,
    stored_is_degraded: bool,
) -> RefreshPlan {
    if meta.degraded && has_local_rows && !stored_is_degraded {
        return RefreshPlan::RejectDegraded;
    }
    // A complete payload replaces degraded rows even byte-for-byte identical
    // ones. The fingerprint stored while degraded is the fingerprint of a
    // payload the parser of the day could *not* read; the same bytes read by a
    // fixed parser are a different snapshot entirely, and that upgrade is the
    // main way a scope leaves degraded. Content addressing would skip exactly
    // that rewrite and pin the fallback rows in place forever.
    //
    // Restricted to a real body: a 304 has nothing to re-parse, so it stays a
    // no-change refresh (which preserves the degraded marker rather than
    // laundering it). `conditional_etag` withholds the ETag for a degraded
    // scope precisely so this stays a corner case.
    if stored_is_degraded && !meta.degraded && !meta.payload_sha256.is_empty() {
        return RefreshPlan::Rewrite;
    }
    if has_local_rows && payload_unchanged(meta, previous_sha256) {
        return RefreshPlan::SkipRewrite;
    }
    RefreshPlan::Rewrite
}

/// [`plan_refresh`] against what is actually stored for `scope`.
///
/// Every `sync_scope_*` decides through this function, and it is the only place
/// the persisted `degraded_reason` is turned back into a decision. Keeping the
/// read here rather than in five call sites is what makes that wiring testable
/// without HTTP: a test that assembles the arguments itself cannot tell
/// `degraded_reason.is_some()` from a hardcoded `false`.
///
/// The state is re-read after the fetch rather than reused from the pre-fetch
/// read: a fetch takes seconds, and the decision must be made against the rows
/// that are there now.
pub(crate) fn plan_scope_refresh(scope: &str, meta: &FetchMeta) -> RefreshPlan {
    let stored = stored_scope_state(scope);
    plan_refresh(
        meta,
        &stored.payload_sha256,
        scope_has_local_rows(scope),
        stored.degraded,
    )
}

/// Record and describe a [`RefreshPlan::RejectDegraded`] outcome. The stored
/// snapshot is untouched, so this is the one place that says so.
///
/// This writes `last_error` on a scope that usually also has a
/// `last_success_at` — as does every other refresh failure after a first
/// success. The two columns answer different questions and both can be set at
/// once: `last_success_at` is "when data last landed", `last_error` is "why the
/// last *refresh* failed". Only `degraded_reason` answers "is the data that
/// landed trustworthy". A diagnostics consumer that reads `last_error.is_some()`
/// as "this scope has no data" is wrong; see `docs/errors.md`.
fn degraded_rejected(scope: &str, fallback_rows: Option<usize>) -> anyhow::Error {
    let rows = match fallback_rows {
        Some(count) => format!("only {count} fallback rows"),
        None => "a fallback payload".to_string(),
    };
    let err = anyhow!(
        "{scope} came back degraded (upstream parsing failed, {rows}); \
         keeping the existing complete snapshot"
    );
    let _ = mark_scope_error(scope, &format!("{err:#}"));
    err
}

pub async fn sync_scope_leaderboard(category: &str) -> Result<()> {
    let scope = leaderboard_scope(category);
    mark_scope_attempt(&scope)?;
    let synced_at = now_rfc3339();
    let stored = stored_scope_state(&scope);
    let has_local_rows = scope_has_local_rows(&scope);

    let (skills, meta) = match remote::get_skills_sh_leaderboard_with_meta(
        category,
        conditional_etag(&stored.etag, has_local_rows, stored.degraded),
    )
    .await
    {
        Ok(fetched) => fetched,
        Err(err) => {
            let err = err.context(format!("Failed to fetch remote leaderboard: {category}"));
            let _ = mark_scope_error(&scope, &format!("{err:#}"));
            return Err(err);
        }
    };

    match plan_scope_refresh(&scope, &meta) {
        RefreshPlan::RejectDegraded => return Err(degraded_rejected(&scope, Some(skills.len()))),
        RefreshPlan::SkipRewrite => {
            debug!(target: "skills_sh", scope = %scope, hash = %meta.payload_sha256, "leaderboard unchanged; skipped rewrite");
            return commit_unchanged(&scope, &meta);
        }
        RefreshPlan::Rewrite => {}
    }

    let write_result: Result<()> = with_conn(|conn| {
        let tx = conn
            .unchecked_transaction()
            .context("Failed to start leaderboard snapshot transaction")?;

        mark_scope_attempt_in_tx(&tx, &scope)?;
        delete_listing_scope_in_tx(&tx, &scope)?;

        for (index, skill) in skills.iter().enumerate() {
            if let Some(skill_key) = upsert_skill_in_tx(&tx, skill, &synced_at)? {
                tx.execute(
                    "INSERT INTO marketplace_listing (listing_type, skill_key, rank, updated_at)
                     VALUES (?1, ?2, ?3, ?4)",
                    params![scope, skill_key, (index + 1) as i64, synced_at],
                )
                .context("Failed to insert marketplace listing row")?;
            }
        }

        cleanup_stale_skills_in_tx(&tx)?;
        mark_scope_success_with_meta_in_tx(&tx, &scope, &meta, false)?;
        tx.commit()
            .context("Failed to commit leaderboard snapshot transaction")?;
        Ok(())
    });

    if let Err(err) = write_result {
        let _ = mark_scope_error(&scope, &format!("{err:#}"));
        return Err(err);
    }

    // Degraded bookkeeping is not done here: `mark_scope_success_with_meta_in_tx`
    // writes `degraded_reason` and withholds the TTL inside the same transaction
    // as the rows, so a crash can never leave fallback rows behind a full TTL.
    Ok(())
}

pub async fn sync_scope_publishers() -> Result<()> {
    let scope = "official_publishers";
    mark_scope_attempt(scope)?;
    let synced_at = now_rfc3339();
    let stored = stored_scope_state(scope);
    let has_local_rows = scope_has_local_rows(scope);

    let (publishers, meta) = match remote::get_official_publishers_with_meta(conditional_etag(
        &stored.etag,
        has_local_rows,
        stored.degraded,
    ))
    .await
    {
        Ok(fetched) => fetched,
        Err(err) => {
            let err = err.context("Failed to fetch remote official publishers");
            let _ = mark_scope_error(scope, &format!("{err:#}"));
            return Err(err);
        }
    };

    match plan_scope_refresh(scope, &meta) {
        RefreshPlan::RejectDegraded => {
            return Err(degraded_rejected(scope, Some(publishers.len())));
        }
        RefreshPlan::SkipRewrite => {
            debug!(target: "skills_sh", scope, hash = %meta.payload_sha256, "official publishers unchanged; skipped rewrite");
            return commit_unchanged(scope, &meta);
        }
        RefreshPlan::Rewrite => {}
    }

    let write_result: Result<()> = with_conn(|conn| {
        let tx = conn
            .unchecked_transaction()
            .context("Failed to start publisher snapshot transaction")?;

        mark_scope_attempt_in_tx(&tx, scope)?;
        tx.execute("DELETE FROM marketplace_publisher", [])
            .context("Failed to clear publisher snapshot table")?;

        for publisher in &publishers {
            tx.execute(
                "INSERT INTO marketplace_publisher (
                    publisher_name,
                    repo_count,
                    skill_count,
                    url,
                    updated_at
                ) VALUES (?1, ?2, ?3, ?4, ?5)",
                params![
                    publisher.name.to_ascii_lowercase(),
                    publisher.repo_count as i64,
                    publisher.skill_count as i64,
                    publisher.url,
                    synced_at
                ],
            )
            .context("Failed to upsert publisher snapshot row")?;
        }

        mark_scope_success_with_meta_in_tx(&tx, scope, &meta, false)?;
        tx.commit()
            .context("Failed to commit publisher snapshot transaction")?;
        Ok(())
    });

    if let Err(err) = write_result {
        let _ = mark_scope_error(scope, &format!("{err:#}"));
        return Err(err);
    }

    Ok(())
}

pub async fn sync_scope_publisher_repos(publisher_name: &str) -> Result<()> {
    let publisher_name = publisher_name.trim().to_ascii_lowercase();
    let scope = format!("publisher_repos:{publisher_name}");
    mark_scope_attempt(&scope)?;
    let synced_at = now_rfc3339();
    let stored = stored_scope_state(&scope);
    let has_local_rows = scope_has_local_rows(&scope);

    let (repos, meta) = match remote::get_publisher_repos_with_meta(
        &publisher_name,
        conditional_etag(&stored.etag, has_local_rows, stored.degraded),
    )
    .await
    {
        Ok(fetched) => fetched,
        Err(err) => {
            let err = err.context(format!(
                "Failed to fetch repos for publisher {publisher_name}"
            ));
            let _ = mark_scope_error(&scope, &format!("{err:#}"));
            return Err(err);
        }
    };

    match plan_scope_refresh(&scope, &meta) {
        RefreshPlan::RejectDegraded => return Err(degraded_rejected(&scope, Some(repos.len()))),
        RefreshPlan::SkipRewrite => {
            debug!(target: "skills_sh", scope = %scope, hash = %meta.payload_sha256, "publisher repos unchanged; skipped rewrite");
            return commit_unchanged(&scope, &meta);
        }
        RefreshPlan::Rewrite => {}
    }

    let write_result: Result<()> = with_conn(|conn| {
        let tx = conn
            .unchecked_transaction()
            .context("Failed to start publisher-repo snapshot transaction")?;

        mark_scope_attempt_in_tx(&tx, &scope)?;
        tx.execute(
            "DELETE FROM marketplace_repo WHERE publisher_name = ?1",
            [publisher_name.as_str()],
        )
        .context("Failed to clear publisher repo snapshot rows")?;

        for repo in &repos {
            tx.execute(
                "INSERT INTO marketplace_repo (
                    source,
                    publisher_name,
                    repo_name,
                    skill_count,
                    installs,
                    installs_label,
                    url,
                    updated_at
                ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
                ON CONFLICT(source) DO UPDATE SET
                    publisher_name = excluded.publisher_name,
                    repo_name = excluded.repo_name,
                    skill_count = excluded.skill_count,
                    installs = excluded.installs,
                    installs_label = excluded.installs_label,
                    url = excluded.url,
                    updated_at = excluded.updated_at",
                params![
                    repo.source.to_ascii_lowercase(),
                    publisher_name,
                    repo.repo.to_ascii_lowercase(),
                    repo.skill_count as i64,
                    repo.installs as i64,
                    repo.installs_label,
                    repo.url,
                    synced_at
                ],
            )
            .context("Failed to upsert publisher repo snapshot row")?;

            seed_repo_skills_from_official_in_tx(&tx, repo, &synced_at)?;
        }

        mark_scope_success_with_meta_in_tx(&tx, &scope, &meta, false)?;
        tx.commit()
            .context("Failed to commit publisher-repo snapshot transaction")?;
        Ok(())
    });

    if let Err(err) = write_result {
        let _ = mark_scope_error(&scope, &format!("{err:#}"));
        return Err(err);
    }

    Ok(())
}

/// Seed `marketplace_repo_skill` from the skills embedded in the `/official`
/// aggregate — only for a repo whose own `repo_skills:<source>` scope has never
/// succeeded. Once the repo page has been scraped it is the authority for that
/// repo: the aggregate lags behind it (it keeps skills the repo no longer
/// ships), so letting every aggregate rewrite clobber the scraped rows made a
/// repo flip between "11 skills" and "3 skills" from one refresh to the next.
pub(crate) fn seed_repo_skills_from_official_in_tx(
    tx: &Transaction<'_>,
    repo: &PublisherRepo,
    synced_at: &str,
) -> Result<()> {
    let source = repo.source.to_ascii_lowercase();
    if repo.skills.is_empty()
        || matches!(
            sync_seed_state(tx, &format!("repo_skills:{source}"))?,
            ScopeSeedState::Synced
        )
    {
        return Ok(());
    }

    tx.execute(
        "DELETE FROM marketplace_repo_skill WHERE source = ?1",
        [source.as_str()],
    )
    .context("Failed to clear embedded repo-skill snapshot rows")?;

    for (index, skill) in repo.skills.iter().enumerate() {
        if let Some(skill_key) =
            upsert_skill_identity_in_tx(tx, &repo.source, &skill.name, skill.installs, synced_at)?
        {
            tx.execute(
                "INSERT INTO marketplace_repo_skill (
                    source,
                    skill_key,
                    installs,
                    rank,
                    updated_at
                ) VALUES (?1, ?2, ?3, ?4, ?5)
                ON CONFLICT(source, skill_key) DO UPDATE SET
                    installs = excluded.installs,
                    rank = excluded.rank,
                    updated_at = excluded.updated_at",
                params![
                    source,
                    skill_key,
                    skill.installs as i64,
                    (index + 1) as i64,
                    synced_at
                ],
            )
            .context("Failed to upsert embedded repo-skill snapshot row")?;
        }
    }
    Ok(())
}

pub async fn sync_scope_repo_skills(source: &str) -> Result<()> {
    let source = normalize_source(source).ok_or_else(|| anyhow!("Invalid repo source"))?;
    let scope = format!("repo_skills:{source}");
    mark_scope_attempt(&scope)?;
    let synced_at = now_rfc3339();
    let stored = stored_scope_state(&scope);
    let (publisher_name, repo_name) = split_source(&source);
    let has_local_rows = scope_has_local_rows(&scope);

    let (skills, meta) = match remote::get_publisher_repo_skills_with_meta(
        &publisher_name,
        &repo_name,
        conditional_etag(&stored.etag, has_local_rows, stored.degraded),
    )
    .await
    {
        Ok(fetched) => fetched,
        Err(err) => {
            let err = err.context(format!("Failed to fetch repo skills for {source}"));
            let _ = mark_scope_error(&scope, &format!("{err:#}"));
            return Err(err);
        }
    };

    match plan_scope_refresh(&scope, &meta) {
        RefreshPlan::RejectDegraded => return Err(degraded_rejected(&scope, Some(skills.len()))),
        RefreshPlan::SkipRewrite => {
            debug!(target: "skills_sh", scope = %scope, hash = %meta.payload_sha256, "repo skills unchanged; skipped rewrite");
            return commit_unchanged(&scope, &meta);
        }
        RefreshPlan::Rewrite => {}
    }

    let write_result: Result<()> = with_conn(|conn| {
        let tx = conn
            .unchecked_transaction()
            .context("Failed to start repo-skill snapshot transaction")?;

        mark_scope_attempt_in_tx(&tx, &scope)?;
        tx.execute(
            "DELETE FROM marketplace_repo_skill WHERE source = ?1",
            [source.as_str()],
        )
        .context("Failed to clear repo-skill snapshot rows")?;

        for (index, skill) in skills.iter().enumerate() {
            if let Some(skill_key) =
                upsert_skill_identity_in_tx(&tx, &source, &skill.name, skill.installs, &synced_at)?
            {
                tx.execute(
                    "INSERT INTO marketplace_repo_skill (
                        source,
                        skill_key,
                        installs,
                        rank,
                        updated_at
                    ) VALUES (?1, ?2, ?3, ?4, ?5)",
                    params![
                        source,
                        skill_key,
                        skill.installs as i64,
                        (index + 1) as i64,
                        synced_at
                    ],
                )
                .context("Failed to insert repo-skill snapshot row")?;
            }
        }

        mark_scope_success_with_meta_in_tx(&tx, &scope, &meta, false)?;
        tx.commit()
            .context("Failed to commit repo-skill snapshot transaction")?;
        Ok(())
    });

    if let Err(err) = write_result {
        let _ = mark_scope_error(&scope, &format!("{err:#}"));
        return Err(err);
    }

    Ok(())
}

pub async fn sync_scope_skill_detail(source: &str, name: &str) -> Result<()> {
    let source = normalize_source(source).ok_or_else(|| anyhow!("Invalid skill source"))?;
    let name = normalize_skill_name(name).ok_or_else(|| anyhow!("Invalid skill name"))?;
    let scope =
        skill_detail_scope(&source, &name).ok_or_else(|| anyhow!("Invalid detail scope"))?;
    mark_scope_attempt(&scope)?;
    let synced_at = now_rfc3339();
    let stored = stored_scope_state(&scope);
    let has_local_rows = scope_has_local_rows(&scope);

    let (details, meta) = match remote::fetch_marketplace_skill_details_with_meta(
        &source,
        &name,
        conditional_etag(&stored.etag, has_local_rows, stored.degraded),
    )
    .await
    {
        Ok(fetched) => fetched,
        Err(err) => {
            let err = err.context(format!(
                "Failed to fetch marketplace detail for {source}/{name}"
            ));
            let _ = mark_scope_error(&scope, &format!("{err:#}"));
            return Err(err);
        }
    };

    match plan_scope_refresh(&scope, &meta) {
        RefreshPlan::RejectDegraded => return Err(degraded_rejected(&scope, None)),
        RefreshPlan::SkipRewrite => {
            debug!(target: "skills_sh", scope = %scope, hash = %meta.payload_sha256, "skill detail unchanged; skipped rewrite");
            return commit_unchanged(&scope, &meta);
        }
        RefreshPlan::Rewrite => {}
    }

    let write_result: Result<()> = with_conn(|conn| {
        let tx = conn
            .unchecked_transaction()
            .context("Failed to start skill-detail snapshot transaction")?;

        mark_scope_attempt_in_tx(&tx, &scope)?;
        let _ = upsert_skill_identity_in_tx(&tx, &source, &name, 0, &synced_at)?;
        upsert_detail_in_tx(&tx, &source, &name, &details, &synced_at)?;
        mark_scope_success_with_meta_in_tx(&tx, &scope, &meta, false)?;
        tx.commit()
            .context("Failed to commit skill-detail snapshot transaction")?;
        Ok(())
    });

    if let Err(err) = write_result {
        let _ = mark_scope_error(&scope, &format!("{err:#}"));
        return Err(err);
    }

    Ok(())
}

/// Seed the shared skill tables from the remote search API for one query, and
/// record that we asked, under `search_seed:<query>`.
///
/// The record is the point, not a side effect: it is the only thing that can
/// answer "have we ever asked the remote about *this* keyword", which is what
/// [`ai_search_local`](super::local_first::ai_search_local) needs to decide
/// whether a thin result means "the remote has little for this keyword" or
/// "we never looked". Row counts cannot answer that question — the whole
/// snapshot is populated by leaderboard syncs that know nothing about any
/// particular keyword.
///
/// Written with `mark_scope_success_with_meta_in_tx` in the same transaction as
/// the rows, like every other scope: a degraded payload therefore leaves the
/// scope without a TTL, so the next AI search re-asks instead of treating the
/// lossy answer as settled.
pub(crate) async fn seed_search_results(query: &str, limit: u32) -> Result<()> {
    let trimmed = query.trim();
    if trimmed.is_empty() {
        return Ok(());
    }
    let scope = search_seed_scope(trimmed);

    let synced_at = now_rfc3339();
    let fetched = remote::search_skills_sh_with_meta(trimmed, limit, None)
        .await
        .with_context(|| {
            format!("Failed to seed marketplace search results for query '{trimmed}'")
        });
    let (result, meta) = match fetched {
        Ok(fetched) => fetched,
        Err(err) => {
            let _ = mark_scope_error(&scope, &format!("{err:#}"));
            return Err(err);
        }
    };

    let write_result = with_conn(|conn| {
        let tx = conn
            .unchecked_transaction()
            .context("Failed to start marketplace search seed transaction")?;

        mark_scope_attempt_in_tx(&tx, &scope)?;
        for skill in &result.skills {
            let _ = upsert_skill_in_tx(&tx, skill, &synced_at)?;
        }

        cleanup_stale_skills_in_tx(&tx)?;
        mark_scope_success_with_meta_in_tx(&tx, &scope, &meta, false)?;
        tx.commit()
            .context("Failed to commit marketplace search seed transaction")?;
        Ok(())
    });

    if let Err(err) = write_result {
        let _ = mark_scope_error(&scope, &format!("{err:#}"));
        return Err(err);
    }

    Ok(())
}
