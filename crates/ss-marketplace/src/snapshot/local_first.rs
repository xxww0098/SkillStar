use super::*;

/// The installed Skills, indexed by how a marketplace row finds the one it
/// refers to: `(source, folder)` and `(source, called)` first, then folder
/// alone. A row is installed when this answers for it, and the answer is the
/// Skill to act on when the row is uninstalled.
pub(crate) struct InstalledIndex {
    by_key: HashMap<String, InstalledSkillState>,
    by_name: HashMap<String, InstalledSkillState>,
}

impl InstalledIndex {
    fn build(installed: Vec<Skill>) -> Self {
        let mut by_key = HashMap::new();
        let mut by_name = HashMap::new();
        for skill in installed {
            let state = InstalledSkillState {
                folder: skill.name.clone(),
                installed: true,
                update_available: skill.update_available,
                skill_type: skill.skill_type.clone(),
                tree_hash: skill.tree_hash.clone(),
                agent_links: skill.agent_links.clone(),
                called: skill.called.clone(),
            };

            if let Some(source) = skill.source.as_deref() {
                if let Some(skill_key) = build_skill_key(source, &skill.name) {
                    by_key.insert(skill_key, state.clone());
                }
                // A folder the installer renamed to coexist (`<name>-<owner>`)
                // still answers to the upstream name it recorded.
                if let Some(called) = skill.called.as_deref()
                    && let Some(called_key) = build_skill_key(source, called)
                {
                    by_key.insert(called_key, state.clone());
                }
            }
            by_name.insert(skill.name.to_ascii_lowercase(), state);
        }
        Self { by_key, by_name }
    }

    fn get(&self, source: Option<&str>, name: &str) -> Option<&InstalledSkillState> {
        source
            .and_then(|source| build_skill_key(source, name))
            .and_then(|key| self.by_key.get(&key))
            .or_else(|| self.by_name.get(&name.to_ascii_lowercase()))
    }
}

/// `None` when the installed Skills could not be read: rows then keep the
/// state the snapshot holds for them.
async fn load_installed_index() -> Option<InstalledIndex> {
    match load_installed_skills().await {
        Ok(skills) => Some(InstalledIndex::build(skills)),
        Err(err) => {
            warn!(target: "marketplace_snapshot", error = %err, "failed to load installed snapshot");
            None
        }
    }
}

pub(crate) async fn apply_installed_state(mut skills: Vec<Skill>) -> Vec<Skill> {
    let Some(index) = load_installed_index().await else {
        return skills;
    };

    for skill in &mut skills {
        if let Some(state) = index.get(skill.source.as_deref(), &skill.name) {
            skill.installed = state.installed;
            skill.update_available = state.update_available;
            skill.skill_type = state.skill_type.clone();
            skill.tree_hash = state.tree_hash.clone();
            skill.agent_links = state.agent_links.clone();
            if skill.called.is_none() {
                skill.called = state.called.clone();
            }
        }
    }

    skills
}

/// The installed folder behind marketplace row `source`/`name`: the Skill
/// [`apply_installed_state`] marks that row installed with, so acting on the
/// row acts on what makes it installed. The row's own name is not enough: when
/// that name was taken the installer puts the Skill in `<name>-<owner>` and
/// records `name` as `called`, while whoever owns `name` is a different Skill.
/// `None` when no installed Skill answers for the row.
pub async fn installed_folder_for(source: &str, name: &str) -> Option<String> {
    load_installed_index()
        .await?
        .get(Some(source), name)
        .map(|state| state.folder.clone())
}

