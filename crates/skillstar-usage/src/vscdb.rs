//! Read plaintext keys from VS Code `state.vscdb` (ItemTable).

use std::path::Path;

use crate::{UsageError, UsageResult};

pub const CURSOR_ACCESS_TOKEN_KEY: &str = "cursorAuth/accessToken";
pub const CURSOR_REFRESH_TOKEN_KEY: &str = "cursorAuth/refreshToken";
pub const CURSOR_EMAIL_KEY: &str = "cursorAuth/cachedEmail";
pub const CURSOR_AUTH_ID_KEY: &str = "cursorAuth/authId";
pub const CURSOR_MIRROR_ACCESS_TOKEN_KEY: &str = "cursor.accessToken";
pub const CURSOR_MIRROR_EMAIL_KEY: &str = "cursor.email";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CursorOAuthSession {
    pub access_token: String,
    pub refresh_token: Option<String>,
    pub email: Option<String>,
    pub auth_id: Option<String>,
}

pub fn read_item_string(db_path: &Path, key: &str) -> UsageResult<Option<String>> {
    if !db_path.exists() {
        return Ok(None);
    }
    let conn = rusqlite::Connection::open(db_path).map_err(|error| {
        UsageError::Io(std::io::Error::other(format!("打开 state.vscdb：{error}")))
    })?;
    match conn.query_row("SELECT value FROM ItemTable WHERE key = ?1", [key], |row| {
        row.get::<_, String>(0)
    }) {
        Ok(value) => Ok(Some(value)),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(error) => Err(UsageError::Other(format!("读取 state.vscdb：{error}"))),
    }
}

/// Read several IDE state keys through one SQLite connection and one query.
///
/// Cursor writes all OAuth fields into one ItemTable; opening a connection
/// for each field created four independent snapshots and four filesystem lock
/// opportunities during a single account refresh. Keeping the query together
/// gives callers one consistent read while preserving the public single-key
/// helper for other providers.
fn read_item_strings(db_path: &Path, keys: &[&str]) -> UsageResult<Vec<Option<String>>> {
    if keys.is_empty() {
        return Ok(Vec::new());
    }
    if !db_path.exists() {
        return Ok(vec![None; keys.len()]);
    }

    let conn = rusqlite::Connection::open(db_path)
        .map_err(|e| UsageError::Io(std::io::Error::other(format!("打开 state.vscdb：{e}"))))?;
    let placeholders = (1..=keys.len())
        .map(|index| format!("?{index}"))
        .collect::<Vec<_>>()
        .join(", ");
    let sql = format!("SELECT key, value FROM ItemTable WHERE key IN ({placeholders})");
    let mut statement = conn
        .prepare(&sql)
        .map_err(|error| UsageError::Other(format!("读取 state.vscdb：{error}")))?;
    let rows = statement
        .query_map(rusqlite::params_from_iter(keys.iter().copied()), |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })
        .map_err(|error| UsageError::Other(format!("读取 state.vscdb：{error}")))?;

    let mut values = std::collections::HashMap::with_capacity(keys.len());
    for row in rows {
        let (key, value) =
            row.map_err(|error| UsageError::Other(format!("读取 state.vscdb：{error}")))?;
        values.insert(key, value);
    }
    Ok(keys.iter().map(|key| values.remove(*key)).collect())
}

fn write_item_strings(db_path: &Path, items: &[(&str, &str)]) -> UsageResult<()> {
    if !db_path.exists() {
        return Err(UsageError::Other(format!(
            "未找到 Cursor state.vscdb：{}。请先启动 Cursor 并完成一次登录",
            db_path.display()
        )));
    }
    let conn = rusqlite::Connection::open(db_path).map_err(|error| {
        UsageError::Io(std::io::Error::other(format!(
            "打开 Cursor state.vscdb：{error}"
        )))
    })?;
    conn.busy_timeout(std::time::Duration::from_secs(5))
        .map_err(|error| {
            UsageError::Other(format!("设置 Cursor state.vscdb 锁等待失败：{error}"))
        })?;
    conn.execute(
        "CREATE TABLE IF NOT EXISTS ItemTable (key TEXT PRIMARY KEY, value TEXT)",
        [],
    )
    .map_err(|error| UsageError::Other(format!("初始化 Cursor ItemTable 失败：{error}")))?;
    let tx = conn
        .unchecked_transaction()
        .map_err(|error| UsageError::Other(format!("开启 Cursor state.vscdb 事务失败：{error}")))?;
    for (key, value) in items {
        tx.execute(
            "INSERT OR REPLACE INTO ItemTable (key, value) VALUES (?1, ?2)",
            (*key, *value),
        )
        .map_err(|error| UsageError::Other(format!("写入 Cursor {key} 失败：{error}")))?;
    }
    tx.commit()
        .map_err(|error| UsageError::Other(format!("提交 Cursor state.vscdb 事务失败：{error}")))?;
    Ok(())
}

