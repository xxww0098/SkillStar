use super::*;

pub(crate) struct SnapshotSkillRow {
    pub(crate) source: String,
    pub(crate) name: String,
    pub(crate) git_url: String,
    pub(crate) author: Option<String>,
    pub(crate) description: String,
    pub(crate) installs: u32,
    pub(crate) last_updated: Option<String>,
    pub(crate) rank: Option<u32>,
}

pub(crate) fn skill_from_snapshot_row(row: SnapshotSkillRow) -> Skill {
    let SnapshotSkillRow {
        source,
        name,
        git_url,
        author,
        description,
        installs,
        last_updated,
        rank,
    } = row;
    let skill_author = author.unwrap_or_else(|| source.clone());
    let mut skill =
        Skill::from_skills_sh(name, description, installs, skill_author.clone(), git_url);
    skill.skill_type = SkillType::Hub;
    skill.author = Some(skill_author);
    skill.source = Some(source);
    skill.last_updated = last_updated.unwrap_or_else(now_rfc3339);
    skill.rank = rank;
    skill.classify();
    skill
}

pub(crate) fn decode_security_audits(raw: Option<String>) -> Vec<SecurityAudit> {
    raw.and_then(|value| serde_json::from_str::<Vec<SecurityAudit>>(&value).ok())
        .unwrap_or_default()
}

pub(crate) fn load_leaderboard_snapshot(conn: &Connection, scope: &str) -> Result<Vec<Skill>> {
    let mut stmt = conn
        .prepare(
            "SELECT
                s.source,
                s.name,
                s.git_url,
                s.author,
                s.description,
                s.installs,
                s.last_list_sync_at,
                l.rank
             FROM marketplace_listing l
             JOIN marketplace_skill s ON s.skill_key = l.skill_key
             WHERE l.listing_type = ?1
             ORDER BY l.rank ASC, s.installs DESC, s.name ASC",
        )
        .context("Failed to prepare leaderboard snapshot query")?;

    let rows = stmt
        .query_map([scope], |row| {
            Ok(skill_from_snapshot_row(SnapshotSkillRow {
                source: row.get(0)?,
                name: row.get(1)?,
                git_url: row.get(2)?,
                author: row.get(3)?,
                description: row.get(4)?,
                installs: row.get::<_, i64>(5)?.max(0) as u32,
                last_updated: row.get(6)?,
                rank: row
                    .get::<_, Option<i64>>(7)?
                    .map(|value| value.max(0) as u32),
            }))
        })
        .context("Failed to read leaderboard snapshot rows")?;

    let mut skills = Vec::new();
    for row in rows {
        skills.push(row.context("Failed to decode leaderboard skill row")?);
    }
    Ok(skills)
}

pub(crate) fn load_all_skills_snapshot(conn: &Connection) -> Result<(Vec<Skill>, Option<String>)> {
    let mut stmt = conn
        .prepare(
            "SELECT
                source,
                name,
                git_url,
                author,
                description,
                installs,
                last_list_sync_at
             FROM marketplace_skill
             ORDER BY installs DESC, name ASC",
        )
        .context("Failed to prepare full marketplace skill snapshot query")?;

    let rows = stmt
        .query_map([], |row| {
            Ok(skill_from_snapshot_row(SnapshotSkillRow {
                source: row.get(0)?,
                name: row.get(1)?,
                git_url: row.get(2)?,
                author: row.get(3)?,
                description: row.get(4)?,
                installs: row.get::<_, i64>(5)?.max(0) as u32,
                last_updated: row.get(6)?,
                rank: None,
            }))
        })
        .context("Failed to read full marketplace skill snapshot rows")?;

    let mut skills = Vec::new();
    for row in rows {
        skills.push(row.context("Failed to decode full marketplace skill row")?);
    }

    let updated_at: Option<String> = conn
        .query_row(
            "SELECT MAX(last_list_sync_at) FROM marketplace_skill",
            [],
            |row| row.get(0),
        )
        .optional()
        .context("Failed to read full marketplace skill snapshot timestamp")?
        .flatten();

    Ok((skills, updated_at))
}

pub(crate) fn any_skill_rows(conn: &Connection) -> Result<bool> {
    let count: i64 = conn
        .query_row("SELECT COUNT(1) FROM marketplace_skill", [], |row| {
            row.get(0)
        })
        .context("Failed to count marketplace skills")?;
    Ok(count > 0)
}

