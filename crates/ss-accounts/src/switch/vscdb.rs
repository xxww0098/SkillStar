//! VS Code / Electron 系客户端共享的 `state.vscdb`（`ItemTable`）原语。
//!
//! Cursor、Devin Desktop、Antigravity 与 Kiro 都把登录态或缓存行放进
//! `<userData>/User/globalStorage/state.vscdb` 的 `ItemTable(key, value)` 表。
//! 数据库文件必须已由客户端创建，本模块绝不凭空新建空库文件。

use std::path::Path;
use std::time::Duration;

use rusqlite::{Connection, OpenFlags};

use crate::error::{AccountError, AccountResult};

/// 写 sqlite 时等客户端放开文件锁的默认超时。
pub(crate) const DEFAULT_BUSY_TIMEOUT: Duration = Duration::from_secs(5);
/// 只读探活时短忙等待，避免客户端持写锁时阻塞 list/reconcile。
pub(crate) const READ_BUSY_TIMEOUT: Duration = Duration::from_millis(100);

/// 打开已存在的 `state.vscdb`；按需补建 `ItemTable`，但绝不新建库文件。
pub(crate) fn open_existing<F>(
    path: &Path,
    busy_timeout: Duration,
    ensure_table: bool,
    missing: F,
) -> AccountResult<Connection>
where
    F: FnOnce(&Path) -> AccountError,
{
    if !path.is_file() {
        return Err(missing(path));
    }
    let connection = Connection::open(path).map_err(|error| sqlite_error("打开", path, error))?;
    connection
        .busy_timeout(busy_timeout)
        .map_err(|error| sqlite_error("设置忙等待", path, error))?;
    if ensure_table {
        connection
            .execute(
                "CREATE TABLE IF NOT EXISTS ItemTable (key TEXT PRIMARY KEY, value TEXT)",
                [],
            )
            .map_err(|error| sqlite_error("初始化 ItemTable", path, error))?;
    }
    Ok(connection)
}

/// 以严格只读方式打开已存在的 `state.vscdb`（绝不写库或执行 `CREATE TABLE` DDL）。
pub(crate) fn open_read_only<F>(
    path: &Path,
    busy_timeout: Duration,
    missing: F,
) -> AccountResult<Connection>
where
    F: FnOnce(&Path) -> AccountError,
{
    if !path.is_file() {
        return Err(missing(path));
    }
    let flags = OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX;
    let connection = Connection::open_with_flags(path, flags)
        .map_err(|error| sqlite_error("只读打开", path, error))?;
    connection
        .busy_timeout(busy_timeout)
        .map_err(|error| sqlite_error("设置忙等待", path, error))?;
    Ok(connection)
}

/// 读取单条 `ItemTable` 键值。
pub(crate) fn read_value(
    connection: &Connection,
    key: &str,
    client: &str,
) -> AccountResult<Option<String>> {
    match connection.query_row(
        "SELECT value FROM ItemTable WHERE key = ?1",
        [key],
        |row| row.get::<_, String>(0),
    ) {
        Ok(value) => Ok(Some(value)),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(error) => Err(AccountError::Client(format!(
            "reading {client}'s {key}: {error}"
        ))),
    }
}

/// 按给定键顺序批量读取 `ItemTable` 行，用于快照与登录态读取。
pub(crate) fn read_values(
    connection: &Connection,
    keys: &[&str],
    client: &str,
) -> AccountResult<Vec<(String, Option<String>)>> {
    let mut rows = Vec::with_capacity(keys.len());
    for key in keys {
        rows.push(((*key).to_string(), read_value(connection, key, client)?));
    }
    Ok(rows)
}

/// 在既有连接或事务上写入（`Some`）或删除（`None`）一组 `ItemTable` 行。
pub(crate) fn apply_rows(
    connection: &Connection,
    rows: &[(&str, Option<&str>)],
    client: &str,
) -> AccountResult<()> {
    for (key, value) in rows {
        match value {
            Some(value) => put(connection, key, value, client)?,
            None => drop_row(connection, key, client)?,
        }
    }
    Ok(())
}

/// 在单个事务里原子写入或清理一组 `ItemTable` 行。
pub(crate) fn write_rows<F>(
    path: &Path,
    busy_timeout: Duration,
    ensure_table: bool,
    client: &str,
    missing: F,
    rows: &[(&str, Option<&str>)],
) -> AccountResult<()>
where
    F: FnOnce(&Path) -> AccountError,
{
    let mut connection = open_existing(path, busy_timeout, ensure_table, missing)?;
    let transaction = connection
        .transaction()
        .map_err(|error| sqlite_error("开始事务", path, error))?;
    apply_rows(&transaction, rows, client)?;
    transaction
        .commit()
        .map_err(|error| sqlite_error("提交事务", path, error))
}

/// 回滚之前捕获的 [super::ClientSnapshot::Items] 行快照。
pub(crate) fn restore_items(path: &Path, rows: &[(String, Option<String>)]) -> AccountResult<()> {
    if !path.is_file() {
        tracing::warn!(
            target: "accounts",
            path = %path.display(),
            "state.vscdb is gone; nothing to roll the session back into"
        );
        return Ok(());
    }
    let pairs: Vec<(&str, Option<&str>)> = rows
        .iter()
        .map(|(key, value)| (key.as_str(), value.as_deref()))
        .collect();
    write_rows(
        path,
        DEFAULT_BUSY_TIMEOUT,
        true,
        "client",
        |missing| AccountError::Client(format!("missing database at {}", missing.display())),
        &pairs,
    )
}

fn put(connection: &Connection, key: &str, value: &str, client: &str) -> AccountResult<()> {
    connection
        .execute(
            "INSERT OR REPLACE INTO ItemTable (key, value) VALUES (?1, ?2)",
            [key, value],
        )
        .map(|_| ())
        .map_err(|error| AccountError::Client(format!("writing {client}'s {key}: {error}")))
}

fn drop_row(connection: &Connection, key: &str, client: &str) -> AccountResult<()> {
    connection
        .execute("DELETE FROM ItemTable WHERE key = ?1", [key])
        .map(|_| ())
        .map_err(|error| AccountError::Client(format!("clearing {client}'s {key}: {error}")))
}

pub(crate) fn sqlite_error(action: &str, path: &Path, error: rusqlite::Error) -> AccountError {
    AccountError::Client(format!("{action} {}: {error}", path.display()))
}
