//! JSON replacement for files outside the usage subscription lock.
//!
//! `storage::write_json_unlocked` already persists through
//! `skillstar_core::infra::fs_ops::atomic_write` (temp sibling, fsync, rename).
//! This is that writer.

#![cfg_attr(not(test), allow(dead_code))]

use std::path::Path;

use serde::Serialize;

use crate::UsageResult;

pub(crate) fn write<T: Serialize>(path: &Path, value: &T) -> UsageResult<()> {
    crate::storage::write_json_unlocked(path, value)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn write_replaces_json_without_leaving_a_temp_file() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("nested").join("state.json");
        write(&path, &serde_json::json!({"ok": true, "n": 1})).expect("write");
        let text = std::fs::read_to_string(&path).expect("read");
        let parsed: serde_json::Value = serde_json::from_str(&text).expect("json");
        assert_eq!(parsed["ok"], true);
        assert_eq!(parsed["n"], 1);

        write(&path, &serde_json::json!({"ok": false})).expect("rewrite");
        let rewritten = std::fs::read_to_string(&path).expect("reread");
        assert!(rewritten.contains("false"), "{rewritten}");
        assert!(!rewritten.contains("\"n\""), "{rewritten}");

        let leftovers: Vec<_> = std::fs::read_dir(path.parent().expect("parent"))
            .expect("read_dir")
            .filter_map(|entry| entry.ok())
            .filter(|entry| entry.file_name().to_string_lossy().contains(".tmp"))
            .map(|entry| entry.path())
            .collect();
        assert!(
            leftovers.is_empty(),
            "temp files must not survive: {leftovers:?}"
        );
    }
}