/// Round-robin interleave skills across repositories while preserving first-seen
/// repository order and intra-repo ranking order so one monorepo cannot fill the
/// entire search result viewport.
pub fn interleave_by_repo(skills: Vec<Skill>) -> Vec<Skill> {
    if skills.len() <= 2 {
        return skills;
    }
    let mut order: Vec<String> = Vec::new();
    let mut buckets: HashMap<String, std::collections::VecDeque<Skill>> = HashMap::new();
    for skill in skills {
        let repo = skill
            .source
            .as_deref()
            .filter(|s| !s.is_empty())
            .map(str::to_string)
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| {
                if skill.git_url.is_empty() {
                    skill.name.clone()
                } else {
                    skill.git_url.clone()
                }
            });
        if !buckets.contains_key(&repo) {
            order.push(repo.clone());
        }
        buckets.entry(repo).or_default().push_back(skill);
    }

    let mut out = Vec::new();
    let mut advanced = true;
    while advanced {
        advanced = false;
        for repo in &order {
            if let Some(queue) = buckets.get_mut(repo)
                && let Some(item) = queue.pop_front()
            {
                out.push(item);
                advanced = true;
            }
        }
    }
    out
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
            data: apply_installed_state(interleave_by_repo(data)).await,
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
                data: apply_installed_state(interleave_by_repo(reseeded.0)).await,
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
    read_repo_skills(source, true, true).await
}

/// Descriptions only. Does not start a round or rescan installed skills.
pub async fn reread_repo_skills_local(source: &str) -> Result<LocalFirstResult<Vec<Skill>>> {
    read_repo_skills(source, false, false).await
}

async fn read_repo_skills(
    source: &str,
    kick_backfill: bool,
    overlay_installed: bool,
) -> Result<LocalFirstResult<Vec<Skill>>> {
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
        Ok((data, _, fresh, updated_at)) if !data.is_empty() => {
            // Repo skills have no leaderboard listing, so the unscoped round
            // can never reach them. One scoped round per read; a round with
            // no candidates is a single local query.
            if kick_backfill && data.iter().any(|skill| skill.description.trim().is_empty()) {
                spawn_repo_description_backfill(source.clone());
            }
            let data = if overlay_installed {
                apply_installed_state(data).await
            } else {
                data
            };
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

const SPARSE_SEARCH_THRESHOLD: usize = 5;
const SPARSE_SEARCH_COOLDOWN_SECS: i64 = 600;

/// Best-effort remote backfill for sparse local search queries (`< 5` local hits
/// and more than 10 minutes since the last seed attempt for `query`).
///
/// Kept separate from [`search_local`] so purely local reads remain offline and
/// instantaneous.
pub async fn maybe_seed_sparse_search(query: &str) -> Result<bool> {
    let trimmed = query.trim();
    if trimmed.chars().count() < 2 {
        return Ok(false);
    }
    let scope = search_seed_scope(trimmed);
    let should_seed = with_conn(|conn| {
        let (hits, _) = load_search_snapshot(conn, trimmed, SPARSE_SEARCH_THRESHOLD as u32)?;
        if hits.len() >= SPARSE_SEARCH_THRESHOLD {
            return Ok(false);
        }
        if let Some(state) = scope_sync_state(conn, &scope)?
            && let Some(attempt) = state.last_attempt_at.as_deref().or(state.last_success_at.as_deref())
            && let Ok(parsed) = chrono::DateTime::parse_from_rfc3339(attempt)
        {
            let age = (Utc::now() - parsed.with_timezone(&Utc)).num_seconds();
            if age < SPARSE_SEARCH_COOLDOWN_SECS {
                return Ok(false);
            }
        }
        Ok(true)
    })?;
    if !should_seed {
        return Ok(false);
    }
    seed_search_results(trimmed, SEARCH_SEED_LIMIT).await?;
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn market_skill(name: &str, source: &str) -> Skill {
        Skill::from_skills_sh(
            name.to_string(),
            String::new(),
            10,
            source.to_string(),
            format!("https://github.com/{source}"),
        )
    }

    #[test]
    fn interleave_by_repo_spreads_monorepo_runs_in_round_robin_order() {
        let input = vec![
            market_skill("a1", "acme/mono"),
            market_skill("a2", "acme/mono"),
            market_skill("a3", "acme/mono"),
            market_skill("b1", "beta/tools"),
            market_skill("c1", "gamma/kit"),
            market_skill("b2", "beta/tools"),
        ];
        let interleaved = interleave_by_repo(input);
        let names: Vec<&str> = interleaved.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(names, vec!["a1", "b1", "c1", "a2", "b2", "a3"]);
    }
}
