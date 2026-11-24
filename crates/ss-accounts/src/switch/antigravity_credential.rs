//! Antigravity 的系统凭据：macOS Keychain 载荷 + `~/.gemini` 里的回退文件。
//!
//! 新版桌面客户端不再只读 state.vscdb：会话放在系统凭据存储里（macOS 上是
//! `security` 的 `gemini/antigravity` 条目），并且在 Keyring 不可用时回退到
//! `~/.gemini/jetski-standalone-oauth-token`。载荷是 go-keyring 的 JSON：
//!
//! ```json
//! { "token": { "access_token": ..., "token_type": "Bearer",
//!              "refresh_token": ..., "expiry": "RFC3339 微秒" },
//!   "auth_method": "consumer" }
//! ```
//!
//! Keychain 里存的是 `go-keyring-base64:` + base64(载荷)。这里把"跑命令"抽成
//! [CommandRunner]，测试据此断言命令与载荷，而不会真的去动开发者的钥匙串。

use std::path::{Path, PathBuf};

use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use serde::{Deserialize, Serialize};

use crate::error::{AccountError, AccountResult};

/// 系统凭据的服务名与账号名（Antigravity 桌面端）。
pub(crate) const KEYCHAIN_SERVICE: &str = "gemini";
pub(crate) const KEYCHAIN_ACCOUNT: &str = "antigravity";
/// go-keyring 在 macOS 上给载荷加的包装前缀。
pub(crate) const KEYCHAIN_PREFIX: &str = "go-keyring-base64:";
/// macOS 上跑 Keychain 命令的可执行文件。
const SECURITY_PROGRAM: &str = "/usr/bin/security";

/// 桌面客户端在 Keyring 不可用时的回退文件（`~/.gemini/` 下）。
pub(crate) const DESKTOP_FALLBACK_FILE: &str = "jetski-standalone-oauth-token";
/// agy CLI 的会话目录与文件；目录存在才算它装着。
pub(crate) const CLI_SESSION_DIR: &str = "antigravity-cli";
pub(crate) const CLI_SESSION_FILE: &str = "antigravity-oauth-token";

/// 跑一条外部命令的最小接口。Keychain 只能调 `security`，但测试不该真的动
/// 用户的钥匙串，所以命令执行从这里注入。
pub(crate) trait CommandRunner {
    fn run(&self, program: &str, args: &[&str]) -> std::io::Result<std::process::Output>;
}

/// 生产用的执行器：真的起一个进程。
pub(crate) struct SystemRunner;

impl CommandRunner for SystemRunner {
    fn run(&self, program: &str, args: &[&str]) -> std::io::Result<std::process::Output> {
        std::process::Command::new(program).args(args).output()
    }
}

/// 一份系统凭据，载荷解析后。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SystemCredential {
    pub(crate) access_token: Option<String>,
    pub(crate) refresh_token: String,
    pub(crate) expires_at: Option<i64>,
    pub(crate) email: Option<String>,
}

#[derive(Serialize)]
struct TokenPayload {
    access_token: String,
    token_type: String,
    refresh_token: String,
    expiry: String,
}

#[derive(Serialize)]
struct CredentialPayload {
    token: TokenPayload,
    auth_method: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    id_token: Option<String>,
}

#[derive(Deserialize)]
struct StoredToken {
    access_token: Option<String>,
    refresh_token: Option<String>,
    expiry: Option<String>,
}

#[derive(Deserialize)]
struct StoredPayload {
    token: StoredToken,
    id_token_email: Option<String>,
}

// ---------------------------------------------------------------------------
// 载荷
// ---------------------------------------------------------------------------

/// 一份系统凭据的 JSON 载荷。expiry 用 RFC3339 微秒：go-keyring 自己也是这么
/// 写的，客户端解析它时不做兼容。
pub(crate) fn payload_json(
    access_token: &str,
    refresh_token: &str,
    expires_at: i64,
    id_token: Option<&str>,
) -> String {
    let expiry = chrono::DateTime::from_timestamp(expires_at, 0)
        .unwrap_or_else(chrono::Utc::now)
        .to_rfc3339_opts(chrono::SecondsFormat::Micros, true);
    let payload = CredentialPayload {
        token: TokenPayload {
            access_token: access_token.to_string(),
            token_type: "Bearer".to_string(),
            refresh_token: refresh_token.to_string(),
            expiry,
        },
        auth_method: "consumer".to_string(),
        id_token: id_token.map(str::to_string),
    };
    serde_json::to_string(&payload).unwrap_or_else(|_| "{}".to_string())
}

