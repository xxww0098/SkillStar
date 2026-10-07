use super::*;

pub fn list_curated_registries() -> Result<Vec<CuratedRegistryEntry>> {
    with_conn(|conn| {
        seed_default_curated_registry(conn)?;
        let mut stmt = conn
            .prepare(
                "SELECT
                    id,
                    name,
                    kind,
                    endpoint,
                    enabled,
                    priority,
                    trust,
                    last_sync_at,
                    last_error,
                    created_at,
                    updated_at
                 FROM marketplace_curated_registry
                 ORDER BY enabled DESC, priority ASC, name ASC, id ASC",
            )
            .context("Failed to prepare curated marketplace registry query")?;

        let rows = stmt
            .query_map([], row_to_curated_registry)
            .context("Failed to read curated marketplace registry rows")?;

        let mut entries = Vec::new();
        for row in rows {
            entries.push(row.context("Failed to decode curated marketplace registry row")?);
        }
        Ok(entries)
    })
}

pub fn upsert_curated_registry(input: CuratedRegistryUpsert) -> Result<CuratedRegistryEntry> {
    with_conn(|conn| {
        let id = normalize_curated_registry_id(&input.id)?;
        let name = input.name.trim();
        if name.is_empty() {
            return Err(anyhow!("Curated registry name cannot be empty"));
        }

        let now = now_rfc3339();
        conn.execute(
            "INSERT INTO marketplace_curated_registry (
                id,
                name,
                kind,
                endpoint,
                enabled,
                priority,
                trust,
                last_sync_at,
                last_error,
                created_at,
                updated_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?10)
            ON CONFLICT(id) DO UPDATE SET
                name = excluded.name,
                kind = excluded.kind,
                endpoint = excluded.endpoint,
                enabled = excluded.enabled,
                priority = excluded.priority,
                trust = excluded.trust,
                last_sync_at = excluded.last_sync_at,
                last_error = excluded.last_error,
                updated_at = excluded.updated_at",
            params![
                id,
                name,
                input.kind.as_str(),
                input.endpoint.trim(),
                i64::from(input.enabled),
                input.priority,
                input.trust.trim(),
                input.last_sync_at,
                input.last_error,
                now
            ],
        )
        .context("Failed to upsert curated marketplace registry")?;

        conn.query_row(
            "SELECT
                id,
                name,
                kind,
                endpoint,
                enabled,
                priority,
                trust,
                last_sync_at,
                last_error,
                created_at,
                updated_at
             FROM marketplace_curated_registry
             WHERE id = ?1",
            [id],
            row_to_curated_registry,
        )
        .optional()
        .context("Failed to load upserted curated marketplace registry")?
        .ok_or_else(|| anyhow!("Curated marketplace registry was not persisted"))
    })
}

pub(crate) fn upsert_source_observation_in_tx(
    tx: &Transaction<'_>,
    observation: MarketplaceSourceObservationUpsert,
) -> Result<MarketplaceSourceObservation> {
    let source_id = normalize_observation_source_id(&observation.source_id)?;
    let source_skill_id = normalize_source_skill_id(&observation.source_skill_id)?;
    let skill_key = observation.skill_key.trim().to_ascii_lowercase();
    if skill_key.is_empty() {
        return Err(anyhow!(
            "Marketplace source observation skill_key cannot be empty"
        ));
    }

    let now = now_rfc3339();
    let source_url = observation.source_url.trim().to_string();
    let repo_url = observation.repo_url.trim().to_string();
    tx.execute(
        "INSERT INTO marketplace_skill_source_observation (
            source_id,
            source_skill_id,
            skill_key,
            source_url,
            repo_url,
            version,
            sha,
            metadata_json,
            fetched_at,
            created_at,
            updated_at
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?10)
        ON CONFLICT(source_id, source_skill_id) DO UPDATE SET
            skill_key = excluded.skill_key,
            source_url = excluded.source_url,
            repo_url = excluded.repo_url,
            version = excluded.version,
            sha = excluded.sha,
            metadata_json = excluded.metadata_json,
            fetched_at = excluded.fetched_at,
            updated_at = excluded.updated_at",
        params![
            source_id,
            source_skill_id,
            skill_key,
            source_url,
            repo_url,
            observation.version,
            observation.sha,
            observation.metadata_json,
            observation.fetched_at,
            now
        ],
    )
    .context("Failed to upsert marketplace source observation")?;

    tx.query_row(
        "SELECT
            source_id,
            source_skill_id,
            skill_key,
            source_url,
            repo_url,
            version,
            sha,
            metadata_json,
            fetched_at,
            created_at,
            updated_at
         FROM marketplace_skill_source_observation
         WHERE source_id = ?1 AND source_skill_id = ?2",
        params![source_id, source_skill_id],
        row_to_source_observation,
    )
    .context("Failed to load upserted marketplace source observation")
}

pub fn upsert_source_observation(
    observation: MarketplaceSourceObservationUpsert,
) -> Result<MarketplaceSourceObservation> {
    with_conn(|conn| {
        let tx = conn
            .unchecked_transaction()
            .context("Failed to start source-observation upsert transaction")?;
        let persisted = upsert_source_observation_in_tx(&tx, observation)?;
        tx.commit()
            .context("Failed to commit source-observation upsert transaction")?;
        Ok(persisted)
    })
}

pub fn list_source_observations_for_skill(
    skill_key: &str,
) -> Result<Vec<MarketplaceSourceObservation>> {
    let skill_key = skill_key.trim().to_ascii_lowercase();
    if skill_key.is_empty() {
        return Ok(Vec::new());
    }

    with_conn(|conn| {
        let mut stmt = conn
            .prepare(
                "SELECT
                    source_id,
                    source_skill_id,
                    skill_key,
                    source_url,
                    repo_url,
                    version,
                    sha,
                    metadata_json,
                    fetched_at,
                    created_at,
                    updated_at
                 FROM marketplace_skill_source_observation
                 WHERE skill_key = ?1
                 ORDER BY source_id ASC, COALESCE(fetched_at, updated_at) DESC, source_skill_id ASC",
            )
            .context("Failed to prepare marketplace source-observation query")?;
        let rows = stmt
            .query_map([skill_key], row_to_source_observation)
            .context("Failed to read marketplace source-observation rows")?;

        let mut observations = Vec::new();
        for row in rows {
            observations.push(row.context("Failed to decode marketplace source observation")?);
        }
        Ok(observations)
    })
}

pub fn list_known_marketplace_sources() -> Result<Vec<MarketplaceSourceSummary>> {
    with_conn(|conn| {
        let mut stmt = conn
            .prepare(
                "SELECT
                    source_id,
                    COUNT(1) AS observation_count,
                    MAX(fetched_at) AS last_fetched_at,
                    MAX(updated_at) AS last_updated_at
                 FROM marketplace_skill_source_observation
                 GROUP BY source_id
                 ORDER BY source_id ASC",
            )
            .context("Failed to prepare known marketplace source query")?;
        let rows = stmt
            .query_map([], |row| {
                Ok(MarketplaceSourceSummary {
                    source_id: row.get(0)?,
                    observation_count: row.get(1)?,
                    last_fetched_at: row.get(2)?,
                    last_updated_at: row.get(3)?,
                })
            })
            .context("Failed to read known marketplace source rows")?;

        let mut sources = Vec::new();
        for row in rows {
            sources.push(row.context("Failed to decode known marketplace source row")?);
        }
        Ok(sources)
    })
}
