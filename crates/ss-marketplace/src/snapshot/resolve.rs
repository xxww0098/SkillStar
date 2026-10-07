use super::*;

pub(crate) fn normalize_resolve_requests(names: &[String]) -> Vec<ResolveSkillRequest> {
    names
        .iter()
        .filter_map(|name| {
            let original_name = name.trim().to_string();
            let normalized_name = normalize_skill_name(&original_name)?;
            Some(ResolveSkillRequest {
                original_name,
                normalized_name,
            })
        })
        .collect()
}

pub(crate) fn existing_named_sources(
    existing_sources: &HashMap<String, String>,
) -> HashMap<String, String> {
    existing_sources
        .iter()
        .filter_map(|(name, url)| {
            let normalized_name = normalize_skill_name(name)?;
            let trimmed_url = url.trim();
            if trimmed_url.is_empty() {
                None
            } else {
                Some((normalized_name, trimmed_url.to_string()))
            }
        })
        .collect()
}

pub(crate) fn preferred_source_repos(
    existing_sources: &HashMap<String, String>,
) -> HashSet<String> {
    existing_sources
        .values()
        .filter_map(|value| {
            normalize_source(value).or_else(|| {
                extract_github_source_from_url(value).and_then(|source| normalize_source(&source))
            })
        })
        .collect()
}

pub(crate) fn load_exact_source_candidates(
    conn: &Connection,
    normalized_names: &[String],
) -> Result<HashMap<String, Vec<ResolveSourceCandidate>>> {
    let mut candidates: HashMap<String, Vec<ResolveSourceCandidate>> = HashMap::new();
    if normalized_names.is_empty() {
        return Ok(candidates);
    }

    let placeholders = (1..=normalized_names.len())
        .map(|index| format!("?{index}"))
        .collect::<Vec<_>>()
        .join(", ");
    let sql = format!(
        "SELECT name, source, git_url, installs
         FROM marketplace_skill
         WHERE name IN ({placeholders})
           AND git_url <> ''
         ORDER BY name ASC, installs DESC, source ASC"
    );
    let mut stmt = conn
        .prepare(&sql)
        .context("Failed to prepare marketplace source-resolution query")?;

    let params: Vec<&dyn rusqlite::types::ToSql> = normalized_names
        .iter()
        .map(|name| name as &dyn rusqlite::types::ToSql)
        .collect();
    let rows = stmt
        .query_map(params.as_slice(), |row| {
            Ok((
                row.get::<_, String>(0)?,
                ResolveSourceCandidate {
                    source: row.get(1)?,
                    git_url: row.get(2)?,
                    installs: row.get::<_, i64>(3)?.max(0) as u32,
                },
            ))
        })
        .context("Failed to read marketplace source-resolution rows")?;

    for row in rows {
        let (name, candidate) = row.context("Failed to decode marketplace source candidate")?;
        candidates.entry(name).or_default().push(candidate);
    }

    Ok(candidates)
}

pub(crate) fn unique_top_install_candidate<'a>(
    candidates: &[&'a ResolveSourceCandidate],
) -> Option<&'a ResolveSourceCandidate> {
    let mut sorted = candidates.to_vec();
    sorted.sort_by(|left, right| {
        right
            .installs
            .cmp(&left.installs)
            .then_with(|| left.source.cmp(&right.source))
    });

    let top = sorted.first().copied()?;
    let next = sorted.get(1).copied();
    if next.is_none_or(|candidate| candidate.installs < top.installs) {
        Some(top)
    } else {
        None
    }
}

pub(crate) fn choose_source_candidate(
    candidates: Option<&[ResolveSourceCandidate]>,
    preferred_repos: &HashSet<String>,
) -> Option<String> {
    let candidates = candidates?;
    if candidates.is_empty() {
        return None;
    }
    if candidates.len() == 1 {
        return Some(candidates[0].git_url.clone());
    }

    let preferred: Vec<&ResolveSourceCandidate> = candidates
        .iter()
        .filter(|candidate| preferred_repos.contains(&candidate.source))
        .collect();

    if preferred.len() == 1 {
        return Some(preferred[0].git_url.clone());
    }

    if !preferred.is_empty() {
        return unique_top_install_candidate(&preferred).map(|candidate| candidate.git_url.clone());
    }

    let all: Vec<&ResolveSourceCandidate> = candidates.iter().collect();
    unique_top_install_candidate(&all).map(|candidate| candidate.git_url.clone())
}