pub fn read_cursor_oauth_session(db_path: &Path) -> UsageResult<Option<CursorOAuthSession>> {
    let [access_token, refresh_token, email, auth_id] = read_item_strings(
        db_path,
        &[
            CURSOR_ACCESS_TOKEN_KEY,
            CURSOR_REFRESH_TOKEN_KEY,
            CURSOR_EMAIL_KEY,
            CURSOR_AUTH_ID_KEY,
        ],
    )
    .and_then(|values| {
        values
            .try_into()
            .map_err(|_| UsageError::Other("Cursor OAuth 查询字段数量不一致".to_string()))
    })?;

    let Some(access_token) = access_token.filter(|value| !value.trim().is_empty()) else {
        return Ok(None);
    };
    let refresh_token = refresh_token.filter(|value| !value.trim().is_empty());
    let email = email.filter(|value| !value.trim().is_empty());
    let auth_id = auth_id
        .filter(|value| !value.trim().is_empty())
        .or_else(|| jwt_subject(&access_token));
    Ok(Some(CursorOAuthSession {
        access_token,
        refresh_token,
        email,
        auth_id,
    }))
}

pub fn write_cursor_oauth_session(
    db_path: &Path,
    access_token: &str,
    refresh_token: &str,
    email: Option<&str>,
    auth_id: Option<&str>,
) -> UsageResult<()> {
    if access_token.trim().is_empty() || refresh_token.trim().is_empty() {
        return Err(UsageError::Other(
            "Cursor 账号缺少 access_token 或 refresh_token，切换未生效".into(),
        ));
    }
    let mut items = vec![
        (CURSOR_ACCESS_TOKEN_KEY, access_token),
        (CURSOR_REFRESH_TOKEN_KEY, refresh_token),
        (CURSOR_MIRROR_ACCESS_TOKEN_KEY, access_token),
    ];
    if let Some(email) = email.filter(|value| !value.trim().is_empty()) {
        items.push((CURSOR_EMAIL_KEY, email));
        items.push((CURSOR_MIRROR_EMAIL_KEY, email));
    }
    if let Some(auth_id) = auth_id.filter(|value| !value.trim().is_empty()) {
        items.push((CURSOR_AUTH_ID_KEY, auth_id));
    }
    write_item_strings(db_path, &items)?;

    let actual = read_cursor_oauth_session(db_path)?
        .ok_or_else(|| UsageError::Other("Cursor state.vscdb 回读不到登录态，切换未生效".into()))?;
    if actual.access_token != access_token || actual.refresh_token.as_deref() != Some(refresh_token)
    {
        return Err(UsageError::Other(
            "Cursor state.vscdb 回读校验失败，切换未生效".into(),
        ));
    }
    if let Some(email) = email.filter(|value| !value.trim().is_empty())
        && actual.email.as_deref() != Some(email)
    {
        return Err(UsageError::Other(
            "Cursor state.vscdb 邮箱回读校验失败，切换未生效".into(),
        ));
    }
    Ok(())
}

