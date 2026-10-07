//! Imported Sign in with ChatGPT sessions. This family has no quota endpoint.
use super::super::account_json::{read, stamp};
use crate::subscription::{Subscription, SubscriptionUsage};
use crate::token_import::ImportedToken;
use crate::{UsageError, UsageResult, crypto};
use serde_json::{Value, json};
const RESOURCE: &str = "https://api.openai.com/v1";
const DIRECT: &str = "chatgpt.tokens.use.direct";

pub(crate) fn import_from_token(input: &str) -> UsageResult<ImportedToken> {
    let value: Value = serde_json::from_str(input)
        .map_err(|_| UsageError::Other("请导入完整 ChatGPT 会话 JSON".into()))?;
    let client = value["clientId"].as_str().unwrap_or("");
    if !client.starts_with("oaiapp_")
        || client.len() <= 7
        || !client
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || c == b'_' || c == b'-')
    {
        return Err(UsageError::Other(
            "ChatGPT 会话缺少已签发的 oaiapp_ clientId".into(),
        ));
    }
    let access = value["accessToken"]
        .as_str()
        .filter(|v| !v.is_empty() && !v.chars().any(|c| c.is_whitespace() || c.is_control()))
        .ok_or_else(|| UsageError::Other("ChatGPT 缺少 accessToken".into()))?;
    let refresh = value["refreshToken"]
        .as_str()
        .filter(|v| !v.is_empty())
        .ok_or_else(|| UsageError::Other("ChatGPT 缺少 refreshToken".into()))?;
    let scopes = value["scopes"]
        .as_array()
        .ok_or_else(|| UsageError::Other("ChatGPT 会话缺少 scopes".into()))?;
    if !scopes.iter().any(|v| v.as_str() == Some(DIRECT)) {
        return Err(UsageError::Other("ChatGPT 未授权使用订阅计划".into()));
    }
    let expiry = stamp(&value["expiresAt"])
        .ok_or_else(|| UsageError::Other("ChatGPT 缺少 expiresAt".into()))?;
    Ok(ImportedToken {
        display_name: value["emailAddress"].as_str().or(value["account"].as_str()).unwrap_or("ChatGPT").into(),
        access_token: access.into(), refresh_token: Some(refresh.into()), expires_at: Some(expiry),
        oauth_account_id: value["subject"].as_str().map(str::to_string),
        provider_state: Some(json!({"clientId":client,"scopes":scopes,"earliestRefreshAt":value["earliestRefreshAt"],"planType":value["planType"]}).to_string()),
        currency: Some("USD".into()), oauth_region: None,
        id_token: value["idToken"].as_str().map(str::to_string), api_key: None,
    })
}

pub async fn fetch(sub: &mut Subscription) -> UsageResult<SubscriptionUsage> {
    let client = super::super::http_client()?;
    let mut state: Value = serde_json::from_str(&super::super::decrypt_required(
        &sub.provider_state_encrypted,
        "ChatGPT 会话",
    )?)
    .map_err(|_| UsageError::AuthRequired)?;
    let now = chrono::Utc::now().timestamp();
    if sub.access_token_expires_at.is_none_or(|t| t <= now + 300) {
        if stamp(&state["earliestRefreshAt"]).is_some_and(|t| t > now) {
            return Err(UsageError::Transient(
                "ChatGPT 尚未允许刷新登录，请稍后重试".into(),
            ));
        }
        let refresh = super::super::decrypt_required(&sub.refresh_token_encrypted, "refreshToken")?;
        let client_id = state["clientId"]
            .as_str()
            .filter(|v| v.starts_with("oaiapp_"))
            .ok_or(UsageError::AuthRequired)?;
        let tokens = crate::oauth::token_endpoint::post_token(
            "https://auth.openai.com/api/accounts/oauth/token",
            &[
                ("grant_type", "refresh_token"),
                ("client_id", client_id),
                ("refresh_token", refresh.as_str()),
                ("resource", RESOURCE),
            ],
            "ChatGPT refresh",
        )
        .await?;
        let access = tokens.access_token().ok_or(UsageError::AuthRequired)?;
        let refresh = tokens.refresh_token().ok_or(UsageError::AuthRequired)?;
        let expiry = tokens
            .expires_in
            .filter(|v| *v > 0)
            .ok_or(UsageError::AuthRequired)?;
        if tokens
            .token_type
            .as_deref()
            .is_some_and(|t| !t.eq_ignore_ascii_case("bearer"))
        {
            return Err(UsageError::AuthRequired);
        }
        if let Some(scope) = tokens.scope.as_deref() {
            if !scope.split_whitespace().any(|v| v == DIRECT) {
                return Err(UsageError::AuthRequired);
            }
            state["scopes"] = json!(scope.split_whitespace().collect::<Vec<_>>());
        }
        state["earliestRefreshAt"] = tokens.earliest_refresh_at.clone();
        sub.access_token_encrypted = Some(crypto::encrypt(access));
        sub.refresh_token_encrypted = Some(crypto::encrypt(refresh));
        sub.access_token_expires_at = Some(now.saturating_add(expiry));
        if let Some(id_token) = tokens.id_token() {
            sub.id_token_encrypted = Some(crypto::encrypt(id_token));
        }
        sub.provider_state_encrypted = Some(crypto::encrypt(&state.to_string()));
    }
    let access = super::super::decrypt_required(&sub.access_token_encrypted, "accessToken")?;
    // Validate the imported session without fabricating usage or touching Codex backend-api.
    read(client.get(format!("{RESOURCE}/models")).bearer_auth(access)).await?;
    Ok(SubscriptionUsage {
        subscription_id: sub.id.clone(),
        fetched_at: now,
        plan_name: state["planType"].as_str().map(str::to_string),
        ..Default::default()
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn imported_session_requires_issued_client_and_plan_consent() {
        let mut v = json!({"clientId":"oaiapp_abc","accessToken":"a","refreshToken":"r","expiresAt":1900000000000_i64,"scopes":[DIRECT]});
        assert_eq!(
            import_from_token(&v.to_string()).unwrap().expires_at,
            Some(1900000000)
        );
        v["clientId"] = json!("dynamic_agent_client");
        assert!(import_from_token(&v.to_string()).is_err());
        v["clientId"] = json!("oaiapp_abc");
        v["scopes"] = json!([]);
        assert!(import_from_token(&v.to_string()).is_err());
    }
}
