//! Cursor local import.
//!
//! Lives beside [`super::cursor`] so that file does not have to change.

use crate::local_import::upsert_oauth_subscription;
use crate::oauth::token_refresh;
use crate::subscription::Subscription;
use crate::tool_paths::cursor_state_db_path;
use crate::vscdb;
use crate::{UsageError, UsageResult};

pub(crate) async fn import_from_local() -> UsageResult<Subscription> {
    let db_path = cursor_state_db_path()
        .ok_or_else(|| UsageError::Other("无法解析 Cursor 数据目录".into()))?;
    let session = vscdb::read_cursor_oauth_session(&db_path)?
        .ok_or_else(|| UsageError::Other("Cursor 未登录（缺少 cursorAuth/accessToken）".into()))?;
    let display_name = session
        .email
        .clone()
        .or_else(|| session.auth_id.clone())
        .unwrap_or_else(|| "Cursor".to_string());
    let expires_at = token_refresh::jwt_exp(&session.access_token);
    upsert_oauth_subscription(
        "cursor",
        display_name,
        session.access_token,
        session.refresh_token,
        expires_at,
        "USD",
        session.auth_id,
    )
    .await
}
