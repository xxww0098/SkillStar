use super::*;

pub(crate) fn table_exists(conn: &Connection, table_name: &str) -> Result<bool> {
    let exists = conn
        .query_row(
            "SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = ?1 LIMIT 1",
            [table_name],
            |_| Ok(()),
        )
        .optional()
        .context("Failed to inspect sqlite schema")?
        .is_some();
    Ok(exists)
}

pub(crate) fn normalize_source(raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }

    let lowered = trimmed
        .trim_start_matches("https://github.com/")
        .trim_start_matches("http://github.com/")
        .trim_end_matches('/')
        .trim_end_matches(".git")
        .to_ascii_lowercase();

    let mut parts = lowered.split('/').filter(|part| !part.is_empty());
    let publisher = parts.next()?;
    let repo = parts.next()?;
    Some(format!("{publisher}/{repo}"))
}

pub(crate) fn normalize_skill_name(raw: &str) -> Option<String> {
    let trimmed = raw.trim().to_ascii_lowercase();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed)
    }
}

pub(crate) fn split_source(source: &str) -> (String, String) {
    let mut parts = source.split('/');
    let publisher = parts.next().unwrap_or_default().to_string();
    let repo = parts.next().unwrap_or_default().to_string();
    (publisher, repo)
}

pub(crate) fn default_curated_registry(now: &str) -> CuratedRegistryEntry {
    CuratedRegistryEntry {
        id: DEFAULT_CURATED_REGISTRY_ID.to_string(),
        name: "skills.sh".to_string(),
        kind: CuratedRegistryKind::SkillsSh,
        endpoint: "https://skills.sh".to_string(),
        enabled: true,
        priority: 0,
        trust: "official".to_string(),
        last_sync_at: None,
        last_error: None,
        created_at: Some(now.to_string()),
        updated_at: Some(now.to_string()),
    }
}

pub(crate) fn normalize_curated_registry_id(id: &str) -> Result<String> {
    let normalized = id.trim().to_ascii_lowercase().replace('.', "_");
    if normalized.is_empty() {
        return Err(anyhow!("Curated registry id cannot be empty"));
    }
    Ok(normalized)
}

pub(crate) fn normalize_observation_source_id(id: &str) -> Result<String> {
    let normalized = id.trim().to_ascii_lowercase().replace('.', "_");
    if normalized.is_empty() {
        return Err(anyhow!("Marketplace source id cannot be empty"));
    }
    Ok(normalized)
}

pub(crate) fn normalize_source_skill_id(id: &str) -> Result<String> {
    let normalized = id.trim().to_ascii_lowercase();
    if normalized.is_empty() {
        return Err(anyhow!("Marketplace source skill id cannot be empty"));
    }
    Ok(normalized)
}

pub(crate) fn normalize_skill_key_value(skill_key: &str) -> Result<String> {
    let normalized = skill_key.trim().to_ascii_lowercase();
    if normalized.is_empty() {
        return Err(anyhow!("Marketplace skill_key cannot be empty"));
    }
    Ok(normalized)
}

pub(crate) fn normalize_marketplace_slug(raw: &str, field: &str) -> Result<String> {
    let mut slug = String::new();
    let mut last_was_separator = false;
    for ch in raw.trim().to_ascii_lowercase().chars() {
        if ch.is_ascii_alphanumeric() {
            slug.push(ch);
            last_was_separator = false;
        } else if matches!(ch, ' ' | '_' | '-' | '.' | '/')
            && !slug.is_empty()
            && !last_was_separator
        {
            slug.push('-');
            last_was_separator = true;
        }
    }
    while slug.ends_with('-') {
        slug.pop();
    }
    if slug.is_empty() {
        return Err(anyhow!("Marketplace {field} slug cannot be empty"));
    }
    Ok(slug)
}

pub(crate) fn normalize_required_label(raw: &str, field: &str) -> Result<String> {
    let label = raw.trim().to_string();
    if label.is_empty() {
        return Err(anyhow!("Marketplace {field} label cannot be empty"));
    }
    Ok(label)
}

pub(crate) fn normalize_optional_source_id(source_id: Option<String>) -> String {
    source_id
        .unwrap_or_default()
        .trim()
        .to_ascii_lowercase()
        .replace('.', "_")
}

pub(crate) fn none_if_empty(value: String) -> Option<String> {
    if value.is_empty() { None } else { Some(value) }
}

