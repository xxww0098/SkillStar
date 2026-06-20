//! Manual OAuth callback submission for desktop flows.
//!
//! Some providers show a `code` or leave the user on a local callback URL when
//! the desktop listener is unreachable. This module turns pasted callback
//! content into the same localhost GET request the browser would have sent, so
//! the provider-specific OAuth worker can keep using its existing exchange path.

use std::time::Duration;

use tokio::time::sleep;
use url::{Url, form_urlencoded};

use crate::oauth::pending_state;
use crate::{UsageError, UsageResult};

const CALLBACK_TIMEOUT: Duration = Duration::from_secs(8);
const CALLBACK_RETRIES: usize = 5;

pub async fn submit(pending_id: &str, callback_input: &str) -> UsageResult<()> {
    let auth_url = pending_state::auth_url(pending_id)
        .ok_or_else(|| UsageError::NotFound(pending_id.to_string()))?;
    let callback_url = build_callback_url(&auth_url, callback_input)?;
    let client = reqwest::Client::builder()
        .timeout(CALLBACK_TIMEOUT)
        .build()
        .map_err(|e| UsageError::Other(format!("OAuth 回调客户端创建失败: {}", e)))?;

    let mut last_error = None;
    for attempt in 0..CALLBACK_RETRIES {
        match client.get(callback_url.as_str()).send().await {
            Ok(resp) => {
                if resp.status().is_success() {
                    return Ok(());
                }
                return Err(UsageError::Other(format!(
                    "OAuth 回调提交失败，状态码 {}",
                    resp.status()
                )));
            }
            Err(err) => {
                last_error = Some(err.to_string());
                if attempt + 1 < CALLBACK_RETRIES {
                    sleep(Duration::from_millis(160)).await;
                }
            }
        }
    }

    Err(UsageError::Other(format!(
        "OAuth 回调提交失败: {}",
        last_error.unwrap_or_else(|| "未知错误".to_string())
    )))
}

fn build_callback_url(auth_url: &str, callback_input: &str) -> UsageResult<Url> {
    let input = callback_input.trim();
    if input.is_empty() {
        return Err(UsageError::Other("请输入 callback URL 或授权 code".into()));
    }

    if let Ok(url) = Url::parse(input)
        && (url.scheme() == "http" || url.scheme() == "https")
    {
        return normalize_local_url(url);
    }

    let auth = Url::parse(auth_url)
        .map_err(|e| UsageError::Other(format!("OAuth 授权链接无效: {}", e)))?;
    let redirect = auth
        .query_pairs()
        .find_map(|(key, value)| {
            matches!(
                key.as_ref(),
                "redirect_uri" | "redirect" | "redirect_url" | "callback_url"
            )
            .then(|| value.into_owned())
        })
        .ok_or_else(|| {
            UsageError::Other(
                "当前 OAuth 流程没有 callback URL，可在浏览器授权后等待自动完成。".into(),
            )
        })?;
    let mut callback = normalize_redirect_url(&redirect)?;
    let expected_state = auth
        .query_pairs()
        .find_map(|(key, value)| (key == "state").then(|| value.into_owned()));

    let raw_query = input.trim_start_matches('?').trim_start_matches('#');
    let looks_like_query = raw_query.contains('=') || raw_query.contains('&');
    let mut fields: Vec<(String, String)> = if looks_like_query {
        form_urlencoded::parse(raw_query.as_bytes())
            .map(|(key, value)| (key.into_owned(), value.into_owned()))
            .collect()
    } else {
        vec![("code".to_string(), input.to_string())]
    };

    if !fields.iter().any(|(key, _)| key == "state")
        && let Some(state) = expected_state
    {
        fields.push(("state".to_string(), state));
    }

    callback.set_query(None);
    callback.set_fragment(None);
    {
        let mut query = callback.query_pairs_mut();
        for (key, value) in fields {
            query.append_pair(&key, &value);
        }
    }
    Ok(callback)
}

fn normalize_redirect_url(raw: &str) -> UsageResult<Url> {
    let mut url = Url::parse(raw)
        .map_err(|e| UsageError::Other(format!("OAuth callback URL 无效: {}", e)))?;
    if url.path().is_empty() || url.path() == "/" {
        // Kiro registers a localhost origin and later redirects to a callback
        // path. Use the Google/GitHub browser path for code-only paste.
        url.set_path("/oauth/callback");
    }
    normalize_local_url(url)
}

fn normalize_local_url(mut url: Url) -> UsageResult<Url> {
    if url.scheme() != "http" {
        return Err(UsageError::Other(
            "callback URL 必须是本机 http://127.0.0.1 或 http://localhost 地址".into(),
        ));
    }
    let host = url.host_str().unwrap_or_default().to_ascii_lowercase();
    if host != "127.0.0.1" && host != "localhost" {
        return Err(UsageError::Other(
            "callback URL 只允许提交到本机 127.0.0.1/localhost".into(),
        ));
    }
    if host == "localhost" {
        url.set_host(Some("127.0.0.1"))
            .map_err(|_| UsageError::Other("无法规范化 callback host".into()))?;
    }

    if let Some(fragment) = url.fragment().map(str::to_string)
        && fragment.contains('=')
    {
        let merged = match url.query() {
            Some(query) if !query.is_empty() => format!("{}&{}", query, fragment),
            _ => fragment,
        };
        url.set_query(Some(&merged));
        url.set_fragment(None);
    }

    Ok(url)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn code_only_uses_redirect_and_state() {
        let auth = "https://auth.x.ai/oauth2/authorize?redirect_uri=http%3A%2F%2F127.0.0.1%3A56121%2Fcallback&state=s1";
        let callback = build_callback_url(auth, "abc123").unwrap();
        assert_eq!(
            callback.as_str(),
            "http://127.0.0.1:56121/callback?code=abc123&state=s1"
        );
    }

    #[test]
    fn query_input_keeps_fields_and_adds_state() {
        let auth = "https://auth.openai.com/oauth/authorize?redirect_uri=http%3A%2F%2F127.0.0.1%3A1455%2Fauth%2Fcallback&state=s2";
        let callback = build_callback_url(auth, "code=abc&scope=openid").unwrap();
        assert_eq!(
            callback.as_str(),
            "http://127.0.0.1:1455/auth/callback?code=abc&scope=openid&state=s2"
        );
    }

    #[test]
    fn fragment_callback_becomes_query() {
        let callback = build_callback_url(
            "https://example.com/auth",
            "http://localhost:3333/windsurf-auth-callback#access_token=tok&state=s3",
        )
        .unwrap();
        assert_eq!(
            callback.as_str(),
            "http://127.0.0.1:3333/windsurf-auth-callback?access_token=tok&state=s3"
        );
    }

    #[test]
    fn poll_flow_without_redirect_gets_clear_error() {
        let err = build_callback_url("https://cursor.com/loginDeepControl?uuid=u", "abc")
            .unwrap_err()
            .to_string();
        assert!(err.contains("没有 callback URL"));
    }
}
