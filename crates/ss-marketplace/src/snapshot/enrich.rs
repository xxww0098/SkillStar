//! Backfill missing skill descriptions from skills.sh detail pages.
//!
//! The leaderboard and search payloads carry no descriptions (probed
//! 2026-10-06: SSR skill objects expose only
//! `source/skillId/name/installs/weeklyInstalls/isOfficial`), so a freshly
//! synced marketplace renders every card with the no-description placeholder.
//! The one-line description exists only on each skill's detail page, as JSON-LD
//! `SoftwareApplication.description` — see
//! [`fetch_skill_description`](crate::remote::fetch_skill_description).
//!
//! Fetching ~600 detail pages in one go is too much traffic for one sync, so
//! the backfill is incremental: every leaderboard sync kicks off one background
//! batch of the highest-ranked skills still missing a description. Progress is
//! watermarked by `marketplace_skill.description_sync_at` (v15) — a skill whose
//! page has no description keeps its empty string and is only re-attempted
//! after [`ENRICH_RETRY_DAYS`], instead of occupying a batch slot on every
//! round.

use super::*;

/// Skills fetched per backfill round. One round rides along each leaderboard
/// sync (TTL 6h), so the visible top of the grid fills in over a few rounds.
pub(crate) const ENRICH_BATCH: usize = 24;
/// Concurrent detail-page fetches inside one leaderboard round.
pub(crate) const ENRICH_CONCURRENCY: usize = 6;
/// The repo the user just opened. Wider than the background round so the
/// first screen (about one viewport of cards) lands in a single wave.
pub(crate) const REPO_ENRICH_CONCURRENCY: usize = 12;
/// Only skills at or above this best listing rank are enriched. Below it the
/// grid rarely scrolls and the per-skill page traffic stops paying for itself.
pub(crate) const ENRICH_RANK_CAP: i64 = 300;
/// How long before a descriptionless skill is re-attempted. skills.sh can
/// gain a description later; a week bounds the retry traffic.
pub(crate) const ENRICH_RETRY_DAYS: i64 = 7;

/// One backfill round: pick the batch, fetch concurrently, write what came
/// back. Failures only warn — the leaderboard sync that spawned this round has
/// already succeeded, and descriptions are an enhancement, not a contract.
pub(crate) async fn backfill_missing_descriptions() {
    let candidates = match with_conn(pending_description_candidates) {
        Ok(candidates) => candidates,
        Err(err) => {
            warn!(target: "marketplace_snapshot", error = %err, "description backfill candidate read failed");
            return;
        }
    };
    write_round(candidates, ENRICH_CONCURRENCY, false).await;
}

/// Same round, scoped to one repo's skills.
///
/// Repo drill-down rows have no listing rank at all — a skill that only ever
/// appeared in `marketplace_repo_skill` is never selected by
/// [`pending_description_candidates`], so a publisher detail page rendered
/// every card as "no description" no matter how many leaderboard rounds ran.
/// Ordered by the on-screen order (rank, then installs), so the rows the user
/// is looking at fill first.
// Test builds reach it only through [`spawn_repo_description_backfill`], which
// never spawns; the reference is `cfg(not(test))`.
#[cfg_attr(test, allow(dead_code))]
pub(crate) async fn backfill_repo_descriptions(source: &str) {
    let candidates = match with_conn(|conn| pending_repo_description_candidates(conn, source)) {
        Ok(candidates) => candidates,
        Err(err) => {
            warn!(target: "marketplace_snapshot", source, error = %err, "repo description backfill candidate read failed");
            return;
        }
    };
    // Report each write so the open page can repaint before the slowest page.
    write_round(candidates, REPO_ENRICH_CONCURRENCY, true).await;
}

async fn write_round(
    candidates: Vec<(String, String, String)>,
    concurrency: usize,
    report_repo: bool,
) {
    if candidates.is_empty() {
        return;
    }
    let permits = Arc::new(tokio::sync::Semaphore::new(concurrency));
    let mut set = tokio::task::JoinSet::new();
    for (skill_key, source, name) in candidates {
        let permits = permits.clone();
        set.spawn(async move {
            // Bound concurrent detail-page fetches; the batch is small enough
            // to spawn whole and gate here rather than stream chunks.
            let _permit = permits.acquire_owned().await;
            let description = remote::fetch_skill_description(&source, &name)
                .await
                .unwrap_or_else(|err| {
                    debug!(target: "marketplace_snapshot", skill_key = %skill_key, error = %err, "description fetch failed");
                    None
                });
            (skill_key, source, description)
        });
    }

    while let Some(joined) = set.join_next().await {
        let (skill_key, source, description) = match joined {
            Ok(result) => result,
            Err(err) => {
                warn!(target: "marketplace_snapshot", error = %err, "description backfill task panicked");
                continue;
            }
        };
        // One write per page. The repo list reads between fetches; holding
        // every row for the slowest page is what made the grid wait.
        if let Err(err) = with_conn(|conn| write_descriptions(conn, &[(skill_key, description)])) {
            warn!(target: "marketplace_snapshot", error = %err, "description backfill write failed");
            continue;
        }
        if report_repo {
            note_repo_description_write(&source);
        }
    }
}

