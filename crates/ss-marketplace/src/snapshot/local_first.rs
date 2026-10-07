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

/// Flatten an error chain into the one-line cause the UI shows instead of its
/// own generic "Marketplace request failed" copy.
pub(crate) fn error_detail(err: &anyhow::Error) -> String {
    format!("{err:#}")
}

/// The local snapshot read failed *and* the remote fallback failed too — both
/// halves matter, so report both.
pub(crate) fn local_and_remote_detail(local: &anyhow::Error, remote: &anyhow::Error) -> String {
    format!("Local snapshot read failed: {local:#}; remote fallback failed: {remote:#}")
}

/// Appended to the `ErrorFallback` cause when the payload that stood in for the
/// unreadable snapshot was itself parsed through a lossy fallback.
pub(crate) const DEGRADED_REMOTE_FALLBACK_NOTE: &str = "the remote fallback payload is itself degraded (upstream parsing failed), \
     so these results are incomplete";

/// The whole `ErrorFallback` answer, built in one place for all seven readers.
///
/// `ErrorFallback` says "this did not come from the snapshot". It does not say
/// "this is missing rows", and the status enum is shared with the healthy read
/// paths, so it cannot carry a second axis. Yet the direct-from-remote path is
/// the one read path that bypasses `marketplace_sync_state` entirely — nothing
/// downstream of it can discover that the payload was degraded. Left as it was,
/// the contract "knowingly lossy data never passes for complete" held on every
/// read path *except* the one that skips the snapshot.
///
/// So the reason rides on `error`, which the UI already renders for this
/// status. Taking the whole [`FetchMeta`] rather than a bare `bool` keeps the
/// call sites from being able to answer the question themselves.
pub(crate) fn error_fallback<T>(
    data: T,
    local_err: &anyhow::Error,
    meta: &FetchMeta,
) -> LocalFirstResult<T> {
    let error = if meta.degraded {
        format!(
            "{}; {DEGRADED_REMOTE_FALLBACK_NOTE}",
            error_detail(local_err)
        )
    } else {
        error_detail(local_err)
    };
    LocalFirstResult {
        data,
        snapshot_status: SnapshotStatus::ErrorFallback,
        snapshot_updated_at: None,
        error: Some(error),
    }
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
                error: None,
            })
        }
        Ok((_, ScopeSeedState::Synced, _, updated_at)) => Ok(LocalFirstResult {
            data: Vec::new(),
            snapshot_status: SnapshotStatus::Miss,
            snapshot_updated_at: updated_at,
            error: None,
        }),
        Ok((_, ScopeSeedState::NeverSynced, _, _)) => {
            if let Err(sync_err) = sync_scope_leaderboard(category).await {
                return Ok(LocalFirstResult {
                    data: Vec::new(),
                    snapshot_status: SnapshotStatus::RemoteError,
                    snapshot_updated_at: None,
                    error: Some(error_detail(&sync_err)),
                });
            }

            let reseeded = with_conn(|conn| {
                let data = load_leaderboard_snapshot(conn, &scope)?;
                let updated_at = scope_updated_at(conn, &scope)?;
                Ok((data, updated_at))
            })?;
            Ok(LocalFirstResult {
                data: apply_installed_state(reseeded.0).await,
                snapshot_status: SnapshotStatus::Seeding,
                snapshot_updated_at: reseeded.1,
                error: None,
            })
        }
        Err(err) => {
            warn!(target: "marketplace_snapshot", error = %err, "leaderboard local read failed");
            match remote::get_skills_sh_leaderboard_with_meta(category, None, None).await {
                Ok((skills, meta)) => Ok(error_fallback(
                    apply_installed_state(skills).await,
                    &err,
                    &meta,
                )),
                Err(remote_err) => Ok(LocalFirstResult {
                    data: Vec::new(),
                    snapshot_status: SnapshotStatus::RemoteError,
                    snapshot_updated_at: None,
                    error: Some(local_and_remote_detail(&err, &remote_err)),
                }),
            }
        }
    }
}

