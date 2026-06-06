use super::*;

pub(crate) async fn apply_installed_state(mut skills: Vec<Skill>) -> Vec<Skill> {
    let installed_skills = match load_installed_skills().await {
        Ok(skills) => skills,
        Err(err) => {
            warn!(target: "marketplace_snapshot", error = %err, "failed to load installed snapshot");
            return skills;
        }
    };

    let mut by_key = HashMap::new();
    let mut by_name = HashMap::new();
    for skill in installed_skills {
        let state = InstalledSkillState {
            installed: true,
            update_available: skill.update_available,
            skill_type: skill.skill_type.clone(),
            tree_hash: skill.tree_hash.clone(),
            agent_links: skill.agent_links.clone(),
        };

        if let Some(skill_key) = skill
            .source
            .as_deref()
            .and_then(|source| build_skill_key(source, &skill.name))
        {
            by_key.insert(skill_key, state.clone());
        }
        by_name.insert(skill.name.to_ascii_lowercase(), state);
    }

    for skill in &mut skills {
        let skill_key = skill
            .source
            .as_deref()
            .and_then(|source| build_skill_key(source, &skill.name));
        let state = skill_key
            .as_deref()
            .and_then(|key| by_key.get(key))
            .or_else(|| by_name.get(&skill.name.to_ascii_lowercase()));

        if let Some(state) = state {
            skill.installed = state.installed;
            skill.update_available = state.update_available;
            skill.skill_type = state.skill_type.clone();
            skill.tree_hash = state.tree_hash.clone();
            skill.agent_links = state.agent_links.clone();
        }
    }

    skills
}

pub(crate) fn empty_details() -> MarketplaceSkillDetails {
    MarketplaceSkillDetails {
        summary: None,
        readme: None,
        weekly_installs: None,
        github_stars: None,
        first_seen: None,
        security_audits: Vec::new(),
    }
}

pub async fn get_leaderboard_local(category: &str) -> Result<LocalFirstResult<Vec<Skill>>> {
    let scope = leaderboard_scope(category);
    let local = with_conn(|conn| {
        let data = load_leaderboard_snapshot(conn, &scope)?;
        let seed_state = sync_seed_state(conn, &scope)?;
        let fresh = is_scope_fresh_conn(conn, &scope)?;
        let updated_at = scope_updated_at(conn, &scope)?;
        Ok((data, seed_state, fresh, updated_at))
    });

    match local {
        Ok((data, _, fresh, updated_at)) if !data.is_empty() => {
            let data = apply_installed_state(data).await;
            Ok(LocalFirstResult {
                data,
                snapshot_status: if fresh {
                    SnapshotStatus::Fresh
                } else {
                    SnapshotStatus::Stale
                },
                snapshot_updated_at: updated_at,
            })
        }
        Ok((_, ScopeSeedState::Synced, _, updated_at)) => Ok(LocalFirstResult {
            data: Vec::new(),
            snapshot_status: SnapshotStatus::Miss,
            snapshot_updated_at: updated_at,
        }),
        Ok((_, ScopeSeedState::NeverSynced, _, _)) => {
            if sync_scope_leaderboard(category).await.is_ok() {
                let reseeded = with_conn(|conn| {
                    let data = load_leaderboard_snapshot(conn, &scope)?;
                    let updated_at = scope_updated_at(conn, &scope)?;
                    Ok((data, updated_at))
                })?;
                return Ok(LocalFirstResult {
                    data: apply_installed_state(reseeded.0).await,
                    snapshot_status: SnapshotStatus::Seeding,
                    snapshot_updated_at: reseeded.1,
                });
            }

            Ok(LocalFirstResult {
                data: Vec::new(),
                snapshot_status: SnapshotStatus::RemoteError,
                snapshot_updated_at: None,
            })
        }
        Err(err) => {
            warn!(target: "marketplace_snapshot", error = %err, "leaderboard local read failed");
            match remote::get_skills_sh_leaderboard(category).await {
                Ok(skills) => Ok(LocalFirstResult {
                    data: apply_installed_state(skills).await,
                    snapshot_status: SnapshotStatus::ErrorFallback,
                    snapshot_updated_at: None,
                }),
                Err(_remote_err) => Ok(LocalFirstResult {
                    data: Vec::new(),
                    snapshot_status: SnapshotStatus::RemoteError,
                    snapshot_updated_at: None,
                }),
            }
        }
    }
}