pub(crate) fn resolve_skill_sources_from_snapshot(
    conn: &Connection,
    requests: &[ResolveSkillRequest],
    named_sources: &HashMap<String, String>,
    preferred_repos: &HashSet<String>,
) -> Result<HashMap<String, String>> {
    let mut normalized_names = Vec::new();
    let mut seen_names = HashSet::new();
    for request in requests {
        if named_sources.contains_key(&request.normalized_name) {
            continue;
        }
        if seen_names.insert(request.normalized_name.clone()) {
            normalized_names.push(request.normalized_name.clone());
        }
    }

    let candidates = load_exact_source_candidates(conn, &normalized_names)?;
    let mut resolved = HashMap::new();

    for request in requests {
        if let Some(url) = named_sources.get(&request.normalized_name) {
            resolved.insert(request.original_name.clone(), url.clone());
            continue;
        }

        if let Some(url) = choose_source_candidate(
            candidates.get(&request.normalized_name).map(Vec::as_slice),
            preferred_repos,
        ) {
            resolved.insert(request.original_name.clone(), url);
        }
    }

    Ok(resolved)
}

pub(crate) fn unresolved_normalized_names(
    requests: &[ResolveSkillRequest],
    resolved: &HashMap<String, String>,
    named_sources: &HashMap<String, String>,
) -> Vec<String> {
    let mut unresolved = Vec::new();
    let mut seen = HashSet::new();

    for request in requests {
        if named_sources.contains_key(&request.normalized_name)
            || resolved.contains_key(&request.original_name)
        {
            continue;
        }

        if seen.insert(request.normalized_name.clone()) {
            unresolved.push(request.normalized_name.clone());
        }
    }

    unresolved
}

pub(crate) async fn seed_resolution_names(names: &[String]) {
    let semaphore = std::sync::Arc::new(tokio::sync::Semaphore::new(3));
    let mut tasks = tokio::task::JoinSet::new();

    for name in names {
        let permit = match semaphore.clone().acquire_owned().await {
            Ok(permit) => permit,
            Err(err) => {
                warn!(
                    target: "marketplace_snapshot",
                    error = %err,
                    "failed to acquire source-resolution permit"
                );
                break;
            }
        };
        let name = name.clone();
        tasks.spawn(async move {
            let _permit = permit;
            let result = seed_search_results(&name, RESOLVE_SOURCE_REMOTE_LIMIT).await;
            (name, result)
        });
    }

    while let Some(result) = tasks.join_next().await {
        match result {
            Ok((name, Err(err))) => {
                warn!(
                    target: "marketplace_snapshot",
                    name = %name,
                    error = %err,
                    "failed to seed source resolution"
                );
            }
            Ok((_name, Ok(()))) => {}
            Err(err) => {
                warn!(target: "marketplace_snapshot", error = %err, "source-resolution task failed");
            }
        }
    }
}

pub(crate) fn remote_source_candidates(
    market_result: MarketplaceResult,
    normalized_name: &str,
) -> Vec<ResolveSourceCandidate> {
    let mut seen_sources = HashSet::new();
    let mut candidates = Vec::new();

    for skill in market_result.skills {
        let Some(skill_name) = normalize_skill_name(&skill.name) else {
            continue;
        };
        if skill_name != normalized_name || skill.git_url.trim().is_empty() {
            continue;
        }

        let Some(source) = skill
            .source
            .as_deref()
            .and_then(normalize_source)
            .or_else(|| {
                extract_github_source_from_url(&skill.git_url)
                    .and_then(|value| normalize_source(&value))
            })
        else {
            continue;
        };

        if seen_sources.insert(source.clone()) {
            candidates.push(ResolveSourceCandidate {
                source,
                git_url: skill.git_url,
                installs: skill.stars,
            });
        }
    }

    candidates.sort_by(|left, right| {
        right
            .installs
            .cmp(&left.installs)
            .then_with(|| left.source.cmp(&right.source))
    });
    candidates
}