/// 把新载荷合进客户端原来那份：我们拥有的字段覆盖，客户端自己的其它字段留着。
///
/// 客户端自己写的载荷除了 `token` 和 `auth_method` 还有 `writer_id`、
/// `token.expires_in`、`id_token_email` 这类我们读不懂但客户端认得的字段。
/// 整条换成我们这份"最小载荷"等于替客户端忘掉它们，所以按原载荷合并：`token`
/// 里我们拥有的子字段覆盖，多出来的兄弟字段与顶层未知键原样保留；知道邮箱时
/// 顺手把 `id_token_email` 指到这次写入的账号（它是客户端的登录提示）。
pub(crate) fn merge_payload(existing: Option<&str>, fresh: &str, email: Option<&str>) -> String {
    // 没有可合并的原载荷（客户端还没写过）时原样用新的，不做一次无意义的键重排。
    let Some(serde_json::Value::Object(mut target)) =
        existing.and_then(|raw| serde_json::from_str::<serde_json::Value>(raw).ok())
    else {
        return fresh.to_string();
    };
    let Ok(serde_json::Value::Object(source)) = serde_json::from_str::<serde_json::Value>(fresh)
    else {
        return fresh.to_string();
    };
    for (key, value) in &source {
        let into_token = key == "token"
            && matches!(target.get(key), Some(serde_json::Value::Object(_)))
            && matches!(value, serde_json::Value::Object(_));
        if !into_token {
            target.insert(key.clone(), value.clone());
            continue;
        }
        if let (Some(serde_json::Value::Object(token)), serde_json::Value::Object(fresh_token)) =
            (target.get_mut(key), value)
        {
            for (token_key, token_value) in fresh_token {
                token.insert(token_key.clone(), token_value.clone());
            }
        }
    }
    if let Some(email) = email.map(str::trim).filter(|email| !email.is_empty()) {
        target.insert(
            "id_token_email".to_string(),
            serde_json::Value::String(email.to_string()),
        );
    }
    serde_json::to_string(&serde_json::Value::Object(target))
        .unwrap_or_else(|_| fresh.to_string())
}

/// 载荷 JSON 到凭据。没有 refresh_token 的载荷对切号没有意义，按读不到处理。
pub(crate) fn parse_payload(payload: &str) -> AccountResult<SystemCredential> {
    let parsed: StoredPayload = serde_json::from_str(payload).map_err(|error| {
        AccountError::Client(format!(
            "Antigravity 系统凭据不是合法 JSON: {error}"
        ))
    })?;
    let refresh_token = clean(parsed.token.refresh_token).ok_or_else(|| {
        AccountError::Client("Antigravity 系统凭据缺少 refresh_token".to_string())
    })?;
    Ok(SystemCredential {
        access_token: clean(parsed.token.access_token),
        refresh_token,
        expires_at: parsed
            .token
            .expiry
            .as_deref()
            .and_then(|expiry| chrono::DateTime::parse_from_rfc3339(expiry.trim()).ok())
            .map(|at| at.timestamp()),
        email: clean(parsed.id_token_email),
    })
}

fn clean(value: Option<String>) -> Option<String> {
    value
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

/// Keychain 里的值：`go-keyring-base64:` + base64(载荷)。
pub(crate) fn keychain_value(payload: &str) -> String {
    format!("{KEYCHAIN_PREFIX}{}", STANDARD.encode(payload.as_bytes()))
}

/// Keychain 的值到载荷 JSON。
pub(crate) fn parse_keychain_value(raw: &str) -> AccountResult<String> {
    let encoded = raw
        .trim()
        .strip_prefix(KEYCHAIN_PREFIX)
        .ok_or_else(|| {
            AccountError::Client(
                "Antigravity Keychain 凭据不是 go-keyring-base64 形态".to_string(),
            )
        })?;
    let decoded = STANDARD.decode(encoded).map_err(|error| {
        AccountError::Client(format!("Antigravity Keychain 凭据解码失败: {error}"))
    })?;
    String::from_utf8(decoded).map_err(|_| {
        AccountError::Client("Antigravity Keychain 凭据不是有效 UTF-8".to_string())
    })
}

/// 同时兼容裸 JSON 载荷与 `go-keyring-base64:` 包装文本。
pub(crate) fn decode_payload_text(raw: &str) -> AccountResult<Option<String>> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Ok(None);
    }
    if trimmed.starts_with(KEYCHAIN_PREFIX) {
        return parse_keychain_value(trimmed).map(Some);
    }
    Ok(Some(trimmed.to_string()))
}

