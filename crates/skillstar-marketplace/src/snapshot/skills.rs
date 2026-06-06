use super::*;

pub(crate) fn build_fts_query(query: &str) -> Option<String> {
    let tokens = tokenize_query(query);
    if tokens.is_empty() {
        None
    } else {
        Some(
            tokens
                .into_iter()
                .map(|token| format!("{token}*"))
                .collect::<Vec<_>>()
                .join(" OR "),
        )
    }
}

pub(crate) fn tokenize_query(query: &str) -> Vec<String> {
    let mut cleaned = String::with_capacity(query.len());
    for ch in query.chars() {
        if ch.is_alphanumeric() {
            cleaned.push(ch.to_ascii_lowercase());
        } else {
            cleaned.push(' ');
        }
    }

    cleaned
        .split_whitespace()
        .filter(|token| !token.is_empty())
        .map(|token| token.trim_matches('/').to_string())
        .filter(|token| !token.is_empty())
        .collect()
}

/// Rebuild the FTS row for a single skill after its `marketplace_skill` row was
/// upserted.
///
/// The FTS row's `rowid` mirrors the owning `marketplace_skill.rowid`, so both
/// the delete and the re-insert key on `rowid` (an O(log n) lookup) rather than
/// the tokenized `skill_key` column, which FTS5 cannot index for exact match and
/// would force a full FTS-index scan per call — see [`migrate_v8_to_v9`].
pub(crate) fn refresh_fts_entry_in_tx(tx: &Transaction<'_>, skill_key: &str) -> Result<()> {
    let row = tx
        .query_row(
            "SELECT
                s.rowid,
                s.name,
                s.description,
                COALESCE(d.summary, ''),
                COALESCE(s.publisher_name, ''),
                COALESCE(s.repo_name, '')
             FROM marketplace_skill s
             LEFT JOIN marketplace_skill_detail d ON d.skill_key = s.skill_key
             WHERE s.skill_key = ?1",
            [skill_key],
            |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, String>(5)?,
                ))
            },
        )
        .optional()
        .context("Failed to load marketplace skill for FTS refresh")?;

    // Callers always upsert the skill first, so the row normally exists. If it
    // doesn't there is no rowid to key on and nothing to index; stale FTS rows
    // for removed skills are cleared by `cleanup_stale_skills_in_tx`.
    let Some((rowid, name, description, summary, publisher_name, repo_name)) = row else {
        return Ok(());
    };

    tx.execute(
        "DELETE FROM marketplace_skill_fts WHERE rowid = ?1",
        [rowid],
    )
    .context("Failed to delete marketplace FTS entry")?;

    tx.execute(
        "INSERT INTO marketplace_skill_fts (
            rowid,
            skill_key,
            name,
            description,
            summary,
            publisher_name,
            repo_name
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        params![
            rowid,
            skill_key,
            name,
            description,
            summary,
            publisher_name,
            repo_name
        ],
    )
    .context("Failed to insert marketplace FTS entry")?;

    Ok(())
}

// ── Pack seeding ──────────────────────────────────────────────────

/// Public type returned by pack search.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MarketplacePack {
    pub pack_key: String,
    pub source: String,
    pub name: String,
    pub description: String,
    pub skill_count: i64,
    pub author: Option<String>,
    pub git_url: String,
    pub installs: i64,
}