pub(crate) fn skill_row_count(conn: &Connection) -> Result<i64> {
    conn.query_row("SELECT COUNT(1) FROM marketplace_skill", [], |row| {
        row.get(0)
    })
    .context("Failed to count marketplace skills")
}

pub(crate) fn load_search_snapshot(
    conn: &Connection,
    query: &str,
    limit: u32,
) -> Result<(Vec<Skill>, Option<String>)> {
    let limit = limit.clamp(1, 200) as i64;
    let normalized_query = query.trim().to_ascii_lowercase();

    if normalized_query.is_empty() {
        let mut stmt = conn
            .prepare(
                "SELECT
                    source,
                    name,
                    git_url,
                    author,
                    description,
                    installs,
                    last_list_sync_at
                 FROM marketplace_skill
                 ORDER BY installs DESC, name ASC
                 LIMIT ?1",
            )
            .context("Failed to prepare blank search snapshot query")?;
        let rows = stmt
            .query_map([limit], |row| {
                Ok(skill_from_snapshot_row(SnapshotSkillRow {
                    source: row.get(0)?,
                    name: row.get(1)?,
                    git_url: row.get(2)?,
                    author: row.get(3)?,
                    description: row.get(4)?,
                    installs: row.get::<_, i64>(5)?.max(0) as u32,
                    last_updated: row.get(6)?,
                    rank: None,
                }))
            })
            .context("Failed to read blank search snapshot rows")?;

        let mut skills = Vec::new();
        for row in rows {
            skills.push(row.context("Failed to decode blank search row")?);
        }
        let updated_at: Option<String> = conn
            .query_row(
                "SELECT MAX(last_list_sync_at) FROM marketplace_skill",
                [],
                |row| row.get(0),
            )
            .optional()
            .context("Failed to read marketplace search snapshot timestamp")?
            .flatten();
        return Ok((skills, updated_at));
    }

    let Some(fts_query) = build_fts_query(&normalized_query) else {
        return Ok((Vec::new(), None));
    };
    let prefix_query = format!("{normalized_query}%");

    let mut stmt = conn
        .prepare(
            "SELECT
                s.source,
                s.name,
                s.git_url,
                s.author,
                s.description,
                s.installs,
                s.last_list_sync_at
             FROM marketplace_skill_fts fts
             JOIN marketplace_skill s ON s.skill_key = fts.skill_key
             WHERE marketplace_skill_fts MATCH ?1
             ORDER BY
                CASE
                    WHEN lower(s.name) = ?2 THEN 0
                    WHEN lower(s.name) LIKE ?3 THEN 1
                    ELSE 2
                END ASC,
                bm25(marketplace_skill_fts, 10.0, 4.0, 2.0, 1.0, 1.0) ASC,
                s.installs DESC,
                s.name ASC
             LIMIT ?4",
        )
        .context("Failed to prepare marketplace FTS query")?;

    let rows = stmt
        .query_map(
            params![fts_query, normalized_query, prefix_query, limit],
            |row| {
                Ok(skill_from_snapshot_row(SnapshotSkillRow {
                    source: row.get(0)?,
                    name: row.get(1)?,
                    git_url: row.get(2)?,
                    author: row.get(3)?,
                    description: row.get(4)?,
                    installs: row.get::<_, i64>(5)?.max(0) as u32,
                    last_updated: row.get(6)?,
                    rank: None,
                }))
            },
        )
        .context("Failed to execute marketplace FTS query")?;

    let mut skills = Vec::new();
    let mut latest_updated_at: Option<String> = None;
    for row in rows {
        let skill = row.context("Failed to decode marketplace FTS row")?;
        if latest_updated_at
            .as_ref()
            .is_none_or(|current| skill.last_updated > *current)
        {
            latest_updated_at = Some(skill.last_updated.clone());
        }
        skills.push(skill);
    }

    Ok((skills, latest_updated_at))
}

