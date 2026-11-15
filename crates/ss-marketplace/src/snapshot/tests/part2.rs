use super::*;
use crate::snapshot::*;

#[test]
fn fts_rowid_mirrors_skill_rowid_and_refresh_is_idempotent() {
    with_temp_data_root(|_| {
        let conn = create_connection().expect("create marketplace connection");
        let tx = conn.unchecked_transaction().expect("start tx");
        let synced_at = now_rfc3339();

        let skill_key = upsert_skill_identity_in_tx(&tx, "acme/skills", "widget", 5, &synced_at)
            .expect("upsert skill")
            .expect("skill key");

        // Refreshing repeatedly must not accumulate duplicate FTS rows.
        crate::snapshot::refresh_fts_entry_in_tx(&tx, &skill_key).expect("refresh fts again");
        crate::snapshot::refresh_fts_entry_in_tx(&tx, &skill_key).expect("refresh fts thrice");

        let base_rowid: i64 = tx
            .query_row(
                "SELECT rowid FROM marketplace_skill WHERE skill_key = ?1",
                [skill_key.as_str()],
                |row| row.get(0),
            )
            .expect("base rowid");
        let (fts_count, fts_rowid): (i64, i64) = tx
            .query_row(
                "SELECT COUNT(*), COALESCE(MIN(rowid), -1) FROM marketplace_skill_fts WHERE skill_key = ?1",
                [skill_key.as_str()],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .expect("fts stats");

        assert_eq!(fts_count, 1, "exactly one FTS row per skill");
        assert_eq!(
            fts_rowid, base_rowid,
            "FTS rowid mirrors marketplace_skill.rowid"
        );

        tx.commit().expect("commit");
    });
}

#[test]
fn migrate_v8_to_v9_realigns_misaligned_fts_rows() {
    with_temp_data_root(|_| {
        let conn = create_connection().expect("create marketplace connection");
        let synced_at = now_rfc3339();

        // Seed a skill, then simulate the pre-v9 on-disk state: an FTS row
        // inserted with an auto-assigned rowid that does NOT match the base
        // skill rowid (how the old skill_key-keyed code left every row).
        let skill_key = {
            let tx = conn.unchecked_transaction().expect("tx");
            let key = upsert_skill_identity_in_tx(&tx, "acme/skills", "gadget", 9, &synced_at)
                .expect("upsert")
                .expect("key");
            tx.execute("DELETE FROM marketplace_skill_fts", [])
                .expect("clear fts");
            tx.execute(
                "INSERT INTO marketplace_skill_fts (rowid, skill_key, name, description, summary, publisher_name, repo_name)
                 VALUES (999999, ?1, 'gadget', '', '', 'acme', 'skills')",
                [key.as_str()],
            )
            .expect("insert misaligned fts row");
            tx.commit().expect("commit seed");
            key
        };

        let base_rowid: i64 = conn
            .query_row(
                "SELECT rowid FROM marketplace_skill WHERE skill_key = ?1",
                [skill_key.as_str()],
                |row| row.get(0),
            )
            .expect("base rowid");
        assert_ne!(base_rowid, 999_999, "precondition: FTS row is misaligned");

        crate::snapshot::migrate_v8_to_v9(&conn).expect("realign fts rowids");

        let (count, rowid): (i64, i64) = conn
            .query_row(
                "SELECT COUNT(*), COALESCE(MIN(rowid), -1) FROM marketplace_skill_fts WHERE skill_key = ?1",
                [skill_key.as_str()],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .expect("fts stats after migration");
        assert_eq!(count, 1, "no duplicate FTS rows after realign");
        assert_eq!(rowid, base_rowid, "FTS rowid realigned to base rowid");
    });
}