/// Insert or update a pack entry and link its skills.
/// Called after syncing a repo that contains a skillpack.toml.
pub fn upsert_pack(
    pack_key: &str,
    source: &str,
    name: &str,
    description: &str,
    author: Option<&str>,
    git_url: &str,
    skill_keys: &[(String, String)], // (skill_key, skill_name)
) -> Result<()> {
    with_conn(|conn| {
        let tx = conn
            .unchecked_transaction()
            .context("Failed to start pack upsert transaction")?;

        tx.execute(
            "INSERT INTO marketplace_pack (
                pack_key, source, name, description, skill_count, author, git_url, installs, last_seen_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 0, ?8)
            ON CONFLICT(pack_key) DO UPDATE SET
                source = excluded.source,
                name = excluded.name,
                description = CASE WHEN excluded.description <> '' THEN excluded.description ELSE marketplace_pack.description END,
                skill_count = excluded.skill_count,
                author = COALESCE(excluded.author, marketplace_pack.author),
                git_url = excluded.git_url,
                last_seen_at = excluded.last_seen_at",
            params![
                pack_key,
                source,
                name,
                description,
                skill_keys.len() as i64,
                author,
                git_url,
                now_rfc3339(),
            ],
        )
        .context("Failed to upsert marketplace pack")?;

        // Clear old skill links and re-insert
        tx.execute(
            "DELETE FROM marketplace_pack_skill WHERE pack_key = ?1",
            [pack_key],
        )
        .context("Failed to clear pack skill links")?;

        for (skill_key, skill_name) in skill_keys {
            tx.execute(
                "INSERT OR IGNORE INTO marketplace_pack_skill (pack_key, skill_key, skill_name)
                 VALUES (?1, ?2, ?3)",
                params![pack_key, skill_key, skill_name],
            )
            .context("Failed to insert pack skill link")?;
        }

        // Refresh pack FTS
        tx.execute(
            "DELETE FROM marketplace_pack_fts WHERE pack_key = ?1",
            [pack_key],
        )
        .context("Failed to delete pack FTS entry")?;

        tx.execute(
            "INSERT INTO marketplace_pack_fts (pack_key, name, description, author)
             VALUES (?1, ?2, ?3, ?4)",
            params![pack_key, name, description, author.unwrap_or("")],
        )
        .context("Failed to insert pack FTS entry")?;

        tx.commit().context("Failed to commit pack upsert")?;
        Ok(())
    })
}

/// Search for packs matching a query string.
pub fn search_packs_local(query: &str, limit: u32) -> Result<Vec<MarketplacePack>> {
    let limit = limit.clamp(1, 50) as i64;
    let normalized = query.trim().to_ascii_lowercase();

    if normalized.is_empty() {
        return list_packs_local(limit as u32);
    }

    let Some(fts_query) = build_fts_query(&normalized) else {
        return Ok(Vec::new());
    };

    with_conn(|conn| {
        let mut stmt = conn.prepare(
            "SELECT p.pack_key, p.source, p.name, p.description, p.skill_count,
                    p.author, p.git_url, p.installs
             FROM marketplace_pack_fts fts
             JOIN marketplace_pack p ON p.pack_key = fts.pack_key
             WHERE marketplace_pack_fts MATCH ?1
             ORDER BY bm25(marketplace_pack_fts, 10.0, 4.0, 1.0) ASC, p.installs DESC
             LIMIT ?2",
        )?;

        let packs = stmt
            .query_map(params![fts_query, limit], |row| {
                Ok(MarketplacePack {
                    pack_key: row.get(0)?,
                    source: row.get(1)?,
                    name: row.get(2)?,
                    description: row.get(3)?,
                    skill_count: row.get(4)?,
                    author: row.get(5)?,
                    git_url: row.get(6)?,
                    installs: row.get(7)?,
                })
            })?
            .filter_map(|r| r.ok())
            .collect();

        Ok(packs)
    })
}

/// List all known packs, ordered by installs descending.
pub fn list_packs_local(limit: u32) -> Result<Vec<MarketplacePack>> {
    let limit = limit.clamp(1, 50) as i64;
    with_conn(|conn| {
        let mut stmt = conn.prepare(
            "SELECT pack_key, source, name, description, skill_count,
                    author, git_url, installs
             FROM marketplace_pack
             ORDER BY installs DESC, name ASC
             LIMIT ?1",
        )?;

        let packs = stmt
            .query_map([limit], |row| {
                Ok(MarketplacePack {
                    pack_key: row.get(0)?,
                    source: row.get(1)?,
                    name: row.get(2)?,
                    description: row.get(3)?,
                    skill_count: row.get(4)?,
                    author: row.get(5)?,
                    git_url: row.get(6)?,
                    installs: row.get(7)?,
                })
            })?
            .filter_map(|r| r.ok())
            .collect();

        Ok(packs)
    })
}