pub(crate) fn load_publishers_snapshot(conn: &Connection) -> Result<Vec<OfficialPublisher>> {
    let mut stmt = conn
        .prepare(
            "SELECT publisher_name, repo_count, skill_count, url
             FROM marketplace_publisher
             ORDER BY skill_count DESC, repo_count DESC, publisher_name ASC",
        )
        .context("Failed to prepare publisher snapshot query")?;

    let rows = stmt
        .query_map([], |row| {
            Ok(OfficialPublisher {
                name: row.get(0)?,
                repo: "skills".to_string(),
                repo_count: row.get::<_, i64>(1)?.max(0) as u32,
                skill_count: row.get::<_, i64>(2)?.max(0) as u32,
                url: row.get(3)?,
            })
        })
        .context("Failed to read publisher snapshot rows")?;

    let mut publishers = Vec::new();
    for row in rows {
        publishers.push(row.context("Failed to decode publisher row")?);
    }
    Ok(publishers)
}

pub(crate) fn load_publisher_repos_snapshot(
    conn: &Connection,
    publisher_name: &str,
) -> Result<Vec<PublisherRepo>> {
    let mut stmt = conn
        .prepare(
            "SELECT source, repo_name, skill_count, installs, installs_label, url
             FROM marketplace_repo
             WHERE publisher_name = ?1
             ORDER BY installs DESC, repo_name ASC",
        )
        .context("Failed to prepare publisher repo snapshot query")?;

    let rows = stmt
        .query_map([publisher_name], |row| {
            Ok(PublisherRepo {
                source: row.get(0)?,
                repo: row.get(1)?,
                skill_count: row.get::<_, i64>(2)?.max(0) as u32,
                installs: row.get::<_, i64>(3)?.max(0) as u32,
                installs_label: row.get(4)?,
                url: row.get(5)?,
                skills: Vec::new(),
            })
        })
        .context("Failed to read publisher repo snapshot rows")?;

    let mut repos = Vec::new();
    for row in rows {
        repos.push(row.context("Failed to decode publisher repo row")?);
    }
    Ok(repos)
}

pub(crate) fn load_repo_skills_snapshot(conn: &Connection, source: &str) -> Result<Vec<Skill>> {
    let mut stmt = conn
        .prepare(
            "SELECT
                s.source,
                s.name,
                s.git_url,
                s.author,
                s.description,
                COALESCE(rs.installs, s.installs),
                COALESCE(s.last_list_sync_at, rs.updated_at),
                rs.rank
             FROM marketplace_repo_skill rs
             JOIN marketplace_skill s ON s.skill_key = rs.skill_key
             WHERE rs.source = ?1
             ORDER BY
                CASE WHEN rs.rank IS NULL THEN 1 ELSE 0 END ASC,
                rs.rank ASC,
                rs.installs DESC,
                s.name ASC",
        )
        .context("Failed to prepare repo-skill snapshot query")?;

    let rows = stmt
        .query_map([source], |row| {
            Ok(skill_from_snapshot_row(SnapshotSkillRow {
                source: row.get(0)?,
                name: row.get(1)?,
                git_url: row.get(2)?,
                author: row.get(3)?,
                description: row.get(4)?,
                installs: row.get::<_, i64>(5)?.max(0) as u32,
                last_updated: row.get(6)?,
                rank: row
                    .get::<_, Option<i64>>(7)?
                    .map(|value| value.max(0) as u32),
            }))
        })
        .context("Failed to read repo-skill snapshot rows")?;

    let mut skills = Vec::new();
    for row in rows {
        skills.push(row.context("Failed to decode repo-skill row")?);
    }
    Ok(skills)
}

pub(crate) fn load_skill_detail_snapshot(
    conn: &Connection,
    skill_key: &str,
) -> Result<Option<MarketplaceSkillDetails>> {
    conn.query_row(
        "SELECT summary, readme, weekly_installs, github_stars, first_seen, security_audits_json
         FROM marketplace_skill_detail
         WHERE skill_key = ?1",
        [skill_key],
        |row| {
            Ok(MarketplaceSkillDetails {
                summary: row.get(0)?,
                readme: row.get(1)?,
                weekly_installs: row.get(2)?,
                github_stars: row
                    .get::<_, Option<i64>>(3)?
                    .map(|value| value.max(0) as u32),
                first_seen: row.get(4)?,
                security_audits: decode_security_audits(row.get(5)?),
            })
        },
    )
    .optional()
    .context("Failed to load marketplace detail snapshot")
}
