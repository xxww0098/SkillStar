//! Grok（xAI）：~/.grok/auth.json（GROK_HOME 可覆盖）。
//!
//! 文件是一个 JSON 对象，键是 OIDC 作用域串（https://auth.x.ai::<client id>），
//! 值是 CLI 自己的一份登录：key（访问令牌）、refresh_token、expires_at、
//! 以及从令牌 claim 里镜像出来的一串身份字段。
//!
//! 写回的三条硬约束，都来自"写进去但 CLI 用不了"比"切换失败"更糟：
//!
//! 1. **作用域不够就不写**：只有 api:access 的令牌能查计费，却让每条 CLI 调用
//!    失败，看起来是切换成功、实际是登录坏了。
//! 2. **条目要可还原**：CLI 从这条记录恢复会话，缺 create_time/user_id/email/
//!    姓名等字段会表现为"已登录但不可用"，所以合成后逐字段核对。
//! 3. **别人的身份字段要清掉**：换到 B 账号却留着 A 的 team/profile 镜像字段，
//!    UI 会按 A 的名字显示 B 的令牌；同一个账号自己的字段则保留（旋转后的不透明
//!    令牌复现不出来）。
//!
//! 不写系统钥匙串：Grok CLI 三个平台都只读这个文件。

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use serde_json::{Map, Value};

use super::paths::{self, PathRoots};
use super::{
    ClientSnapshot, LocalCredential, SwitchOutcome, SwitchRequest, read_json_file, snapshot_file,
    write_restricted,
};
use crate::error::{AccountError, AccountResult};
use crate::oauth::agents::grok;
use crate::oauth::{Identity, TokenSet};

/// CLI 要求的最小 scope 集合：缺一条就不写。
const REQUIRED_CLI_SCOPES: [&str; 3] = [
    "grok-cli:access",
    "conversations:read",
    "conversations:write",
];

/// 从令牌 claim 镜像到条目上的字段（CLI 的 UI 直接读它们）。
const MIRRORED_CLAIMS: [&str; 11] = [
    "email",
    "user_id",
    "principal_id",
    "principal_type",
    "team_id",
    "team_name",
    "team_role",
    "subscription_tier",
    "first_name",
    "last_name",
    "profile_image_asset_id",
];

/// CLI 恢复会话需要的字段：缺任何一个都算"半条记录"。
const RESTORABLE_FIELDS: [&str; 10] = [
    "create_time",
    "user_id",
    "email",
    "first_name",
    "last_name",
    "profile_image_asset_id",
    "principal_type",
    "principal_id",
    "team_id",
    "expires_at",
];

/// The file the Grok CLI reads at startup.
pub fn credential_path(roots: &PathRoots) -> PathBuf {
    paths::grok_auth_path(roots)
}

/// Write the Grok CLI credential.
pub fn write(roots: &PathRoots, request: &SwitchRequest<'_>) -> AccountResult<SwitchOutcome> {
    let path = credential_path(roots);
    let access_token = request.tokens.access_token.trim();
    if access_token.is_empty() {
        return Err(AccountError::Client(
            "this account has no access token to write into the Grok CLI".to_string(),
        ));
    }
    let refresh_token = request
        .tokens
        .refresh_token
        .as_deref()
        .map(str::trim)
        .filter(|token| !token.is_empty())
        .ok_or_else(|| {
            AccountError::Client(
                "this account has no Grok refresh token; a switch needs both tokens".to_string(),
            )
        })?;
    check_cli_scopes(access_token)?;

    let existing = read_json_file(&path)?;
    let root = materialize(existing.as_ref(), request, access_token, refresh_token)?;
    let body = serde_json::to_vec_pretty(&root)?;
    write_restricted(&path, &body)?;

    // 回读：写不进去（或写进去的是别的账号）必须让切换失败。
    let stored = read_entry(&path)?.ok_or_else(|| {
        AccountError::Client(format!(
            "{} did not keep the Grok login: no entry with a key",
            path.display()
        ))
    })?;
    if stored.tokens.access_token != access_token
        || stored.tokens.refresh_token.as_deref() != Some(refresh_token)
    {
        return Err(AccountError::Client(format!(
            "{} did not keep the Grok login: it reads back a different token",
            path.display()
        )));
    }
    Ok(SwitchOutcome {
        agent: "grok".to_string(),
        live: live_identity(&stored.identity),
        target: path.display().to_string(),
    })
}

