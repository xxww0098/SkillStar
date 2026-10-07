//! Cline account sessions and quota, matching the DSH Cline endpoint contract.
use super::super::account_json::{number, read, read_response, stamp};
use crate::subscription::{
    MonetaryBalance, Subscription, SubscriptionUsage, UsageUnit, UsageWindow,
};
use crate::token_import::ImportedToken;
use crate::{UsageError, UsageResult, crypto};
use serde_json::Value;
const BASE: &str = "https://api.cline.bot/api/v1";

pub(crate) fn import_from_token(input: &str) -> UsageResult<ImportedToken> {
    let value: Value = if input.trim().starts_with('{') {
        serde_json::from_str(input).map_err(|_| UsageError::Other("Cline 会话 JSON 无效".into()))?
    } else {
        serde_json::json!({"accessToken": input.trim()})
    };
    let access = value["accessToken"]
        .as_str()
        .or(value["access_token"].as_str())
        .unwrap_or("")
        .trim();
    if access.is_empty() || access.chars().any(|c| c.is_whitespace() || c.is_control()) {
        return Err(UsageError::Other("Cline 缺少有效 accessToken".into()));
    }
    let access = if access
        .get(..7)
        .is_some_and(|s| s.eq_ignore_ascii_case("workos:"))
    {
        format!("workos:{}", &access[7..])
    } else {
        format!("workos:{access}")
    };
    if access == "workos:" {
        return Err(UsageError::Other("Cline 缺少有效 accessToken".into()));
    }
    Ok(ImportedToken {
        display_name: value["account"].as_str().unwrap_or("Cline").into(),
        access_token: access,
        refresh_token: value["refreshToken"]
            .as_str()
            .or(value["refresh_token"].as_str())
            .map(str::to_string),
        expires_at: stamp(&value["expiresAt"]),
        oauth_account_id: value["userId"].as_str().map(str::to_string),
        provider_state: None,
        currency: Some("USD".into()),
        oauth_region: None,
        id_token: None,
        api_key: None,
    })
}

pub async fn fetch(sub: &mut Subscription) -> UsageResult<SubscriptionUsage> {
    let client = super::super::http_client()?;
    if sub
        .access_token_expires_at
        .is_some_and(|t| t <= chrono::Utc::now().timestamp() + 300)
    {
        let refresh = super::super::decrypt_required(&sub.refresh_token_encrypted, "refreshToken")?;
        let v = read_response(
            client
                .post(format!("{BASE}/auth/refresh"))
                .json(&serde_json::json!({"refreshToken":refresh,"grantType":"refresh_token"})),
        )
        .await?;
        if v["success"] != Value::Bool(true) {
            return Err(UsageError::AuthRequired);
        }
        let parsed = import_from_token(&v["data"].to_string())?;
        sub.access_token_encrypted = Some(crypto::encrypt(&parsed.access_token));
        if let Some(refresh) = parsed.refresh_token {
            sub.refresh_token_encrypted = Some(crypto::encrypt(&refresh));
        }
        sub.access_token_expires_at = parsed.expires_at.or(sub.access_token_expires_at);
    }
    let token = super::super::decrypt_required(&sub.access_token_encrypted, "accessToken")?;
    let me = read(client.get(format!("{BASE}/users/me")).bearer_auth(&token)).await?;
    let user = me.get("data").unwrap_or(&me);
    let id = user["id"]
        .as_str()
        .filter(|id| !id.is_empty())
        .ok_or_else(|| UsageError::Fetcher("Cline 缺少账号 ID".into()))?;
    let balance_url = format!("{BASE}/users/{}/balance", crate::urlencode::encode(id));
    let (balance, plan, limits) = tokio::join!(
        read(client.get(balance_url).bearer_auth(&token)),
        read(
            client
                .get(format!("{BASE}/users/me/plan"))
                .bearer_auth(&token)
        ),
        read(
            client
                .get(format!("{BASE}/users/me/plan/usage-limits"))
                .bearer_auth(&token)
        )
    );
    sub.oauth_account_id = Some(id.into());
    super::common::apply_email_title(sub, user["email"].as_str(), &["Cline"]);
    Ok(parse(
        &sub.id,
        &balance?,
        &plan.unwrap_or(Value::Null),
        &limits.unwrap_or(Value::Null),
    ))
}

fn parse(id: &str, balance: &Value, plan: &Value, limits: &Value) -> SubscriptionUsage {
    let balance = balance.get("data").unwrap_or(balance);
    let plan = plan.get("data").unwrap_or(plan);
    let limits = limits.get("data").unwrap_or(limits);
    let mut usage = SubscriptionUsage {
        subscription_id: id.into(),
        fetched_at: chrono::Utc::now().timestamp(),
        plan_name: plan["plan"]["displayName"]
            .as_str()
            .or(plan["plan"]["name"].as_str())
            .map(str::to_string),
        balance: number(&balance["balance"]).map(|v| MonetaryBalance {
            currency: "USD".into(),
            total: v / 1_000_000.0,
            granted: 0.0,
            topped_up: 0.0,
            is_available: None,
        }),
        ..Default::default()
    };
    for limit in limits["limits"].as_array().into_iter().flatten() {
        let Some(pct) = number(&limit["percentUsed"]) else {
            continue;
        };
        let (label, slot) = match limit["type"].as_str() {
            Some("five_hour") => ("5h", &mut usage.hourly),
            Some("weekly") => ("7d", &mut usage.weekly),
            Some("monthly") => ("30d", &mut usage.monthly),
            _ => continue,
        };
        *slot = Some(UsageWindow {
            label: label.into(),
            used: pct.clamp(0.0, 100.0).round() as i64,
            total: Some(100),
            percent: Some(pct.clamp(0.0, 100.0).round() as i32),
            reset_at: stamp(&limit["resetsAt"]),
            breakdown: vec![],

            unit: UsageUnit::Count,
        });
    }
    usage
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn session_and_quota_follow_cline_units() {
        assert!(import_from_token("bad token").is_err());
        assert_eq!(
            import_from_token("WORKOS:abc").unwrap().access_token,
            "workos:abc"
        );
        let usage = parse(
            "c",
            &serde_json::json!({"data":{"balance":"1230000"}}),
            &Value::Null,
            &serde_json::json!({"data":{"limits":[{"type":"five_hour","percentUsed":24,"resetsAt":"2030-01-01T00:00:00Z"}]}}),
        );
        assert_eq!(usage.balance.unwrap().total, 1.23);
        assert_eq!(usage.hourly.unwrap().percent, Some(24));
        assert!(usage.weekly.is_none());
    }
}
