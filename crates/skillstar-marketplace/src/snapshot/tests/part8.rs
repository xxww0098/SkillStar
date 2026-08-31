//! A publisher's repo card and the repo drill-down must agree on the skill
//! count. Two scopes write the same repo: `publisher_repos:<publisher>` from
//! the `/official` aggregate (complete, but it keeps skills a repo no longer
//! ships) and `repo_skills:<source>` from the repo page (current). The card
//! derives its count from the rows, and the aggregate only seeds rows for a
//! repo the page has never been scraped for. See `docs/errors.md`.

use super::*;
use crate::remote::{PublisherRepo, PublisherRepoSkill};
use crate::snapshot::*;

fn official_repo(skills: &[&str]) -> PublisherRepo {
    PublisherRepo {
        repo: "repo".into(),
        source: "acme/repo".into(),
        skill_count: skills.len() as u32,
        installs_label: "11".into(),
        installs: 11,
        url: "https://skills.sh/acme/repo".into(),
        skills: skills
            .iter()
            .map(|name| PublisherRepoSkill {
                name: (*name).into(),
                installs: 1,
            })
            .collect(),
    }
}

fn repo_skill_rows(conn: &Connection) -> i64 {
    conn.query_row(
        "SELECT COUNT(*) FROM marketplace_repo_skill WHERE source = 'acme/repo'",
        [],
        |row| row.get(0),
    )
    .expect("count repo skills")
}

#[test]
fn the_repo_page_owns_a_repo_once_scraped_and_the_card_counts_its_rows() {
    with_temp_data_root(|_| {
        let synced_at = now_rfc3339();
        let conn = create_connection().expect("open marketplace db");
        conn.execute(
            "INSERT INTO marketplace_repo (source, publisher_name, repo_name, skill_count, installs, installs_label, url, updated_at)
             VALUES ('acme/repo', 'acme', 'repo', 11, 11, '11', 'https://skills.sh/acme/repo', ?1),
                    ('acme/bare', 'acme', 'bare', 4, 1, '1', 'https://skills.sh/acme/bare', ?1)",
            rusqlite::params![synced_at],
        )
        .expect("insert repos");

        // Never scraped: the aggregate seeds the rows.
        let tx = conn.unchecked_transaction().expect("open tx");
        seed_repo_skills_from_official_in_tx(&tx, &official_repo(&["a", "b", "c"]), &synced_at)
            .expect("seed");
        tx.commit().expect("commit");
        assert_eq!(repo_skill_rows(&conn), 3);

        // The repo page is scraped and finds a single current skill.
        let tx = conn.unchecked_transaction().expect("open tx");
        tx.execute(
            "DELETE FROM marketplace_repo_skill WHERE source = 'acme/repo'",
            [],
        )
        .expect("clear");
        let key = upsert_skill_identity_in_tx(&tx, "acme/repo", "a", 1, &synced_at)
            .expect("upsert")
            .expect("key");
        tx.execute(
            "INSERT INTO marketplace_repo_skill (source, skill_key, installs, rank, updated_at)
             VALUES ('acme/repo', ?1, 1, 1, ?2)",
            rusqlite::params![key, synced_at],
        )
        .expect("insert scraped row");
        mark_scope_success_with_meta_in_tx(
            &tx,
            "repo_skills:acme/repo",
            &crate::remote::FetchMeta {
                payload_sha256: "page".into(),
                source_host: "https://skills.sh/".into(),
                etag: None,
                degraded: false,
            },
            false,
        )
        .expect("record scrape");
        tx.commit().expect("commit");

        // A later aggregate rewrite must not resurrect the stale three.
        let tx = conn.unchecked_transaction().expect("open tx");
        seed_repo_skills_from_official_in_tx(&tx, &official_repo(&["a", "b", "c"]), &synced_at)
            .expect("seed again");
        tx.commit().expect("commit");
        assert_eq!(repo_skill_rows(&conn), 1);

        // The card reports the rows, not the stale column; a repo with no rows
        // keeps the stored count.
        let repos = load_publisher_repos_snapshot(&conn, "acme").expect("load repos");
        let count = |name: &str| {
            repos
                .iter()
                .find(|repo| repo.repo == name)
                .map(|repo| repo.skill_count)
                .expect("repo present")
        };
        assert_eq!(count("repo"), 1);
        assert_eq!(count("bare"), 4);
    });
}