pub async fn list_skills_local() -> Result<LocalFirstResult<Vec<Skill>>> {
    let local = with_conn(load_all_skills_snapshot);

    match local {
        Ok((data, updated_at)) if !data.is_empty() => Ok(LocalFirstResult {
            data: apply_installed_state(data).await,
            snapshot_status: SnapshotStatus::Fresh,
            snapshot_updated_at: updated_at,
        }),
        Ok((_, updated_at)) => Ok(LocalFirstResult {
            data: Vec::new(),
            snapshot_status: SnapshotStatus::Miss,
            snapshot_updated_at: updated_at,
        }),
        Err(err) => {
            warn!(target: "marketplace_snapshot", error = %err, "full marketplace local read failed");
            Ok(LocalFirstResult {
                data: Vec::new(),
                snapshot_status: SnapshotStatus::RemoteError,
                snapshot_updated_at: None,
            })
        }
    }
}

pub async fn search_local(query: &str, limit: Option<u32>) -> Result<LocalFirstResult<Vec<Skill>>> {
    let limit = limit.unwrap_or(50).clamp(1, 200);
    let local = with_conn(|conn| {
        let (data, updated_at) = load_search_snapshot(conn, query, limit)?;
        let has_any = any_skill_rows(conn)?;
        Ok((data, updated_at, has_any))
    });

    match local {
        Ok((data, updated_at, _)) if !data.is_empty() => Ok(LocalFirstResult {
            data: apply_installed_state(data).await,
            snapshot_status: SnapshotStatus::Fresh,
            snapshot_updated_at: updated_at,
        }),
        Ok((_, updated_at, true)) => Ok(LocalFirstResult {
            data: Vec::new(),
            snapshot_status: SnapshotStatus::Miss,
            snapshot_updated_at: updated_at,
        }),
        Ok((_, _, false)) => {
            if seed_search_results(query, limit).await.is_ok() {
                let reseeded = with_conn(|conn| load_search_snapshot(conn, query, limit))?;
                return Ok(LocalFirstResult {
                    data: apply_installed_state(reseeded.0).await,
                    snapshot_status: SnapshotStatus::Seeding,
                    snapshot_updated_at: reseeded.1,
                });
            }
            Ok(LocalFirstResult {
                data: Vec::new(),
                snapshot_status: SnapshotStatus::RemoteError,
                snapshot_updated_at: None,
            })
        }
        Err(err) => {
            warn!(target: "marketplace_snapshot", error = %err, "search local read failed");
            match remote::search_skills_sh(query, limit).await {
                Ok(result) => Ok(LocalFirstResult {
                    data: apply_installed_state(result.skills).await,
                    snapshot_status: SnapshotStatus::ErrorFallback,
                    snapshot_updated_at: None,
                }),
                Err(_) => Ok(LocalFirstResult {
                    data: Vec::new(),
                    snapshot_status: SnapshotStatus::RemoteError,
                    snapshot_updated_at: None,
                }),
            }
        }
    }
}

pub async fn get_publishers_local() -> Result<LocalFirstResult<Vec<OfficialPublisher>>> {
    let scope = "official_publishers";
    let local: Result<(Vec<OfficialPublisher>, ScopeSeedState, bool, Option<String>)> =
        with_conn(|conn| {
            let data = load_publishers_snapshot(conn)?;
            let seed_state = sync_seed_state(conn, scope)?;
            let fresh = is_scope_fresh_conn(conn, scope)?;
            let updated_at = scope_updated_at(conn, scope)?;
            Ok((data, seed_state, fresh, updated_at))
        });

    match local {
        Ok((data, _, fresh, updated_at)) if !data.is_empty() => Ok(LocalFirstResult {
            data,
            snapshot_status: if fresh {
                SnapshotStatus::Fresh
            } else {
                SnapshotStatus::Stale
            },
            snapshot_updated_at: updated_at,
        }),
        Ok((_, ScopeSeedState::Synced, _, updated_at)) => Ok(LocalFirstResult {
            data: Vec::new(),
            snapshot_status: SnapshotStatus::Miss,
            snapshot_updated_at: updated_at,
        }),
        Ok((_, ScopeSeedState::NeverSynced, _, _)) => {
            if sync_scope_publishers().await.is_ok() {
                let reseeded: (Vec<OfficialPublisher>, Option<String>) = with_conn(|conn| {
                    let data = load_publishers_snapshot(conn)?;
                    let updated_at = scope_updated_at(conn, scope)?;
                    Ok((data, updated_at))
                })?;
                return Ok(LocalFirstResult {
                    data: reseeded.0,
                    snapshot_status: SnapshotStatus::Seeding,
                    snapshot_updated_at: reseeded.1,
                });
            }
            Ok(LocalFirstResult {
                data: Vec::new(),
                snapshot_status: SnapshotStatus::RemoteError,
                snapshot_updated_at: None,
            })
        }
        Err(err) => {
            warn!(target: "marketplace_snapshot", error = %err, "publishers local read failed");
            match remote::get_official_publishers().await {
                Ok(publishers) => Ok(LocalFirstResult {
                    data: publishers,
                    snapshot_status: SnapshotStatus::ErrorFallback,
                    snapshot_updated_at: None,
                }),
                Err(_) => Ok(LocalFirstResult {
                    data: Vec::new(),
                    snapshot_status: SnapshotStatus::RemoteError,
                    snapshot_updated_at: None,
                }),
            }
        }
    }
}