fn jwt_subject(access_token: &str) -> Option<String> {
    let payload = access_token.split('.').nth(1)?;
    let mut encoded = payload.replace('-', "+").replace('_', "/");
    while !encoded.len().is_multiple_of(4) {
        encoded.push('=');
    }
    let bytes = base64::Engine::decode(&base64::engine::general_purpose::STANDARD, encoded).ok()?;
    serde_json::from_slice::<serde_json::Value>(&bytes)
        .ok()?
        .get("sub")
        .and_then(serde_json::Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .map(str::to_string)
}

const ANTIGRAVITY_OAUTH_KEY: &str = "antigravityUnifiedStateSync.oauthToken";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AntigravityOAuthSession {
    pub access_token: String,
    pub refresh_token: String,
    pub expires_at: Option<i64>,
    pub email: Option<String>,
}

/// Read the refresh token from Antigravity's real IDE credential store.
/// `None` means the IDE has no usable legacy state DB credential.
pub fn read_antigravity_refresh_token(db_path: &Path) -> UsageResult<Option<String>> {
    Ok(read_antigravity_oauth_session(db_path)?.map(|session| session.refresh_token))
}

pub fn read_antigravity_oauth_session(
    db_path: &Path,
) -> UsageResult<Option<AntigravityOAuthSession>> {
    let Some(encoded) = read_item_string(db_path, ANTIGRAVITY_OAUTH_KEY)? else {
        return Ok(None);
    };
    let blob = base64::Engine::decode(&base64::engine::general_purpose::STANDARD, encoded.trim())
        .map_err(|error| {
        UsageError::Other(format!("Antigravity OAuth Base64 解码失败：{error}"))
    })?;
    Ok(
        crate::protobuf_oauth::extract_oauth_token_from_unified_oauth_token(&blob).map(|token| {
            AntigravityOAuthSession {
                access_token: token.access_token,
                refresh_token: token.refresh_token,
                expires_at: token.expires_at,
                email: token.email,
            }
        }),
    )
}

/// Write one OAuth session into the legacy Antigravity IDE state database and
/// verify the value that the IDE will read back before returning success.
pub fn write_antigravity_oauth_token(
    db_path: &Path,
    access_token: &str,
    refresh_token: &str,
    expires_at: i64,
    email: Option<&str>,
) -> UsageResult<()> {
    if !db_path.exists() {
        return Err(UsageError::Other(format!(
            "未找到 Antigravity state.vscdb：{}。请先启动 Antigravity IDE 并完成一次登录",
            db_path.display()
        )));
    }
    if access_token.trim().is_empty() || refresh_token.trim().is_empty() {
        return Err(UsageError::Other(
            "Antigravity 账号缺少 access_token 或 refresh_token，切换未生效".into(),
        ));
    }

    let encoded = base64::Engine::encode(
        &base64::engine::general_purpose::STANDARD,
        crate::protobuf_oauth::create_unified_oauth_token(
            access_token,
            refresh_token,
            expires_at,
            email,
        ),
    );
    let conn = rusqlite::Connection::open(db_path).map_err(|error| {
        UsageError::Io(std::io::Error::other(format!(
            "打开 Antigravity state.vscdb：{error}"
        )))
    })?;
    conn.busy_timeout(std::time::Duration::from_secs(5))
        .map_err(|error| {
            UsageError::Other(format!("设置 Antigravity state.vscdb 锁等待失败：{error}"))
        })?;
    conn.execute(
        "CREATE TABLE IF NOT EXISTS ItemTable (key TEXT PRIMARY KEY, value TEXT)",
        [],
    )
    .map_err(|error| UsageError::Other(format!("初始化 Antigravity ItemTable 失败：{error}")))?;
    let tx = conn.unchecked_transaction().map_err(|error| {
        UsageError::Other(format!("开启 Antigravity state.vscdb 事务失败：{error}"))
    })?;
    tx.execute(
        "INSERT OR REPLACE INTO ItemTable (key, value) VALUES (?1, ?2)",
        (ANTIGRAVITY_OAUTH_KEY, &encoded),
    )
    .map_err(|error| UsageError::Other(format!("写入 Antigravity OAuth 失败：{error}")))?;
    tx.commit()
        .map_err(|error| UsageError::Other(format!("提交 Antigravity OAuth 失败：{error}")))?;

    let actual = read_antigravity_refresh_token(db_path)?;
    if actual.as_deref() != Some(refresh_token) {
        return Err(UsageError::Other(
            "Antigravity state.vscdb 回读校验失败，切换未生效".into(),
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn writes_and_reads_unified_oauth_token() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("state.vscdb");
        let conn = rusqlite::Connection::open(&path).expect("db");
        conn.execute(
            "CREATE TABLE ItemTable (key TEXT PRIMARY KEY, value TEXT)",
            [],
        )
        .expect("table");
        drop(conn);

        write_antigravity_oauth_token(
            &path,
            "access-token",
            "refresh-token",
            1_700_000_000,
            Some("user@example.com"),
        )
        .expect("write");

        assert_eq!(
            read_antigravity_refresh_token(&path)
                .expect("read")
                .as_deref(),
            Some("refresh-token")
        );
        let session = read_antigravity_oauth_session(&path)
            .expect("session read")
            .expect("session");
        assert_eq!(session.access_token, "access-token");
        assert_eq!(session.refresh_token, "refresh-token");
        assert_eq!(session.expires_at, Some(1_700_000_000));
        assert_eq!(session.email.as_deref(), Some("user@example.com"));
    }

    #[test]
    fn refuses_to_create_a_missing_ide_database() {
        let dir = tempfile::tempdir().expect("tempdir");
        let error = write_antigravity_oauth_token(
            &dir.path().join("missing.vscdb"),
            "access",
            "refresh",
            1,
            None,
        )
        .expect_err("missing database must fail closed");
        assert!(error.to_string().contains("未找到 Antigravity state.vscdb"));
    }

    #[test]
    fn writes_cursor_session_and_preserves_unrelated_rows() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("state.vscdb");
        let conn = rusqlite::Connection::open(&path).expect("db");
        conn.execute(
            "CREATE TABLE ItemTable (key TEXT PRIMARY KEY, value TEXT)",
            [],
        )
        .expect("table");
        conn.execute(
            "INSERT INTO ItemTable (key, value) VALUES ('cursor.other', 'keep-me')",
            [],
        )
        .expect("sibling");
        drop(conn);

        write_cursor_oauth_session(
            &path,
            "cursor-access",
            "cursor-refresh",
            Some("cursor@example.com"),
            Some("auth0|cursor-user"),
        )
        .expect("write");

        let session = read_cursor_oauth_session(&path)
            .expect("read")
            .expect("session");
        assert_eq!(session.access_token, "cursor-access");
        assert_eq!(session.refresh_token.as_deref(), Some("cursor-refresh"));
        assert_eq!(session.email.as_deref(), Some("cursor@example.com"));
        assert_eq!(session.auth_id.as_deref(), Some("auth0|cursor-user"));
        assert_eq!(
            read_item_string(&path, "cursor.other")
                .expect("sibling read")
                .as_deref(),
            Some("keep-me")
        );
        assert_eq!(
            read_item_string(&path, CURSOR_MIRROR_ACCESS_TOKEN_KEY)
                .expect("mirror read")
                .as_deref(),
            Some("cursor-access")
        );
    }
}