pub(crate) fn upsert_skill_in_tx(
    tx: &Transaction<'_>,
    skill: &Skill,
    synced_at: &str,
) -> Result<Option<String>> {
    let source = skill
        .source
        .as_deref()
        .and_then(normalize_source)
        .or_else(|| {
            extract_github_source_from_url(&skill.git_url)
                .and_then(|value| normalize_source(&value))
        });
    let Some(source) = source else {
        return Ok(None);
    };

    let Some(name) = normalize_skill_name(&skill.name) else {
        return Ok(None);
    };
    let Some(skill_key) = build_skill_key(&source, &name) else {
        return Ok(None);
    };
    let (publisher_name, repo_name) = split_source(&source);
    let git_url = if skill.git_url.trim().is_empty() {
        format!("https://github.com/{source}")
    } else {
        skill.git_url.clone()
    };
    let author = skill.author.clone().unwrap_or_else(|| source.clone());
    let installs = skill.stars as i64;

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
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?10)
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
            installs = MAX(marketplace_skill.installs, excluded.installs),
            last_seen_remote_at = excluded.last_seen_remote_at,
            last_list_sync_at = excluded.last_list_sync_at",
        params![
            skill_key,
            source,
            name,
            git_url.clone(),
            author,
            publisher_name,
            repo_name,
            skill.description,
            installs,
            synced_at
        ],
    )
    .context("Failed to upsert marketplace skill snapshot row")?;

    refresh_fts_entry_in_tx(tx, &skill_key)?;
    upsert_source_observation_in_tx(
        tx,
        MarketplaceSourceObservationUpsert {
            source_id: DEFAULT_CURATED_REGISTRY_ID.to_string(),
            source_skill_id: skill_key.clone(),
            skill_key: skill_key.clone(),
            source_url: "https://skills.sh".to_string(),
            repo_url: git_url,
            version: None,
            sha: skill.tree_hash.clone(),
            metadata_json: None,
            fetched_at: Some(synced_at.to_string()),
        },
    )?;
    Ok(Some(skill_key))
}

pub(crate) fn upsert_skill_identity_in_tx(
    tx: &Transaction<'_>,
    source: &str,
    name: &str,
    installs: u32,
    synced_at: &str,
) -> Result<Option<String>> {
    let source = match normalize_source(source) {
        Some(value) => value,
        None => return Ok(None),
    };
    let name = match normalize_skill_name(name) {
        Some(value) => value,
        None => return Ok(None),
    };
    let (publisher_name, repo_name) = split_source(&source);
    let skill_key = build_skill_key(&source, &name).expect("normalized key");
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
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, '', ?8, ?9, ?9)
        ON CONFLICT(skill_key) DO UPDATE SET
            source = excluded.source,
            name = excluded.name,
            git_url = excluded.git_url,
            author = excluded.author,
            publisher_name = excluded.publisher_name,
            repo_name = excluded.repo_name,
            installs = MAX(marketplace_skill.installs, excluded.installs),
            last_seen_remote_at = excluded.last_seen_remote_at,
            last_list_sync_at = excluded.last_list_sync_at",
        params![
            skill_key,
            source,
            name,
            git_url,
            source,
            publisher_name,
            repo_name,
            installs as i64,
            synced_at
        ],
    )
    .context("Failed to upsert marketplace repo-skill identity row")?;

    refresh_fts_entry_in_tx(tx, &skill_key)?;
    upsert_source_observation_in_tx(
        tx,
        MarketplaceSourceObservationUpsert {
            source_id: DEFAULT_CURATED_REGISTRY_ID.to_string(),
            source_skill_id: skill_key.clone(),
            skill_key: skill_key.clone(),
            source_url: "https://skills.sh".to_string(),
            repo_url: git_url,
            version: None,
            sha: None,
            metadata_json: None,
            fetched_at: Some(synced_at.to_string()),
        },
    )?;
    Ok(Some(skill_key))
}