pub async fn get_publisher_repos_local(
    publisher_name: &str,
) -> Result<LocalFirstResult<Vec<PublisherRepo>>> {
    let publisher_name = publisher_name.trim().to_ascii_lowercase();
    let scope = format!("publisher_repos:{publisher_name}");
    let local: Result<(Vec<PublisherRepo>, ScopeSeedState, bool, Option<String>)> =
        with_conn(|conn| {
            let data = load_publisher_repos_snapshot(conn, &publisher_name)?;
            let seed_state = sync_seed_state(conn, &scope)?;
            let fresh = is_scope_fresh_conn(conn, &scope)?;
            let updated_at = scope_updated_at(conn, &scope)?;
            Ok((data, seed_state, fresh, updated_at))
        });

    match local {
        Ok((data, _, fresh, updated_at)) if !data.is_empty() => Ok(LocalFirstResult {
            data,
            snapshot_status: if fresh {
                SnapshotStatus::Fresh
            } else {
                SnapshotStatus::Stale
            },
            snapshot_updated_at: updated_at,
        }),
        Ok((_, ScopeSeedState::Synced, _, updated_at)) => Ok(LocalFirstResult {
            data: Vec::new(),
            snapshot_status: SnapshotStatus::Miss,
            snapshot_updated_at: updated_at,
        }),
        Ok((_, ScopeSeedState::NeverSynced, _, _)) => {
            if sync_scope_publisher_repos(&publisher_name).await.is_ok() {
                let reseeded: (Vec<PublisherRepo>, Option<String>) = with_conn(|conn| {
                    let data = load_publisher_repos_snapshot(conn, &publisher_name)?;
                    let updated_at = scope_updated_at(conn, &scope)?;
                    Ok((data, updated_at))
                })?;
                return Ok(LocalFirstResult {
                    data: reseeded.0,
                    snapshot_status: SnapshotStatus::Seeding,
                    snapshot_updated_at: reseeded.1,
                });
            }

            Ok(LocalFirstResult {
                data: Vec::new(),
                snapshot_status: SnapshotStatus::RemoteError,
                snapshot_updated_at: None,
            })
        }
        Err(err) => {
            warn!(target: "marketplace_snapshot", error = %err, "publisher repos local read failed");
            match remote::get_publisher_repos(&publisher_name).await {
                Ok(repos) => Ok(LocalFirstResult {
                    data: repos,
                    snapshot_status: SnapshotStatus::ErrorFallback,
                    snapshot_updated_at: None,
                }),
                Err(_) => Ok(LocalFirstResult {
                    data: Vec::new(),
                    snapshot_status: SnapshotStatus::RemoteError,
                    snapshot_updated_at: None,
                }),
            }
        }
    }
}

