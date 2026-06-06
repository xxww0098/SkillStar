//! Read plaintext keys from VS Code `state.vscdb` (ItemTable).

use std::path::Path;

use crate::{UsageError, UsageResult};

pub fn read_item_string(db_path: &Path, key: &str) -> UsageResult<Option<String>> {
    if !db_path.exists() {
        return Ok(None);
    }
    let conn = rusqlite::Connection::open(db_path)
        .map_err(|e| UsageError::Io(std::io::Error::other(format!("打开 state.vscdb：{e}"))))?;
    let value: Result<String, rusqlite::Error> =
        conn.query_row("SELECT value FROM ItemTable WHERE key = ?1", [key], |row| {
            row.get(0)
        });
    match value {
        Ok(v) => Ok(Some(v)),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(e) => Err(UsageError::Other(format!("读取 state.vscdb：{e}"))),
    }
}