/// The marketplace "all" tab. Reads the whole local skill table, but answers
/// freshness from the `leaderboard_all` scope — the same scope the UI refreshes
/// (`sync_marketplace_scope("leaderboard_all")`) and `schedule_startup_refreshes`
/// retries.
///
/// Judging by rows alone made this the one view outside the freshness
/// contract: any row left behind by a single search seed made the default tab
/// report `Fresh` forever, TTL ignored, degraded fallback rows included, and
/// `Stale` was unreachable — so the auto-refresh, the retry budget and the
/// retry button were all dead code exactly where users spend most of their
/// time. Same states, same rules as hot/trending. See `docs/errors.md`.
pub async fn list_skills_local() -> Result<LocalFirstResult<Vec<Skill>>> {
    let scope = leaderboard_scope("all");
    let local = with_conn(|conn| {
        let (data, rows_updated_at) = load_all_skills_snapshot(conn)?;
        let seed_state = sync_seed_state(conn, &scope)?;
        let fresh = is_scope_fresh_conn(conn, &scope)?;
        // Prefer the scope's own success timestamp; fall back to the newest row
        // stamp for a table seeded by searches before any leaderboard sync.
        let updated_at = scope_updated_at(conn, &scope)?.or(rows_updated_at);
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
            error: None,
        }),
        Ok((_, ScopeSeedState::Synced, _, updated_at)) => Ok(LocalFirstResult {
            data: Vec::new(),
            snapshot_status: SnapshotStatus::Miss,
            snapshot_updated_at: updated_at,
            error: None,
        }),
        Ok((_, ScopeSeedState::NeverSynced, _, _)) => {
            if let Err(sync_err) = sync_scope_leaderboard("all").await {
                return Ok(LocalFirstResult {
                    data: Vec::new(),
                    snapshot_status: SnapshotStatus::RemoteError,
                    snapshot_updated_at: None,
                    error: Some(error_detail(&sync_err)),
                });
            }

            let reseeded = with_conn(|conn| {
                let (data, _) = load_all_skills_snapshot(conn)?;
                let updated_at = scope_updated_at(conn, &scope)?;
                Ok((data, updated_at))
            })?;
            Ok(LocalFirstResult {
                data: apply_installed_state(reseeded.0).await,
                snapshot_status: SnapshotStatus::Seeding,
                snapshot_updated_at: reseeded.1,
                error: None,
            })
        }
        Err(err) => {
            // A local SQLite failure is not a network failure: reporting
            // `RemoteError` here sent the UI's "check your network or proxy"
            // copy out over a `database is locked`. Same shape as every other
            // reader in this file — try remote, and only then admit both halves
            // failed.
            warn!(target: "marketplace_snapshot", error = %err, "full marketplace local read failed");
            match remote::get_skills_sh_leaderboard_with_meta("all", None, None).await {
                Ok((skills, meta)) => Ok(error_fallback(
                    apply_installed_state(skills).await,
                    &err,
                    &meta,
                )),
                Err(remote_err) => Ok(LocalFirstResult {
                    data: Vec::new(),
                    snapshot_status: SnapshotStatus::RemoteError,
                    snapshot_updated_at: None,
                    error: Some(local_and_remote_detail(&err, &remote_err)),
                }),
            }
        }
    }
}

