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

pub async fn sync_scope_leaderboard(category: &str) -> Result<()> {
    let scope = leaderboard_scope(category);
    mark_scope_attempt(&scope)?;
    let synced_at = now_rfc3339();
    let skills = remote::get_skills_sh_leaderboard(category)
        .await
        .with_context(|| format!("Failed to fetch remote leaderboard: {category}"))?;

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
        mark_scope_success_in_tx(&tx, &scope)?;
        tx.commit()
            .context("Failed to commit leaderboard snapshot transaction")?;
        Ok(())
    });

    if let Err(err) = write_result {
        let _ = mark_scope_error(&scope, &err.to_string());
        return Err(err);
    }

    Ok(())
}

pub async fn sync_scope_publishers() -> Result<()> {
    let scope = "official_publishers";
    mark_scope_attempt(scope)?;
    let synced_at = now_rfc3339();
    let publishers = remote::get_official_publishers()
        .await
        .context("Failed to fetch remote official publishers")?;

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

        mark_scope_success_in_tx(&tx, scope)?;
        tx.commit()
            .context("Failed to commit publisher snapshot transaction")?;
        Ok(())
    });

    if let Err(err) = write_result {
        let _ = mark_scope_error(scope, &err.to_string());
        return Err(err);
    }

    Ok(())
}

pub async fn sync_scope_publisher_repos(publisher_name: &str) -> Result<()> {
    let publisher_name = publisher_name.trim().to_ascii_lowercase();
    let scope = format!("publisher_repos:{publisher_name}");
    mark_scope_attempt(&scope)?;
    let synced_at = now_rfc3339();
    let repos = remote::get_publisher_repos(&publisher_name)
        .await
        .with_context(|| format!("Failed to fetch repos for publisher {publisher_name}"))?;

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

            if !repo.skills.is_empty() {
                tx.execute(
                    "DELETE FROM marketplace_repo_skill WHERE source = ?1",
                    [repo.source.to_ascii_lowercase()],
                )
                .context("Failed to clear embedded repo-skill snapshot rows")?;

                for (index, skill) in repo.skills.iter().enumerate() {
                    if let Some(skill_key) = upsert_skill_identity_in_tx(
                        &tx,
                        &repo.source,
                        &skill.name,
                        skill.installs,
                        &synced_at,
                    )? {
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
                                repo.source.to_ascii_lowercase(),
                                skill_key,
                                skill.installs as i64,
                                (index + 1) as i64,
                                synced_at
                            ],
                        )
                        .context("Failed to upsert embedded repo-skill snapshot row")?;
                    }
                }
            }
        }

        mark_scope_success_in_tx(&tx, &scope)?;
        tx.commit()
            .context("Failed to commit publisher-repo snapshot transaction")?;
        Ok(())
    });

    if let Err(err) = write_result {
        let _ = mark_scope_error(&scope, &err.to_string());
        return Err(err);
    }

    Ok(())
}

pub async fn sync_scope_repo_skills(source: &str) -> Result<()> {
    let source = normalize_source(source).ok_or_else(|| anyhow!("Invalid repo source"))?;
    let scope = format!("repo_skills:{source}");
    mark_scope_attempt(&scope)?;
    let synced_at = now_rfc3339();
    let (publisher_name, repo_name) = split_source(&source);
    let skills = remote::get_publisher_repo_skills(&publisher_name, &repo_name)
        .await
        .with_context(|| format!("Failed to fetch repo skills for {source}"))?;

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

        mark_scope_success_in_tx(&tx, &scope)?;
        tx.commit()
            .context("Failed to commit repo-skill snapshot transaction")?;
        Ok(())
    });

    if let Err(err) = write_result {
        let _ = mark_scope_error(&scope, &err.to_string());
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
    let details = remote::fetch_marketplace_skill_details(&source, &name)
        .await
        .with_context(|| format!("Failed to fetch marketplace detail for {source}/{name}"))?;

    let write_result: Result<()> = with_conn(|conn| {
        let tx = conn
            .unchecked_transaction()
            .context("Failed to start skill-detail snapshot transaction")?;

        mark_scope_attempt_in_tx(&tx, &scope)?;
        let _ = upsert_skill_identity_in_tx(&tx, &source, &name, 0, &synced_at)?;
        upsert_detail_in_tx(&tx, &source, &name, &details, &synced_at)?;
        mark_scope_success_in_tx(&tx, &scope)?;
        tx.commit()
            .context("Failed to commit skill-detail snapshot transaction")?;
        Ok(())
    });

    if let Err(err) = write_result {
        let _ = mark_scope_error(&scope, &err.to_string());
        return Err(err);
    }

    Ok(())
}

pub(crate) async fn seed_search_results(query: &str, limit: u32) -> Result<()> {
    let trimmed = query.trim();
    if trimmed.is_empty() {
        return Ok(());
    }

    let synced_at = now_rfc3339();
    let result = remote::search_skills_sh(trimmed, limit)
        .await
        .with_context(|| {
            format!("Failed to seed marketplace search results for query '{trimmed}'")
        })?;

    with_conn(|conn| {
        let tx = conn
            .unchecked_transaction()
            .context("Failed to start marketplace search seed transaction")?;

        for skill in &result.skills {
            let _ = upsert_skill_in_tx(&tx, skill, &synced_at)?;
        }

        cleanup_stale_skills_in_tx(&tx)?;
        tx.commit()
            .context("Failed to commit marketplace search seed transaction")?;
        Ok(())
    })
}