pub async fn get_repo_skills_local(source: &str) -> Result<LocalFirstResult<Vec<Skill>>> {
    let source = normalize_source(source).ok_or_else(|| anyhow!("Invalid repo source"))?;
    let scope = format!("repo_skills:{source}");
    let local: Result<(Vec<Skill>, ScopeSeedState, bool, Option<String>)> = with_conn(|conn| {
        let data = load_repo_skills_snapshot(conn, &source)?;
        let seed_state = sync_seed_state(conn, &scope)?;
        let fresh = is_scope_fresh_conn(conn, &scope)?;
        let updated_at = scope_updated_at(conn, &scope)?;
        Ok((data, seed_state, fresh, updated_at))
    });

    match local {
        Ok((data, _, fresh, updated_at)) if !data.is_empty() => Ok(LocalFirstResult {
            data: apply_installed_state(data).await,
            snapshot_status: if fresh {
                SnapshotStatus::Fresh
            } else {
                SnapshotStatus::Stale
            },
            snapshot_updated_at: updated_at,
        }),
        Ok((_, ScopeSeedState::Synced, _, updated_at)) => Ok(LocalFirstResult {
            data: Vec::new(),
            snapshot_status: SnapshotStatus::Miss,
            snapshot_updated_at: updated_at,
        }),
        Ok((_, ScopeSeedState::NeverSynced, _, _)) => {
            if sync_scope_repo_skills(&source).await.is_ok() {
                let reseeded: (Vec<Skill>, Option<String>) = with_conn(|conn| {
                    let data = load_repo_skills_snapshot(conn, &source)?;
                    let updated_at = scope_updated_at(conn, &scope)?;
                    Ok((data, updated_at))
                })?;
                return Ok(LocalFirstResult {
                    data: apply_installed_state(reseeded.0).await,
                    snapshot_status: SnapshotStatus::Seeding,
                    snapshot_updated_at: reseeded.1,
                });
            }

            Ok(LocalFirstResult {
                data: Vec::new(),
                snapshot_status: SnapshotStatus::RemoteError,
                snapshot_updated_at: None,
            })
        }
        Err(err) => {
            warn!(target: "marketplace_snapshot", error = %err, "repo skills local read failed");
            let (publisher_name, repo_name) = split_source(&source);
            match remote::get_publisher_repo_skills(&publisher_name, &repo_name).await {
                Ok(skills) => {
                    let data = skills
                        .into_iter()
                        .map(|skill| {
                            skill_from_snapshot_row(SnapshotSkillRow {
                                source: source.clone(),
                                name: skill.name,
                                git_url: format!("https://github.com/{source}"),
                                author: Some(source.clone()),
                                description: String::new(),
                                installs: skill.installs,
                                last_updated: Some(now_rfc3339()),
                                rank: None,
                            })
                        })
                        .collect();
                    Ok(LocalFirstResult {
                        data: apply_installed_state(data).await,
                        snapshot_status: SnapshotStatus::ErrorFallback,
                        snapshot_updated_at: None,
                    })
                }
                Err(_) => Ok(LocalFirstResult {
                    data: Vec::new(),
                    snapshot_status: SnapshotStatus::RemoteError,
                    snapshot_updated_at: None,
                }),
            }
        }
    }
}

pub async fn get_skill_detail_local(
    source: &str,
    name: &str,
) -> Result<LocalFirstResult<MarketplaceSkillDetails>> {
    let source = normalize_source(source).ok_or_else(|| anyhow!("Invalid skill source"))?;
    let name = normalize_skill_name(name).ok_or_else(|| anyhow!("Invalid skill name"))?;
    let scope =
        skill_detail_scope(&source, &name).ok_or_else(|| anyhow!("Invalid detail scope"))?;
    let skill_key = build_skill_key(&source, &name).expect("normalized skill detail key");

    let local = with_conn(|conn| {
        let data = load_skill_detail_snapshot(conn, &skill_key)?;
        let seed_state = sync_seed_state(conn, &scope)?;
        let fresh = is_scope_fresh_conn(conn, &scope)?;
        let updated_at = scope_updated_at(conn, &scope)?;
        Ok((data, seed_state, fresh, updated_at))
    });

    match local {
        Ok((Some(data), _, fresh, updated_at)) => Ok(LocalFirstResult {
            data,
            snapshot_status: if fresh {
                SnapshotStatus::Fresh
            } else {
                SnapshotStatus::Stale
            },
            snapshot_updated_at: updated_at,
        }),
        Ok((None, ScopeSeedState::Synced, _, updated_at)) => Ok(LocalFirstResult {
            data: empty_details(),
            snapshot_status: SnapshotStatus::Miss,
            snapshot_updated_at: updated_at,
        }),
        Ok((None, ScopeSeedState::NeverSynced, _, _)) => {
            if sync_scope_skill_detail(&source, &name).await.is_ok() {
                let reseeded = with_conn(|conn| {
                    let data = load_skill_detail_snapshot(conn, &skill_key)?;
                    let updated_at = scope_updated_at(conn, &scope)?;
                    Ok((data, updated_at))
                })?;
                return Ok(LocalFirstResult {
                    data: reseeded.0.unwrap_or_else(empty_details),
                    snapshot_status: SnapshotStatus::Seeding,
                    snapshot_updated_at: reseeded.1,
                });
            }

            Ok(LocalFirstResult {
                data: empty_details(),
                snapshot_status: SnapshotStatus::RemoteError,
                snapshot_updated_at: None,
            })
        }
        Err(err) => {
            warn!(target: "marketplace_snapshot", error = %err, "detail local read failed");
            match remote::fetch_marketplace_skill_details(&source, &name).await {
                Ok(details) => Ok(LocalFirstResult {
                    data: details,
                    snapshot_status: SnapshotStatus::ErrorFallback,
                    snapshot_updated_at: None,
                }),
                Err(_) => Ok(LocalFirstResult {
                    data: empty_details(),
                    snapshot_status: SnapshotStatus::RemoteError,
                    snapshot_updated_at: None,
                }),
            }
        }
    }
}