/// In-flight repo round. The open page polls `progress` and repaints as
/// pages land, instead of sleeping until the whole batch finishes.
struct RepoDescriptionRound {
    progress: u64,
    running: bool,
}

fn repo_rounds() -> &'static std::sync::Mutex<HashMap<String, RepoDescriptionRound>> {
    static ROUNDS: std::sync::LazyLock<std::sync::Mutex<HashMap<String, RepoDescriptionRound>>> =
        std::sync::LazyLock::new(|| std::sync::Mutex::new(HashMap::new()));
    &ROUNDS
}

fn lock_rounds() -> std::sync::MutexGuard<'static, HashMap<String, RepoDescriptionRound>> {
    repo_rounds()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// Start a round for `source`. A second call while it is running does nothing,
/// so a re-read cannot launch a duplicate batch.
pub(crate) fn begin_repo_description_round(source: &str) -> bool {
    let mut rounds = lock_rounds();
    if rounds.get(source).is_some_and(|round| round.running) {
        return false;
    }
    rounds.insert(
        source.to_string(),
        RepoDescriptionRound {
            progress: 0,
            running: true,
        },
    );
    true
}

pub(crate) fn finish_repo_description_round(source: &str) {
    if let Some(round) = lock_rounds().get_mut(source) {
        round.running = false;
    }
}

pub(crate) fn note_repo_description_write(source: &str) {
    if let Some(round) = lock_rounds().get_mut(source) {
        round.progress = round.progress.saturating_add(1);
    }
}

pub(crate) fn round_progress(source: &str) -> u64 {
    lock_rounds()
        .get(source)
        .map(|round| round.progress)
        .unwrap_or(0)
}

pub(crate) fn round_running(source: &str) -> bool {
    lock_rounds().get(source).is_some_and(|round| round.running)
}

/// Highest-ranked skills still missing a description, oldest attempts first.
/// Rank is the best rank across all listings (`all` / `hot` / `trending`), so
/// a skill high on any tab gets priority over its `all`-only position.
fn pending_description_candidates(conn: &Connection) -> Result<Vec<(String, String, String)>> {
    let retry_cutoff = (Utc::now() - Duration::days(ENRICH_RETRY_DAYS)).to_rfc3339();
    let mut stmt = conn
        .prepare(
            "SELECT s.skill_key, s.source, s.name,
                    COALESCE(MIN(l.rank), 1000000) AS best_rank
             FROM marketplace_skill s
             LEFT JOIN marketplace_listing l ON l.skill_key = s.skill_key
             WHERE s.description = ''
               AND (s.description_sync_at IS NULL OR s.description_sync_at < ?1)
             GROUP BY s.skill_key
             HAVING best_rank <= ?2
             ORDER BY best_rank ASC, s.installs DESC
             LIMIT ?3",
        )
        .context("Failed to prepare description backfill candidate query")?;

    let rows = stmt
        .query_map(
            params![retry_cutoff, ENRICH_RANK_CAP, ENRICH_BATCH as i64],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                ))
            },
        )
        .context("Failed to read description backfill candidates")?;

    let mut candidates = Vec::new();
    for row in rows {
        candidates.push(row.context("Failed to decode description backfill candidate")?);
    }
    Ok(candidates)
}

/// Skills of one repo still missing a description, in the order the repo page
/// shows them (ranked rows first, then by installs).
fn pending_repo_description_candidates(
    conn: &Connection,
    source: &str,
) -> Result<Vec<(String, String, String)>> {
    let retry_cutoff = (Utc::now() - Duration::days(ENRICH_RETRY_DAYS)).to_rfc3339();
    let mut stmt = conn
        .prepare(
            "SELECT s.skill_key, s.source, s.name
             FROM marketplace_repo_skill rs
             JOIN marketplace_skill s ON s.skill_key = rs.skill_key
             WHERE rs.source = ?1
               AND s.description = ''
               AND (s.description_sync_at IS NULL OR s.description_sync_at < ?2)
             ORDER BY
                CASE WHEN rs.rank IS NULL THEN 1 ELSE 0 END ASC,
                rs.rank ASC,
                rs.installs DESC,
                s.name ASC
             LIMIT ?3",
        )
        .context("Failed to prepare repo description backfill candidate query")?;

    let rows = stmt
        .query_map(params![source, retry_cutoff, ENRICH_BATCH as i64], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
            ))
        })
        .context("Failed to read repo description backfill candidates")?;

    let mut candidates = Vec::new();
    for row in rows {
        candidates.push(row.context("Failed to decode repo description backfill candidate")?);
    }
    Ok(candidates)
}