/// 合成要写进去的那一份 root：只碰我们自己的作用域条目，别的条目原样保留。
fn materialize(
    base: Option<&Value>,
    request: &SwitchRequest<'_>,
    access_token: &str,
    refresh_token: &str,
) -> AccountResult<Value> {
    let mut root = base
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_else(Map::new);
    let scope = grok::scope_key();
    let mut entry = root
        .get(&scope)
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_else(Map::new);
    entry.insert("key".to_string(), Value::String(access_token.to_string()));
    entry.insert(
        "refresh_token".to_string(),
        Value::String(refresh_token.to_string()),
    );
    if let Some(expires_at) = expires_at(request.tokens).and_then(format_rfc3339) {
        entry.insert("expires_at".to_string(), Value::String(expires_at));
    }
    // 换了账号就把同一份 entry 里属于上一个账号的镜像字段清掉。
    let previous_identity = entry_identity(base.unwrap_or(&Value::Null));
    let wanted_identity = request_identity(request);
    if conflicts(&previous_identity, &wanted_identity) {
        for field in MIRRORED_CLAIMS {
            entry.remove(field);
        }
    }
    fill_schema(&mut entry, request, access_token);
    require_restorable(&entry)?;
    root.insert(scope, Value::Object(entry));
    Ok(Value::Object(root))
}

/// 令牌里读得到 scope 时核对 CLI 需要的三条；不透明令牌没有可读 claim，
/// 那不算"缺 scope"的证据，放行。
fn check_cli_scopes(access_token: &str) -> AccountResult<()> {
    let Some(claims) = crate::identity::jwt_claims(access_token) else {
        return Ok(());
    };
    let mut scopes: HashSet<String> = HashSet::new();
    for key in ["scope", "scp"] {
        match claims.get(key) {
            Some(Value::String(raw)) => {
                scopes.extend(raw.split_whitespace().map(str::to_string));
            }
            Some(Value::Array(values)) => {
                scopes.extend(values.iter().filter_map(Value::as_str).map(str::to_string));
            }
            _ => {}
        }
    }
    let missing: Vec<&str> = REQUIRED_CLI_SCOPES
        .iter()
        .copied()
        .filter(|required| !scopes.contains(*required))
        .collect();
    if missing.is_empty() {
        return Ok(());
    }
    Err(AccountError::Client(format!(
        "this Grok token cannot drive the CLI: it is missing {}",
        missing.join(", ")
    )))
}

