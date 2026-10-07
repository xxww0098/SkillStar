//! Generic ItemTable writes on top of [`crate::vscdb`].
//!
//! Cursor and Antigravity keep their own read/write facades. New IDE stores
//! pass a product `label` so errors are not hardcoded to "Cursor".

#![cfg_attr(not(test), allow(dead_code))]

use std::path::Path;

use crate::UsageResult;

pub(crate) fn upsert_item(db_path: &Path, label: &str, key: &str, value: &str) -> UsageResult<()> {
    crate::vscdb::write_labeled_items(db_path, label, &[(key, value)])
}

pub(crate) fn delete_item(db_path: &Path, label: &str, key: &str) -> UsageResult<()> {
    crate::vscdb::delete_labeled_items(db_path, label, &[key])
}

/// Insert or replace every pair in one SQLite transaction.
pub(crate) fn write_items(db_path: &Path, label: &str, items: &[(&str, &str)]) -> UsageResult<()> {
    crate::vscdb::write_labeled_items(db_path, label, items)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn empty_db() -> (tempfile::TempDir, std::path::PathBuf) {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("state.vscdb");
        let conn = rusqlite::Connection::open(&path).expect("db");
        conn.execute(
            "CREATE TABLE ItemTable (key TEXT PRIMARY KEY, value TEXT)",
            [],
        )
        .expect("table");
        conn.execute(
            "INSERT INTO ItemTable (key, value) VALUES ('unrelated', 'keep')",
            [],
        )
        .expect("sibling");
        drop(conn);
        (dir, path)
    }

    fn item(path: &Path, key: &str) -> Option<String> {
        crate::vscdb::read_item_string(path, key).expect("read")
    }

    #[test]
    fn upsert_delete_and_multi_key_write_preserve_unrelated_rows() {
        let (_dir, path) = empty_db();
        upsert_item(&path, "Windsurf", "auth", "one").expect("upsert");
        assert_eq!(item(&path, "auth").as_deref(), Some("one"));
        assert_eq!(item(&path, "unrelated").as_deref(), Some("keep"));

        write_items(&path, "Windsurf", &[("auth", "two"), ("extra", "three")]).expect("multi");
        assert_eq!(item(&path, "auth").as_deref(), Some("two"));
        assert_eq!(item(&path, "extra").as_deref(), Some("three"));
        assert_eq!(item(&path, "unrelated").as_deref(), Some("keep"));

        delete_item(&path, "Windsurf", "auth").expect("delete");
        assert_eq!(item(&path, "auth"), None);
        assert_eq!(item(&path, "extra").as_deref(), Some("three"));
        assert_eq!(item(&path, "unrelated").as_deref(), Some("keep"));
    }

    #[test]
    fn multi_key_write_rolls_back_when_a_later_key_fails() {
        let (_dir, path) = empty_db();
        let conn = rusqlite::Connection::open(&path).expect("db");
        conn.execute(
            "CREATE TRIGGER fail_bad BEFORE INSERT ON ItemTable
             WHEN NEW.key = 'bad'
             BEGIN
                 SELECT RAISE(ABORT, 'nope');
             END",
            [],
        )
        .expect("trigger");
        drop(conn);

        let error = write_items(&path, "Qoder", &[("fresh", "1"), ("bad", "2")])
            .expect_err("trigger aborts the statement");
        assert!(error.to_string().contains("写入 Qoder bad"), "{error}");
        assert_eq!(item(&path, "fresh"), None, "earlier key must roll back");
        assert_eq!(item(&path, "bad"), None);
        assert_eq!(item(&path, "unrelated").as_deref(), Some("keep"));
    }

    #[test]
    fn missing_database_error_uses_the_caller_label() {
        let dir = tempfile::tempdir().expect("tempdir");
        let missing = dir.path().join("state.vscdb");
        for error in [
            upsert_item(&missing, "Windsurf", "k", "v").expect_err("upsert"),
            delete_item(&missing, "Windsurf", "k").expect_err("delete"),
            write_items(&missing, "Windsurf", &[("k", "v")]).expect_err("write"),
        ] {
            let text = error.to_string();
            assert!(text.contains("未找到 Windsurf state.vscdb"), "{text}");
            assert!(!text.contains("Cursor"), "{text}");
        }
    }
}