/// Search reads the whole `marketplace_skill` table, so it has no TTL of its
/// own: hits come from whatever filled the table — a leaderboard sync, a
/// per-query seed — and a query the user just ran online is not stale merely
/// because the leaderboard is due for a refresh.
///
/// The one part of the freshness contract that does apply is the degraded rule.
/// The fallback rows a degraded leaderboard write leaves behind are in that same
/// table and come back as search hits, so while *any* leaderboard scope is
/// degraded search reports `Stale`, never `Fresh`: no read path may present
/// knowingly lossy rows as up to date. See
/// `docs/features/marketplace/README.md`.
///
/// All three leaderboard scopes, not just `leaderboard_all`: `hot` and
/// `trending` write through the same `upsert_skill_in_tx` into the same table.
/// Asking only about `all` meant a user who had opened the hot tab and never
/// the default one got hot's fallback rows back from search labelled `Fresh`.
pub async fn search_local(query: &str, limit: Option<u32>) -> Result<LocalFirstResult<Vec<Skill>>> {
    let limit = limit.unwrap_or(50).clamp(1, 200);
    let local = with_conn(|conn| {
        let (data, updated_at) = load_search_snapshot(conn, query, limit)?;
        let has_any = any_skill_rows(conn)?;
        let degraded = shared_skill_table_is_degraded(conn)?;
        Ok((data, updated_at, has_any, degraded))
    });

    match local {
        Ok((data, updated_at, _, degraded)) if !data.is_empty() => Ok(LocalFirstResult {
            data: apply_installed_state(data).await,
            snapshot_status: if degraded {
                SnapshotStatus::Stale
            } else {
                SnapshotStatus::Fresh
            },
            snapshot_updated_at: updated_at,
            error: None,
        }),
        Ok((_, updated_at, true, _)) => Ok(LocalFirstResult {
            data: Vec::new(),
            snapshot_status: SnapshotStatus::Miss,
            snapshot_updated_at: updated_at,
            error: None,
        }),
        Ok((_, _, false, _)) => {
            if let Err(seed_err) = seed_search_results(query, limit).await {
                return Ok(LocalFirstResult {
                    data: Vec::new(),
                    snapshot_status: SnapshotStatus::RemoteError,
                    snapshot_updated_at: None,
                    error: Some(error_detail(&seed_err)),
                });
            }
            let reseeded = with_conn(|conn| load_search_snapshot(conn, query, limit))?;
            Ok(LocalFirstResult {
                data: apply_installed_state(reseeded.0).await,
                snapshot_status: SnapshotStatus::Seeding,
                snapshot_updated_at: reseeded.1,
                error: None,
            })
        }
        Err(err) => {
            warn!(target: "marketplace_snapshot", error = %err, "search local read failed");
            match remote::search_skills_sh_with_meta(query, limit, None, None).await {
                Ok((result, meta)) => Ok(error_fallback(
                    apply_installed_state(result.skills).await,
                    &err,
                    &meta,
                )),
                Err(remote_err) => Ok(LocalFirstResult {
                    data: Vec::new(),
                    snapshot_status: SnapshotStatus::RemoteError,
                    snapshot_updated_at: None,
                    error: Some(local_and_remote_detail(&err, &remote_err)),
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
            error: None,
        }),
        Ok((_, ScopeSeedState::Synced, _, updated_at)) => Ok(LocalFirstResult {
            data: Vec::new(),
            snapshot_status: SnapshotStatus::Miss,
            snapshot_updated_at: updated_at,
            error: None,
        }),
        Ok((_, ScopeSeedState::NeverSynced, _, _)) => {
            if let Err(sync_err) = sync_scope_publishers().await {
                return Ok(LocalFirstResult {
                    data: Vec::new(),
                    snapshot_status: SnapshotStatus::RemoteError,
                    snapshot_updated_at: None,
                    error: Some(error_detail(&sync_err)),
                });
            }
            let reseeded: (Vec<OfficialPublisher>, Option<String>) = with_conn(|conn| {
                let data = load_publishers_snapshot(conn)?;
                let updated_at = scope_updated_at(conn, scope)?;
                Ok((data, updated_at))
            })?;
            Ok(LocalFirstResult {
                data: reseeded.0,
                snapshot_status: SnapshotStatus::Seeding,
                snapshot_updated_at: reseeded.1,
                error: None,
            })
        }
        Err(err) => {
            warn!(target: "marketplace_snapshot", error = %err, "publishers local read failed");
            match remote::get_official_publishers_with_meta(None, None).await {
                Ok((publishers, meta)) => Ok(error_fallback(publishers, &err, &meta)),
                Err(remote_err) => Ok(LocalFirstResult {
                    data: Vec::new(),
                    snapshot_status: SnapshotStatus::RemoteError,
                    snapshot_updated_at: None,
                    error: Some(local_and_remote_detail(&err, &remote_err)),
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
            error: None,
        }),
        Ok((_, ScopeSeedState::Synced, _, updated_at)) => Ok(LocalFirstResult {
            data: Vec::new(),
            snapshot_status: SnapshotStatus::Miss,
            snapshot_updated_at: updated_at,
            error: None,
        }),
        Ok((_, ScopeSeedState::NeverSynced, _, _)) => {
            if let Err(sync_err) = sync_scope_publisher_repos(&publisher_name).await {
                return Ok(LocalFirstResult {
                    data: Vec::new(),
                    snapshot_status: SnapshotStatus::RemoteError,
                    snapshot_updated_at: None,
                    error: Some(error_detail(&sync_err)),
                });
            }
            let reseeded: (Vec<PublisherRepo>, Option<String>) = with_conn(|conn| {
                let data = load_publisher_repos_snapshot(conn, &publisher_name)?;
                let updated_at = scope_updated_at(conn, &scope)?;
                Ok((data, updated_at))
            })?;
            Ok(LocalFirstResult {
                data: reseeded.0,
                snapshot_status: SnapshotStatus::Seeding,
                snapshot_updated_at: reseeded.1,
                error: None,
            })
        }
        Err(err) => {
            warn!(target: "marketplace_snapshot", error = %err, "publisher repos local read failed");
            match remote::get_publisher_repos_with_meta(&publisher_name, None, None).await {
                Ok((repos, meta)) => Ok(error_fallback(repos, &err, &meta)),
                Err(remote_err) => Ok(LocalFirstResult {
                    data: Vec::new(),
                    snapshot_status: SnapshotStatus::RemoteError,
                    snapshot_updated_at: None,
                    error: Some(local_and_remote_detail(&err, &remote_err)),
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
            error: None,
        }),
        Ok((_, ScopeSeedState::Synced, _, updated_at)) => Ok(LocalFirstResult {
            data: Vec::new(),
            snapshot_status: SnapshotStatus::Miss,
            snapshot_updated_at: updated_at,
            error: None,
        }),
        Ok((_, ScopeSeedState::NeverSynced, _, _)) => {
            if let Err(sync_err) = sync_scope_repo_skills(&source).await {
                return Ok(LocalFirstResult {
                    data: Vec::new(),
                    snapshot_status: SnapshotStatus::RemoteError,
                    snapshot_updated_at: None,
                    error: Some(error_detail(&sync_err)),
                });
            }
            let reseeded: (Vec<Skill>, Option<String>) = with_conn(|conn| {
                let data = load_repo_skills_snapshot(conn, &source)?;
                let updated_at = scope_updated_at(conn, &scope)?;
                Ok((data, updated_at))
            })?;
            Ok(LocalFirstResult {
                data: apply_installed_state(reseeded.0).await,
                snapshot_status: SnapshotStatus::Seeding,
                snapshot_updated_at: reseeded.1,
                error: None,
            })
        }
        Err(err) => {
            warn!(target: "marketplace_snapshot", error = %err, "repo skills local read failed");
            let (publisher_name, repo_name) = split_source(&source);
            match remote::get_publisher_repo_skills_with_meta(
                &publisher_name,
                &repo_name,
                None,
                None,
            )
            .await
            {
                Ok((skills, meta)) => {
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
                    Ok(error_fallback(
                        apply_installed_state(data).await,
                        &err,
                        &meta,
                    ))
                }
                Err(remote_err) => Ok(LocalFirstResult {
                    data: Vec::new(),
                    snapshot_status: SnapshotStatus::RemoteError,
                    snapshot_updated_at: None,
                    error: Some(local_and_remote_detail(&err, &remote_err)),
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
            error: None,
        }),
        Ok((None, ScopeSeedState::Synced, _, updated_at)) => Ok(LocalFirstResult {
            data: empty_details(),
            snapshot_status: SnapshotStatus::Miss,
            snapshot_updated_at: updated_at,
            error: None,
        }),
        Ok((None, ScopeSeedState::NeverSynced, _, _)) => {
            if let Err(sync_err) = sync_scope_skill_detail(&source, &name).await {
                return Ok(LocalFirstResult {
                    data: empty_details(),
                    snapshot_status: SnapshotStatus::RemoteError,
                    snapshot_updated_at: None,
                    error: Some(error_detail(&sync_err)),
                });
            }
            let reseeded = with_conn(|conn| {
                let data = load_skill_detail_snapshot(conn, &skill_key)?;
                let updated_at = scope_updated_at(conn, &scope)?;
                Ok((data, updated_at))
            })?;
            Ok(LocalFirstResult {
                data: reseeded.0.unwrap_or_else(empty_details),
                snapshot_status: SnapshotStatus::Seeding,
                snapshot_updated_at: reseeded.1,
                error: None,
            })
        }
        Err(err) => {
            warn!(target: "marketplace_snapshot", error = %err, "detail local read failed");
            match remote::fetch_marketplace_skill_details_with_meta(&source, &name, None, None)
                .await
            {
                Ok((details, meta)) => Ok(error_fallback(details, &err, &meta)),
                Err(remote_err) => Ok(LocalFirstResult {
                    data: empty_details(),
                    snapshot_status: SnapshotStatus::RemoteError,
                    snapshot_updated_at: None,
                    error: Some(local_and_remote_detail(&err, &remote_err)),
                }),
            }
        }
    }
}