pub(crate) fn upsert_detail_in_tx(
    tx: &Transaction<'_>,
    source: &str,
    name: &str,
    details: &MarketplaceSkillDetails,
    synced_at: &str,
) -> Result<()> {
    let Some(skill_key) = build_skill_key(source, name) else {
        return Ok(());
    };

    let audits_json = serde_json::to_string(&details.security_audits)
        .context("Failed to serialize marketplace security audits")?;
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
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
        ON CONFLICT(skill_key) DO UPDATE SET
            summary = excluded.summary,
            readme = excluded.readme,
            weekly_installs = excluded.weekly_installs,
            github_stars = excluded.github_stars,
            first_seen = excluded.first_seen,
            security_audits_json = excluded.security_audits_json,
            last_detail_sync_at = excluded.last_detail_sync_at",
        params![
            skill_key,
            details.summary,
            details.readme,
            details.weekly_installs,
            details.github_stars.map(|value| value as i64),
            details.first_seen,
            audits_json,
            synced_at
        ],
    )
    .context("Failed to upsert marketplace detail snapshot row")?;
    refresh_fts_entry_in_tx(tx, &skill_key)?;
    Ok(())
}

pub(crate) fn delete_listing_scope_in_tx(tx: &Transaction<'_>, scope: &str) -> Result<()> {
    tx.execute(
        "DELETE FROM marketplace_listing WHERE listing_type = ?1",
        [scope],
    )
    .with_context(|| format!("Failed to clear marketplace listing scope: {scope}"))?;
    Ok(())
}

pub(crate) fn cleanup_stale_skills_in_tx(tx: &Transaction<'_>) -> Result<()> {
    let installed_markers = installed_markers();
    let cutoff = (Utc::now() - Duration::days(STALE_SKILL_RETENTION_DAYS)).to_rfc3339();

    let mut stmt = tx
        .prepare(
            "SELECT rowid, skill_key, name
             FROM marketplace_skill
             WHERE last_seen_remote_at IS NOT NULL
               AND last_seen_remote_at < ?1",
        )
        .context("Failed to prepare stale marketplace skill query")?;

    let rows = stmt
        .query_map([cutoff], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
            ))
        })
        .context("Failed to scan stale marketplace skills")?;

    for row in rows {
        let (rowid, skill_key, name) =
            row.context("Failed to decode stale marketplace skill row")?;
        if installed_markers.contains(&skill_key)
            || installed_markers.contains(&name.to_ascii_lowercase())
        {
            continue;
        }

        tx.execute(
            "DELETE FROM marketplace_listing WHERE skill_key = ?1",
            [skill_key.as_str()],
        )
        .context("Failed to delete stale marketplace listing rows")?;
        tx.execute(
            "DELETE FROM marketplace_repo_skill WHERE skill_key = ?1",
            [skill_key.as_str()],
        )
        .context("Failed to delete stale marketplace repo-skill rows")?;
        tx.execute(
            "DELETE FROM marketplace_skill_detail WHERE skill_key = ?1",
            [skill_key.as_str()],
        )
        .context("Failed to delete stale marketplace detail row")?;
        // FTS rowid mirrors marketplace_skill.rowid — delete by rowid (the base
        // row is still present here) to avoid a full FTS-index scan per skill.
        tx.execute(
            "DELETE FROM marketplace_skill_fts WHERE rowid = ?1",
            [rowid],
        )
        .context("Failed to delete stale marketplace FTS row")?;
        tx.execute(
            "DELETE FROM marketplace_skill WHERE skill_key = ?1",
            [skill_key.as_str()],
        )
        .context("Failed to delete stale marketplace skill row")?;
    }

    Ok(())
}