pub async fn ai_search_local(
    keywords: &[String],
    limit: Option<u32>,
) -> Result<LocalFirstResult<AiKeywordSearchResult>> {
    async fn load_ai_search_snapshot(
        keywords: &[String],
        limit: u32,
    ) -> Result<(Vec<Skill>, HashMap<String, Vec<String>>, Option<String>)> {
        let mut skill_map: HashMap<String, Skill> = HashMap::new();
        let mut keyword_skill_map: HashMap<String, Vec<String>> = HashMap::new();
        let mut latest_updated_at: Option<String> = None;

        for keyword in keywords {
            let local = with_conn(|conn| load_search_snapshot(conn, keyword, limit))?;
            let mut names = Vec::new();
            for skill in local.0 {
                let key = skill
                    .source
                    .as_deref()
                    .and_then(|source| build_skill_key(source, &skill.name))
                    .unwrap_or_else(|| skill.name.to_ascii_lowercase());

                names.push(skill.name.clone());
                let entry = skill_map.entry(key).or_insert_with(|| skill.clone());
                if skill.stars > entry.stars {
                    *entry = skill;
                }
            }
            if !names.is_empty() {
                keyword_skill_map.insert(keyword.clone(), names);
            }
            if latest_updated_at.as_ref().is_none_or(|current| {
                local
                    .1
                    .as_ref()
                    .is_some_and(|candidate| candidate > current)
            }) {
                latest_updated_at = local.1;
            }
        }

        let mut skills: Vec<Skill> = skill_map.into_values().collect();
        skills.sort_by(|left, right| {
            right
                .stars
                .cmp(&left.stars)
                .then_with(|| left.name.cmp(&right.name))
        });
        for (index, skill) in skills.iter_mut().enumerate() {
            skill.rank = Some((index + 1) as u32);
        }

        Ok::<(Vec<Skill>, HashMap<String, Vec<String>>, Option<String>), anyhow::Error>((
            skills,
            keyword_skill_map,
            latest_updated_at,
        ))
    }

    let keywords = normalize_keywords(keywords);
    let limit = limit.unwrap_or(50).clamp(1, 200);
    let mut snapshot_status = SnapshotStatus::Fresh;
    let mut loaded = load_ai_search_snapshot(&keywords, limit).await?;
    let snapshot_rows = with_conn(skill_row_count)?;

    let should_seed = (loaded.0.is_empty() && !with_conn(any_skill_rows)?)
        || (!keywords.is_empty()
            && loaded.0.len() < AI_SEARCH_REMOTE_SEED_MIN_HITS
            && snapshot_rows < AI_SEARCH_LOW_COVERAGE_ROWS);
    if should_seed {
        for keyword in &keywords {
            seed_search_results(keyword, limit).await?;
        }
        loaded = load_ai_search_snapshot(&keywords, limit).await?;
        snapshot_status = SnapshotStatus::Seeding;
    }

    let skills = apply_installed_state(loaded.0).await;
    Ok(LocalFirstResult {
        data: AiKeywordSearchResult {
            total_count: skills.len() as u32,
            skills,
            keyword_skill_map: loaded.1,
        },
        snapshot_status: if keywords.is_empty() {
            SnapshotStatus::Miss
        } else {
            snapshot_status
        },
        snapshot_updated_at: loaded.2,
    })
}

pub(crate) fn normalize_keywords(keywords: &[String]) -> Vec<String> {
    keywords
        .iter()
        .map(|keyword| keyword.trim())
        .filter(|keyword| !keyword.is_empty())
        .map(ToOwned::to_owned)
        .collect()
}