// ---------------------------------------------------------------------------
// Keychain
// ---------------------------------------------------------------------------

/// 读系统凭据；没有这个条目就是 None（客户端还没登录过）。
pub(crate) fn read_keychain(runner: &dyn CommandRunner) -> AccountResult<Option<String>> {
    let output = runner
        .run(
            SECURITY_PROGRAM,
            &[
                "find-generic-password",
                "-s",
                KEYCHAIN_SERVICE,
                "-a",
                KEYCHAIN_ACCOUNT,
                "-w",
            ],
        )
        .map_err(|error| {
            AccountError::Client(format!("执行 macOS Keychain 读取失败: {error}"))
        })?;
    if output.status.success() {
        let raw = String::from_utf8(output.stdout).map_err(|_| {
            AccountError::Client("Antigravity Keychain 凭据不是有效 UTF-8".to_string())
        })?;
        return Ok(Some(raw));
    }
    // security 用 44 报告"这条目不存在"。
    if output.status.code() == Some(44) {
        return Ok(None);
    }
    Err(AccountError::Client(format!(
        "读取 Antigravity Keychain 凭据失败: {}",
        String::from_utf8_lossy(&output.stderr).trim()
    )))
}

/// 写系统凭据。`-U` 原地更新，不加 `-A`/`-T`：那两个会顺手改访问策略，
/// 结果每次切号都弹授权框。
pub(crate) fn write_keychain(runner: &dyn CommandRunner, payload: &str) -> AccountResult<()> {
    let value = keychain_value(payload);
    let output = runner
        .run(
            SECURITY_PROGRAM,
            &[
                "add-generic-password",
                "-U",
                "-s",
                KEYCHAIN_SERVICE,
                "-a",
                KEYCHAIN_ACCOUNT,
                "-w",
                &value,
            ],
        )
        .map_err(|error| {
            AccountError::Client(format!("执行 macOS Keychain 写入失败: {error}"))
        })?;
    if output.status.success() {
        return Ok(());
    }
    Err(AccountError::Client(format!(
        "写入 Antigravity Keychain 失败: {}",
        String::from_utf8_lossy(&output.stderr).trim()
    )))
}

/// 删掉系统凭据（回滚到"客户端原本没有这条"的状态）。
pub(crate) fn delete_keychain(runner: &dyn CommandRunner) -> AccountResult<()> {
    let output = runner
        .run(
            SECURITY_PROGRAM,
            &[
                "delete-generic-password",
                "-s",
                KEYCHAIN_SERVICE,
                "-a",
                KEYCHAIN_ACCOUNT,
            ],
        )
        .map_err(|error| {
            AccountError::Client(format!("执行 macOS Keychain 删除失败: {error}"))
        })?;
    // 已经没有这条就等于删掉了。
    if output.status.success() || output.status.code() == Some(44) {
        return Ok(());
    }
    Err(AccountError::Client(format!(
        "删除 Antigravity Keychain 凭据失败: {}",
        String::from_utf8_lossy(&output.stderr).trim()
    )))
}

// ---------------------------------------------------------------------------
// ~/.gemini 文件
// ---------------------------------------------------------------------------

/// 客户端会读的两个回退文件；CLI 那个只在目录存在时才算数。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct GeminiPaths {
    pub(crate) fallback: PathBuf,
    pub(crate) cli_session: Option<PathBuf>,
}

/// `~/.gemini` 下的两个回退文件。
pub(crate) fn gemini_paths(gemini_dir: &Path) -> GeminiPaths {
    let cli_dir = gemini_dir.join(CLI_SESSION_DIR);
    GeminiPaths {
        fallback: gemini_dir.join(DESKTOP_FALLBACK_FILE),
        cli_session: cli_dir
            .is_dir()
            .then(|| cli_dir.join(CLI_SESSION_FILE)),
    }
}