/// Persist one round's answers: the description where one came back, and the
/// attempt watermark for every candidate either way — the watermark is what
/// keeps a permanently descriptionless skill from monopolizing its batch slot.
fn write_descriptions(conn: &Connection, results: &[(String, Option<String>)]) -> Result<()> {
    let tx = conn
        .unchecked_transaction()
        .context("Failed to start description backfill transaction")?;
    let now = now_rfc3339();
    let mut refreshed = 0usize;

    for (skill_key, description) in results {
        let description = description
            .as_deref()
            .map(str::trim)
            .filter(|text| !text.is_empty());
        if let Some(text) = description {
            tx.execute(
                "UPDATE marketplace_skill
                 SET description = ?1, description_sync_at = ?2
                 WHERE skill_key = ?3 AND description = ''",
                params![text, now, skill_key],
            )
            .with_context(|| format!("Failed to write backfilled description for {skill_key}"))?;
            refresh_fts_entry_in_tx(&tx, skill_key)?;
            refreshed += 1;
        } else {
            tx.execute(
                "UPDATE marketplace_skill
                 SET description_sync_at = ?1
                 WHERE skill_key = ?2",
                params![now, skill_key],
            )
            .with_context(|| {
                format!("Failed to write description attempt watermark for {skill_key}")
            })?;
        }
    }

    tx.commit()
        .context("Failed to commit description backfill transaction")?;
    debug!(
        target: "marketplace_snapshot",
        attempted = results.len(),
        refreshed,
        "description backfill round done"
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::snapshot::tests::{open_raw_conn, with_temp_data_root};

    fn seed(path: &std::path::Path) -> Connection {
        let conn = open_raw_conn(path);
        migrate_schema(&conn).expect("migrate");
        conn.execute_batch(
            "INSERT INTO marketplace_skill (skill_key, source, name, description, installs)
             VALUES
                ('a/top',     'a/top',     'top',     '', 500),
                ('a/mid',     'a/mid',     'mid',     '', 300),
                ('a/low',     'a/low',     'low',     '', 100),
                ('a/described','a/described','described','already', 900),
                ('a/recent',  'a/recent',  'recent',  '', 800);
             INSERT INTO marketplace_listing (listing_type, skill_key, rank, updated_at)
             VALUES ('leaderboard_all', 'a/top', 1, '2026-01-01'),
                    ('leaderboard_all', 'a/mid', 2, '2026-01-01'),
                    ('leaderboard_all', 'a/described', 3, '2026-01-01'),
                    ('leaderboard_all', 'a/recent', 4, '2026-01-01');
             UPDATE marketplace_skill SET description_sync_at = '2999-01-01' WHERE skill_key = 'a/recent';",
        )
        .expect("seed");
        conn
    }

    #[test]
    fn a_repo_round_does_not_start_twice() {
        let source = "round-gate-test";
        finish_repo_description_round(source);
        assert!(begin_repo_description_round(source));
        assert!(!begin_repo_description_round(source));
        assert!(round_running(source));
        note_repo_description_write(source);
        note_repo_description_write(source);
        assert_eq!(round_progress(source), 2);
        finish_repo_description_round(source);
        assert!(!round_running(source));
        assert!(begin_repo_description_round(source));
        finish_repo_description_round(source);
    }

    #[test]
    fn candidates_skip_described_recent_attempts_and_unranked() {
        with_temp_data_root(|temp_root| {
            let conn = seed(&temp_root.join("marketplace.db"));
            let candidates = pending_description_candidates(&conn).expect("candidates");
            // top before mid; `described` has text, `recent` was just attempted,
            // `low` never appears in a listing.
            assert_eq!(
                candidates,
                vec![
                    ("a/top".to_string(), "a/top".to_string(), "top".to_string()),
                    ("a/mid".to_string(), "a/mid".to_string(), "mid".to_string()),
                ]
            );
        });
    }

    #[test]
    fn repo_candidates_reach_skills_without_a_listing() {
        with_temp_data_root(|temp_root| {
            let conn = seed(&temp_root.join("marketplace.db"));
            conn.execute_batch(
                "INSERT INTO marketplace_skill (skill_key, source, name, description, installs)
                 VALUES
                    ('a/repo/one', 'a/repo', 'one', '', 10),
                    ('a/repo/two', 'a/repo', 'two', '', 5),
                    ('a/repo/described', 'a/repo', 'described', 'text', 3);
                 INSERT INTO marketplace_repo_skill (source, skill_key, installs, rank, updated_at)
                 VALUES ('a/repo', 'a/repo/one', 10, 1, '2026-01-01'),
                        ('a/repo', 'a/repo/two', 5, 2, '2026-01-01'),
                        ('a/repo', 'a/repo/described', 3, 3, '2026-01-01'),
                        ('a/repo', 'a/top', 900, NULL, '2026-01-01');",
            )
            .expect("seed repo rows");

            let candidates =
                pending_repo_description_candidates(&conn, "a/repo").expect("repo candidates");
            // Ranked rows first; `described` has text; the unranked `a/top`
            // belongs to the repo but sorts after the ranked ones.
            assert_eq!(
                candidates,
                vec![
                    (
                        "a/repo/one".to_string(),
                        "a/repo".to_string(),
                        "one".to_string()
                    ),
                    (
                        "a/repo/two".to_string(),
                        "a/repo".to_string(),
                        "two".to_string()
                    ),
                    ("a/top".to_string(), "a/top".to_string(), "top".to_string()),
                ]
            );
        });
    }

    #[test]
    fn write_descriptions_fills_text_and_watermarks_misses() {
        with_temp_data_root(|temp_root| {
            let conn = seed(&temp_root.join("marketplace.db"));
            write_descriptions(
                &conn,
                &[
                    ("a/top".to_string(), Some("Top description".to_string())),
                    ("a/mid".to_string(), Some("   ".to_string())),
                ],
            )
            .expect("write descriptions");

            let top: (String, bool) = conn
                .query_row(
                    "SELECT description, description_sync_at IS NOT NULL
                     FROM marketplace_skill WHERE skill_key = 'a/top'",
                    [],
                    |row| Ok((row.get(0)?, row.get(1)?)),
                )
                .unwrap();
            assert_eq!(top, ("Top description".to_string(), true));

            // Whitespace-only is a miss: text stays empty, watermark still set.
            let mid: (String, bool) = conn
                .query_row(
                    "SELECT description, description_sync_at IS NOT NULL
                     FROM marketplace_skill WHERE skill_key = 'a/mid'",
                    [],
                    |row| Ok((row.get(0)?, row.get(1)?)),
                )
                .unwrap();
            assert_eq!(mid, (String::new(), true));

            // FTS row carries the backfilled text.
            let fts: String = conn
                .query_row(
                    "SELECT description FROM marketplace_skill_fts WHERE skill_key = 'a/top'",
                    [],
                    |row| row.get(0),
                )
                .unwrap();
            assert_eq!(fts, "Top description");
        });
    }

    /// Real-network smoke test for one backfill round. Never runs in CI
    /// (`#[ignore]`); run it by hand with
    /// `cargo test -p ss-marketplace --lib backfill_round -- --ignored --nocapture`.
    #[allow(clippy::await_holding_lock)]
    #[tokio::test]
    #[ignore = "hits skills.sh"]
    async fn backfill_round_fills_real_descriptions() {
        use crate::snapshot::{InstalledSkillsFuture, SnapshotRuntimeConfig, configure_runtime};

        let _guard = crate::snapshot::tests::test_env_lock()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let temp = tempfile::tempdir().expect("create temp dir");
        let db_path = temp.path().join("marketplace.db");
        configure_runtime(SnapshotRuntimeConfig::new(
            db_path.clone(),
            temp.path().to_path_buf(),
            HashSet::new,
            || -> InstalledSkillsFuture { Box::pin(async { Ok(Vec::new()) }) },
        ));

        let conn = open_raw_conn(&db_path);
        migrate_schema(&conn).expect("migrate");
        // Real skills.sh entries whose detail pages exist today.
        conn.execute_batch(
            "INSERT INTO marketplace_skill (skill_key, source, name, description, installs)
             VALUES
                ('vercel-labs/skills/find-skills', 'vercel-labs/skills', 'find-skills', '', 3700000),
                ('mattpocock/skills/grill-me', 'mattpocock/skills', 'grill-me', '', 1287000);
             INSERT INTO marketplace_listing (listing_type, skill_key, rank, updated_at)
             VALUES ('leaderboard_all', 'vercel-labs/skills/find-skills', 1, '2026-01-01'),
                    ('leaderboard_all', 'mattpocock/skills/grill-me', 2, '2026-01-01');",
        )
        .expect("seed");

        backfill_missing_descriptions().await;

        let filled: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM marketplace_skill WHERE description <> ''",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert!(
            filled > 0,
            "expected at least one real description, got {filled}"
        );
    }
}