pub(crate) fn trim_optional(value: Option<String>) -> Option<String> {
    value
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

pub(crate) fn validate_rating_value(rating: i64) -> Result<()> {
    if !(1..=5).contains(&rating) {
        return Err(anyhow!("Marketplace review rating must be between 1 and 5"));
    }
    Ok(())
}

pub(crate) fn validate_rating_summary_values(
    rating_avg: f64,
    rating_count: i64,
    review_count: i64,
) -> Result<()> {
    if !rating_avg.is_finite() || !(0.0..=5.0).contains(&rating_avg) {
        return Err(anyhow!(
            "Marketplace rating average must be between 0 and 5"
        ));
    }
    if rating_count < 0 || review_count < 0 {
        return Err(anyhow!("Marketplace rating counts cannot be negative"));
    }
    Ok(())
}

pub(crate) fn curated_registry_kind_from_db(raw: &str) -> CuratedRegistryKind {
    raw.parse().unwrap_or(CuratedRegistryKind::Custom)
}

pub(crate) fn row_to_curated_registry(row: &rusqlite::Row<'_>) -> rusqlite::Result<CuratedRegistryEntry> {
    let kind: String = row.get(2)?;
    Ok(CuratedRegistryEntry {
        id: row.get(0)?,
        name: row.get(1)?,
        kind: curated_registry_kind_from_db(&kind),
        endpoint: row.get(3)?,
        enabled: row.get::<_, i64>(4)? != 0,
        priority: row.get(5)?,
        trust: row.get(6)?,
        last_sync_at: row.get(7)?,
        last_error: row.get(8)?,
        created_at: row.get(9)?,
        updated_at: row.get(10)?,
    })
}

pub(crate) fn row_to_source_observation(
    row: &rusqlite::Row<'_>,
) -> rusqlite::Result<MarketplaceSourceObservation> {
    Ok(MarketplaceSourceObservation {
        source_id: row.get(0)?,
        source_skill_id: row.get(1)?,
        skill_key: row.get(2)?,
        source_url: row.get(3)?,
        repo_url: row.get(4)?,
        version: row.get(5)?,
        sha: row.get(6)?,
        metadata_json: row.get(7)?,
        fetched_at: row.get(8)?,
        created_at: row.get(9)?,
        updated_at: row.get(10)?,
    })
}

pub(crate) fn row_to_category(row: &rusqlite::Row<'_>) -> rusqlite::Result<MarketplaceCategory> {
    Ok(MarketplaceCategory {
        id: row.get(0)?,
        label: row.get(1)?,
        slug: row.get(2)?,
        parent_id: row.get(3)?,
        position: row.get(4)?,
        created_at: row.get(5)?,
        updated_at: row.get(6)?,
    })
}

pub(crate) fn row_to_tag(row: &rusqlite::Row<'_>) -> rusqlite::Result<MarketplaceTag> {
    Ok(MarketplaceTag {
        slug: row.get(0)?,
        label: row.get(1)?,
        usage_count: row.get(2)?,
        created_at: row.get(3)?,
        updated_at: row.get(4)?,
    })
}

pub(crate) fn row_to_skill_category_assignment(
    row: &rusqlite::Row<'_>,
) -> rusqlite::Result<MarketplaceSkillCategoryAssignment> {
    Ok(MarketplaceSkillCategoryAssignment {
        skill_key: row.get(0)?,
        category_id: row.get(1)?,
        assigned_at: row.get(2)?,
    })
}

pub(crate) fn row_to_skill_tag_assignment(
    row: &rusqlite::Row<'_>,
) -> rusqlite::Result<MarketplaceSkillTagAssignment> {
    let source_id: String = row.get(2)?;
    Ok(MarketplaceSkillTagAssignment {
        skill_key: row.get(0)?,
        tag_slug: row.get(1)?,
        source_id: none_if_empty(source_id),
        assigned_at: row.get(3)?,
    })
}

pub(crate) fn row_to_rating_summary(row: &rusqlite::Row<'_>) -> rusqlite::Result<MarketplaceRatingSummary> {
    let source_id: String = row.get(1)?;
    Ok(MarketplaceRatingSummary {
        skill_key: row.get(0)?,
        source_id: none_if_empty(source_id),
        rating_avg: row.get(2)?,
        rating_count: row.get(3)?,
        review_count: row.get(4)?,
        last_review_at: row.get(5)?,
        updated_at: row.get(6)?,
    })
}

pub(crate) fn row_to_review(row: &rusqlite::Row<'_>) -> rusqlite::Result<MarketplaceReview> {
    let source_id: String = row.get(2)?;
    Ok(MarketplaceReview {
        review_id: row.get(0)?,
        skill_key: row.get(1)?,
        source_id: none_if_empty(source_id),
        author_hash: row.get(3)?,
        rating: row.get(4)?,
        title: row.get(5)?,
        body: row.get(6)?,
        locale: row.get(7)?,
        status: row.get(8)?,
        reviewed_at: row.get(9)?,
        created_at: row.get(10)?,
        updated_at: row.get(11)?,
    })
}

pub(crate) fn row_to_update_notification(
    row: &rusqlite::Row<'_>,
) -> rusqlite::Result<MarketplaceUpdateNotification> {
    Ok(MarketplaceUpdateNotification {
        skill_key: row.get(0)?,
        source_id: row.get(1)?,
        installed_version: row.get(2)?,
        available_version: row.get(3)?,
        installed_hash: row.get(4)?,
        available_hash: row.get(5)?,
        detected_at: row.get(6)?,
        dismissed_at: row.get(7)?,
        message: row.get(8)?,
        metadata_json: row.get(9)?,
        updated_at: row.get(10)?,
    })
}

pub(crate) fn seed_default_curated_registry(conn: &Connection) -> Result<()> {
    let now = now_rfc3339();
    let entry = default_curated_registry(&now);
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
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)
        ON CONFLICT(id) DO NOTHING",
        params![
            entry.id,
            entry.name,
            entry.kind.as_str(),
            entry.endpoint,
            i64::from(entry.enabled),
            entry.priority,
            entry.trust,
            entry.last_sync_at,
            entry.last_error,
            entry.created_at,
            entry.updated_at
        ],
    )
    .context("Failed to seed default curated marketplace registry")?;
    Ok(())
}

pub(crate) fn build_skill_key(source: &str, name: &str) -> Option<String> {
    let source = normalize_source(source)?;
    let name = normalize_skill_name(name)?;
    Some(format!("{source}/{name}"))
}

pub(crate) fn parse_skill_key(skill_key: &str) -> Option<(String, String)> {
    let normalized = skill_key.trim().to_ascii_lowercase();
    let mut parts = normalized.split('/').filter(|part| !part.is_empty());
    let publisher = parts.next()?;
    let repo = parts.next()?;
    let name = parts.collect::<Vec<_>>().join("/");
    if name.is_empty() {
        return None;
    }
    Some((format!("{publisher}/{repo}"), name))
}