/// 把条目补齐到 CLI 能恢复会话的形状，已存在的一律不覆盖。
fn fill_schema(entry: &mut Map<String, Value>, request: &SwitchRequest<'_>, access_token: &str) {
    entry
        .entry("auth_mode".to_string())
        .or_insert_with(|| Value::String("oidc".to_string()));
    entry
        .entry("oidc_issuer".to_string())
        .or_insert_with(|| Value::String("https://auth.x.ai".to_string()));
    entry
        .entry("oidc_client_id".to_string())
        .or_insert_with(|| Value::String(grok::client_id().to_string()));

    if let Some(claims) = crate::identity::jwt_claims(access_token) {
        for field in MIRRORED_CLAIMS {
            if let Some(value) = claims.get(field).and_then(Value::as_str) {
                entry
                    .entry(field.to_string())
                    .or_insert_with(|| Value::String(value.to_string()));
            }
        }
        if let Some(subject) = claims.get("sub").and_then(Value::as_str) {
            entry
                .entry("user_id".to_string())
                .or_insert_with(|| Value::String(subject.to_string()));
        }
        if let Some(opt_out) = claims
            .get("coding_data_retention_opt_out")
            .and_then(Value::as_bool)
        {
            entry
                .entry("coding_data_retention_opt_out".to_string())
                .or_insert(Value::Bool(opt_out));
        }
    }

    if let Some(email) = request_email(request) {
        entry
            .entry("email".to_string())
            .or_insert_with(|| Value::String(email.clone()));
        let prefix = email
            .split('@')
            .next()
            .unwrap_or_default()
            .to_string();
        entry
            .entry("first_name".to_string())
            .or_insert_with(|| Value::String(prefix));
    }
    if let Some(subject) = request_subject(request) {
        entry
            .entry("user_id".to_string())
            .or_insert_with(|| Value::String(subject.clone()));
        entry
            .entry("principal_id".to_string())
            .or_insert_with(|| Value::String(subject));
    }

    entry
        .entry("create_time".to_string())
        .or_insert_with(|| Value::String(now_rfc3339()));
    for field in ["last_name", "profile_image_asset_id", "team_id"] {
        entry
            .entry(field.to_string())
            .or_insert_with(|| Value::String(String::new()));
    }
    entry
        .entry("principal_type".to_string())
        .or_insert_with(|| Value::String("user".to_string()));
    entry
        .entry("coding_data_retention_opt_out".to_string())
        .or_insert(Value::Bool(false));
}

/// 合成出来的条目必须能被 CLI 还原成一次登录。
fn require_restorable(entry: &Map<String, Value>) -> AccountResult<()> {
    for field in RESTORABLE_FIELDS {
        if entry.get(field).and_then(Value::as_str).is_none() {
            return Err(AccountError::Client(format!(
                "the Grok CLI would read a half-formed login: {field} is missing"
            )));
        }
    }
    Ok(())
}

/// Read the Grok CLI credential in full (this is how "add from this machine" works).
pub fn read(roots: &PathRoots) -> AccountResult<Option<LocalCredential>> {
    let path = credential_path(roots);
    let Some(stored) = read_entry(&path)? else {
        return Ok(None);
    };
    Ok(Some(LocalCredential {
        tokens: stored.tokens,
        identity: stored.identity,
        provider_state: None,
        source: path.display().to_string(),
    }))
}

/// 一份读出来的登录。
struct StoredEntry {
    tokens: TokenSet,
    identity: Identity,
}

/// 条目的身份：provider 侧账号 id 优先，其次 email。
pub(crate) fn live_identity(identity: &Identity) -> Option<String> {
    identity.account_id.clone().or_else(|| identity.email.clone())
}

/// Capture the auth file so a failed switch can put it back.
pub fn snapshot(roots: &PathRoots) -> AccountResult<ClientSnapshot> {
    snapshot_file(&credential_path(roots))
}

/// 我们的作用域条目优先；否则按 Cockpit Tools 的读法取第一个带 key 的条目
/// （键排序，保证同一份文件每次读到同一条）。
fn read_entry(path: &Path) -> AccountResult<Option<StoredEntry>> {
    let Some(root) = read_json_file(path)? else {
        return Ok(None);
    };
    let Some(map) = root.as_object() else {
        return Ok(None);
    };
    let scope = grok::scope_key();
    let entry = match map.get(&scope) {
        Some(entry) if entry_has_key(entry) => entry,
        _ => {
            let mut keys: Vec<&String> = map.keys().collect();
            keys.sort();
            match keys
                .into_iter()
                .map(|key| &map[key])
                .find(|entry| entry_has_key(entry))
            {
                Some(entry) => entry,
                None => return Ok(None),
            }
        }
    };
    let Some(access_token) = text_field(entry, "key") else {
        return Ok(None);
    };
    // 过期时间取"文件里写的"和"令牌自己声明的"里更早的那个。
    let stored = entry.get("expires_at").and_then(rfc3339_seconds);
    let jwt = crate::identity::jwt_exp(&access_token);
    let expires_at = match (stored, jwt) {
        (Some(stored), Some(jwt)) => Some(stored.min(jwt)),
        (stored, jwt) => stored.or(jwt),
    };
    let email = text_field(entry, "email").or_else(|| crate::identity::jwt_email(&access_token));
    let subject = ["user_id", "principal_id", "sub"]
        .into_iter()
        .find_map(|field| text_field(entry, field))
        .or_else(|| crate::identity::jwt_claim(&access_token, "sub"));
    Ok(Some(StoredEntry {
        tokens: TokenSet {
            access_token,
            refresh_token: text_field(entry, "refresh_token"),
            id_token: None,
            expires_at,
            token_type: "Bearer".to_string(),
        },
        identity: Identity {
            display_name: email.clone(),
            email,
            plan: None,
            account_id: subject,
        },
    }))
}

