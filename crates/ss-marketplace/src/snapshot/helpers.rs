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

/// Does `table_name` already have `column_name`? Used by migrations to make
/// `ALTER TABLE ADD COLUMN` steps repeatable — SQLite rejects a duplicate add
/// with `duplicate column name`, which would otherwise turn a partially
/// applied migration into a permanent startup failure.
pub(crate) fn column_exists(
    conn: &Connection,
    table_name: &str,
    column_name: &str,
) -> Result<bool> {
    let mut stmt = conn
        .prepare(&format!("PRAGMA table_info({table_name})"))
        .with_context(|| format!("Failed to inspect columns of {table_name}"))?;
    let rows = stmt
        .query_map([], |row| row.get::<_, String>(1))
        .with_context(|| format!("Failed to read columns of {table_name}"))?;
    for name in rows {
        if name.with_context(|| format!("Failed to decode column of {table_name}"))? == column_name
        {
            return Ok(true);
        }
    }
    Ok(false)
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

pub(crate) fn seed_default_curated_registry(conn: &Connection) -> Result<()> {
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
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)
        ON CONFLICT(id) DO NOTHING",
        params![
            "skills_sh",
            "skills.sh",
            "skills_sh",
            "https://skills.sh",
            1_i64,
            0_i64,
            "official",
            Option::<String>::None,
            Option::<String>::None,
            now,
            now
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
