//! Cookie-based fetchers (web-session-driven providers).
//!
//! Users paste raw `Cookie:` header from browser DevTools; the header is
//! parsed into a [`CookieJar`](crate::cookie_jar) and persisted encrypted.
//! Fetchers use these cookies to scrape usage data from provider consoles.

pub mod opencode;
pub mod stepfun;

use crate::cookie_jar;
use crate::crypto;
use crate::subscription::{Subscription, SubscriptionUsage};
use crate::{UsageError, UsageResult};

/// Dispatch a cookie refresh based on `subscription.catalog_id`.
pub async fn dispatch(subscription: &mut Subscription) -> UsageResult<SubscriptionUsage> {
    let cookie_json = decrypt_cookie_jar(&subscription.cookie_jar_encrypted)?;
    let entries = cookie_jar::deserialize_cookie_jar(&cookie_json)
        .ok_or_else(|| UsageError::Other("Cookie 数据已损坏，请重新粘贴。".into()))?;
    let cookie_header = cookie_jar::build_cookie_header(&entries);

    // Mark session expiry 24h from now on successful fetch (heuristic).
    let result = match subscription.catalog_id.as_str() {
        "opencode" | "opencode-go" | "opencode-zen" => {
            let tier = subscription.plan_tier.as_deref().or_else(|| {
                if subscription.catalog_id == "opencode-go" {
                    Some("Go")
                } else if subscription.catalog_id == "opencode-zen" {
                    Some("Zen")
                } else {
                    None
                }
            });
            opencode::fetch(&subscription.id, &entries, &cookie_header, tier).await
        }
        "stepfun" => stepfun::fetch(&subscription.id, &entries, &cookie_header).await,
        other => Err(super::unsupported(other)),
    };

    if result.is_ok() {
        subscription.cookie_session_expires_at = Some(chrono::Utc::now().timestamp() + 86_400);
    }

    result
}

pub(crate) fn decrypt_cookie_jar(cipher: &Option<String>) -> UsageResult<String> {
    let cipher = cipher
        .as_deref()
        .filter(|s| !s.is_empty())
        .ok_or_else(|| UsageError::Other("缺少 Cookie（请先粘贴浏览器 Cookie）".into()))?;
    let pt = crypto::decrypt(cipher);
    if pt.is_empty() {
        return Err(UsageError::AuthRequired);
    }
    Ok(pt)
}