fn entry_has_key(entry: &Value) -> bool {
    entry
        .get("key")
        .and_then(Value::as_str)
        .is_some_and(|key| !key.trim().is_empty())
}

fn text_field(entry: &Value, field: &str) -> Option<String> {
    entry
        .get(field)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

/// 一份条目的身份（用于判断"上一个账号的镜像字段要不要清"）。
fn entry_identity(root: &Value) -> Identity {
    let scope = grok::scope_key();
    let entry = root.get(&scope).unwrap_or(root);
    Identity {
        email: text_field(entry, "email"),
        display_name: None,
        plan: None,
        account_id: ["user_id", "principal_id", "sub"]
            .into_iter()
            .find_map(|field| text_field(entry, field)),
    }
}

fn request_identity(request: &SwitchRequest<'_>) -> Identity {
    Identity {
        email: request_email(request),
        display_name: None,
        plan: None,
        account_id: request_subject(request),
    }
}

/// 记录侧的身份；记录里没有时退回令牌 claim。
fn request_email(request: &SwitchRequest<'_>) -> Option<String> {
    request
        .email
        .map(str::trim)
        .filter(|email| email.contains('@'))
        .map(str::to_string)
        .or_else(|| {
            request
                .display_name
                .contains('@')
                .then(|| request.display_name.trim().to_string())
        })
        .or_else(|| {
            request
                .tokens
                .id_token
                .as_deref()
                .and_then(crate::identity::jwt_email)
        })
        .or_else(|| crate::identity::jwt_email(&request.tokens.access_token))
}

fn request_subject(request: &SwitchRequest<'_>) -> Option<String> {
    request
        .account_id
        .map(str::trim)
        .filter(|id| !id.is_empty())
        .map(str::to_string)
        .or_else(|| {
            request
                .tokens
                .id_token
                .as_deref()
                .and_then(|token| crate::identity::jwt_claim(token, "sub"))
        })
        .or_else(|| crate::identity::jwt_claim(&request.tokens.access_token, "sub"))
}

/// 两条身份是否指向不同账号：只有"两边都已知且不同"才算冲突（不知道不等于换了）。
fn conflicts(previous: &Identity, wanted: &Identity) -> bool {
    let differs = |left: &Option<String>, right: &Option<String>| match (left, right) {
        (Some(left), Some(right)) => !crate::identity::same_identity(Some(left), Some(right)),
        _ => false,
    };
    differs(&previous.account_id, &wanted.account_id) || differs(&previous.email, &wanted.email)
}

fn expires_at(tokens: &TokenSet) -> Option<i64> {
    tokens
        .expires_at
        .or_else(|| crate::identity::jwt_exp(&tokens.access_token))
}

fn format_rfc3339(seconds: i64) -> Option<String> {
    chrono::DateTime::from_timestamp(seconds, 0)
        .map(|date| date.format("%Y-%m-%dT%H:%M:%S%.6fZ").to_string())
}

fn rfc3339_seconds(value: &Value) -> Option<i64> {
    let text = value.as_str()?.trim();
    chrono::DateTime::parse_from_rfc3339(text)
        .ok()
        .map(|at| at.timestamp())
}

fn now_rfc3339() -> String {
    chrono::Utc::now().format("%Y-%m-%dT%H:%M:%S%.6fZ").to_string()
}

#[cfg(test)]
#[path = "grok_tests.rs"]
mod grok_tests;