pub(crate) async fn resolve_skill_sources_remote_fallback(
    requests: &[ResolveSkillRequest],
    named_sources: &HashMap<String, String>,
    preferred_repos: &HashSet<String>,
) -> Result<HashMap<String, String>> {
    let mut resolved = HashMap::new();
    for request in requests {
        if let Some(url) = named_sources.get(&request.normalized_name) {
            resolved.insert(request.original_name.clone(), url.clone());
        }
    }

    let unresolved = unresolved_normalized_names(requests, &resolved, named_sources);
    if unresolved.is_empty() {
        return Ok(resolved);
    }

    let semaphore = std::sync::Arc::new(tokio::sync::Semaphore::new(3));
    let mut tasks = tokio::task::JoinSet::new();

    for name in unresolved {
        let permit =
            semaphore.clone().acquire_owned().await.map_err(|err| {
                anyhow!("Failed to acquire remote source-resolution permit: {err}")
            })?;
        tasks.spawn(async move {
            let _permit = permit;
            let result = remote::search_skills_sh(&name, RESOLVE_SOURCE_REMOTE_LIMIT).await;
            (name, result)
        });
    }

    let mut candidates_by_name: HashMap<String, Vec<ResolveSourceCandidate>> = HashMap::new();
    while let Some(result) = tasks.join_next().await {
        match result {
            Ok((name, Ok(market_result))) => {
                let candidates = remote_source_candidates(market_result, &name);
                if !candidates.is_empty() {
                    candidates_by_name.insert(name, candidates);
                }
            }
            Ok((name, Err(err))) => {
                warn!(
                    target: "marketplace_snapshot",
                    name = %name,
                    error = %err,
                    "remote source fallback failed"
                );
            }
            Err(err) => {
                warn!(target: "marketplace_snapshot", error = %err, "remote source fallback task join error");
            }
        }
    }

    for request in requests {
        if resolved.contains_key(&request.original_name) {
            continue;
        }

        if let Some(url) = choose_source_candidate(
            candidates_by_name
                .get(&request.normalized_name)
                .map(Vec::as_slice),
            preferred_repos,
        ) {
            resolved.insert(request.original_name.clone(), url);
        }
    }

    Ok(resolved)
}

pub async fn resolve_skill_sources_local_first(
    names: &[String],
    existing_sources: &HashMap<String, String>,
) -> Result<HashMap<String, String>> {
    let requests = normalize_resolve_requests(names);
    if requests.is_empty() {
        return Ok(HashMap::new());
    }

    let named_sources = existing_named_sources(existing_sources);
    let preferred_repos = preferred_source_repos(existing_sources);

    let initial = with_conn(|conn| {
        resolve_skill_sources_from_snapshot(conn, &requests, &named_sources, &preferred_repos)
    });

    let mut resolved = match initial {
        Ok(resolved) => resolved,
        Err(err) => {
            warn!(target: "marketplace_snapshot", error = %err, "source resolution local read failed");
            return resolve_skill_sources_remote_fallback(
                &requests,
                &named_sources,
                &preferred_repos,
            )
            .await;
        }
    };

    let unresolved = unresolved_normalized_names(&requests, &resolved, &named_sources);
    if unresolved.is_empty() {
        return Ok(resolved);
    }

    seed_resolution_names(&unresolved).await;

    match with_conn(|conn| {
        resolve_skill_sources_from_snapshot(conn, &requests, &named_sources, &preferred_repos)
    }) {
        Ok(after_seed) => {
            resolved.extend(after_seed);
            Ok(resolved)
        }
        Err(err) => {
            warn!(
                target: "marketplace_snapshot",
                error = %err,
                "source resolution local re-read failed after seed"
            );
            resolve_skill_sources_remote_fallback(&requests, &named_sources, &preferred_repos).await
        }
    }
}