/// 把同一份载荷写进两个回退文件。旧文件必须一起换掉：留着上一个账号的回退
/// 文件，客户端会在 Keyring 超时后"复活"那个账号。
pub(crate) fn write_gemini_files(paths: &GeminiPaths, payload: &str) -> AccountResult<()> {
    super::write_restricted(&paths.fallback, payload.as_bytes())?;
    if let Some(cli_session) = &paths.cli_session {
        super::write_restricted(cli_session, payload.as_bytes())?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_payload_round_trips_with_a_microsecond_expiry() {
        let payload = payload_json("at-1", "rt-1", 1_700_000_000, None);
        assert!(payload.contains("\"auth_method\":\"consumer\""), "{payload}");
        assert!(payload.contains("\"token_type\":\"Bearer\""), "{payload}");
        assert!(
            payload.contains("2023-11-14T22:13:20.000000Z"),
            "the expiry is RFC3339 with microseconds: {payload}"
        );
        assert!(!payload.contains("id_token"), "{payload}");

        let credential = parse_payload(&payload).expect("credential");
        assert_eq!(credential.access_token.as_deref(), Some("at-1"));
        assert_eq!(credential.refresh_token, "rt-1");
        assert_eq!(credential.expires_at, Some(1_700_000_000));
    }

    #[test]
    fn a_payload_without_a_refresh_token_is_not_a_credential() {
        let payload = payload_json("at-1", "  ", 1_700_000_000, None);
        let error = parse_payload(&payload).expect_err("no refresh token");
        assert!(matches!(error, AccountError::Client(_)), "{error:?}");
        assert!(parse_payload("not json").is_err());
    }

    #[test]
    fn an_id_token_is_carried_when_there_is_one() {
        let payload = payload_json("at-1", "rt-1", 1_700_000_000, Some("id-1"));
        assert!(payload.contains("\"id_token\":\"id-1\""), "{payload}");
    }

    #[test]
    fn merging_keeps_the_fields_the_client_wrote_itself() {
        // 客户端自己写的载荷：token 里多一个 expires_in，顶层有 writer_id 与
        // 未知键，id_token_email 还是上一个账号。
        let existing = r#"{"token":{"access_token":"old-at","token_type":"Bearer","refresh_token":"old-rt","expiry":"2026-11-17T16:06:50.000000Z","expires_in":3599},"auth_method":"consumer","writer_id":"w-1","id_token_email":"old@example.com","unknown":{"keep":true}}"#;
        let fresh = payload_json("new-at", "new-rt", 1_700_000_000, None);
        let merged = merge_payload(Some(existing), &fresh, Some("new@example.com"));
        let value: serde_json::Value = serde_json::from_str(&merged).expect("json");

        assert_eq!(value["token"]["access_token"], "new-at");
        assert_eq!(value["token"]["refresh_token"], "new-rt");
        assert_eq!(
            value["token"]["expiry"], "2023-11-14T22:13:20.000000Z",
            "我们拥有 expiry：{merged}"
        );
        assert_eq!(value["token"]["expires_in"], 3599, "{merged}");
        assert_eq!(value["writer_id"], "w-1", "{merged}");
        assert_eq!(value["unknown"]["keep"], true, "{merged}");
        assert_eq!(value["id_token_email"], "new@example.com", "{merged}");
        // 合并后的载荷必须还是客户端认得的凭据。
        assert_eq!(parse_payload(&merged).expect("credential").refresh_token, "new-rt");
    }

    #[test]
    fn merging_without_a_previous_payload_is_the_fresh_one() {
        let fresh = payload_json("at-1", "rt-1", 1_700_000_000, None);
        assert_eq!(merge_payload(None, &fresh, None), fresh);
        assert_eq!(merge_payload(Some("not json"), &fresh, None), fresh);
        // 不知道邮箱时不动客户端原来那个提示值。
        let existing = r#"{"token":{"refresh_token":"old-rt"},"auth_method":"consumer","id_token_email":"old@example.com"}"#;
        let merged = merge_payload(Some(existing), &fresh, None);
        let value: serde_json::Value = serde_json::from_str(&merged).expect("json");
        assert_eq!(value["id_token_email"], "old@example.com", "{merged}");
    }

    #[test]
    fn the_keychain_value_is_the_go_keyring_wrapping() {
        let payload = payload_json("at-1", "rt-1", 1_700_000_000, None);
        let value = keychain_value(&payload);
        assert!(value.starts_with(KEYCHAIN_PREFIX), "{value}");
        assert_eq!(parse_keychain_value(&value).expect("payload"), payload);

        let unwrapped = parse_keychain_value(&payload).expect_err("no prefix");
        assert!(matches!(unwrapped, AccountError::Client(_)), "{unwrapped:?}");
        assert!(parse_keychain_value("go-keyring-base64:!!!").is_err());
    }
}
