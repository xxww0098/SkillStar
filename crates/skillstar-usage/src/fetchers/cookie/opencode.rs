//! OpenCode first-party services — Cookie-based usage fetcher.
//!
//! Supports `opencode` (Go / Zen selected via plan_tier).
//! Users paste cookies from `https://opencode.ai` after login.
//!
//! Data source: `https://opencode.ai/_server` SolidStart server functions.
//! The workspace page SSR payload is used to discover the workspace ID; then
//! dedicated server functions are called for keys, usage detail, monthly
//! aggregation, and billing.
//!
//! Server function IDs (content hashes) are stable per OpenCode deployment.
//! If OpenCode pushes a new release these hashes may change — the fetcher will
//! return an error prompting the user to update the cookie.

use chrono::{Datelike, Utc};
use serde_json::Value;

use crate::cookie_jar::CookieEntry;
use crate::subscription::{MonetaryBalance, OpenCodeApiKey, SubscriptionUsage, UsageWindow};
use crate::{UsageError, UsageResult};

const CONSOLE_BASE: &str = "https://opencode.ai";

// Server function content-hash IDs (built-in defaults). These are stable per
// OpenCode deployment; if OpenCode ships a release that changes them, they can
// be overridden at runtime via `~/.skillstar/config/opencode_scrape.json`
// (see `server_fn_ids`) so the fix lands without rebuilding/republishing.
const SFN_KEYS: &str = "def2ab20a296ef06465b1c3cf86da4ea983c0696e7a5708b9468aaed85083d6b";
const SFN_USAGE: &str = "6262ba54bff26cd7ec162f93db420e0d19df9cd94b2233dfe3b6b24c3f990388";
const SFN_BILLING: &str = "c83b78a614689c38ebee981f9b39a8b377716db85c1fd7dbab604adc02d3313d";

const COST_DIVISOR: f64 = 100_000_000.0;

/// Resolved server-function content-hash IDs (defaults + optional override).
struct ServerFnIds {
    keys: String,
    usage: String,
    billing: String,
}

/// Resolve the server-function IDs, applying any overrides from
/// `~/.skillstar/config/opencode_scrape.json`:
/// `{ "keys": "...", "usage": "...", "billing": "..." }`.
///
/// This makes a hash change upstream a config edit (or a pushed config) rather
/// than a required app release — a key resilience property for a fetcher that
/// depends on an external site's internal signatures.
fn server_fn_ids() -> ServerFnIds {
    let mut ids = ServerFnIds {
        keys: SFN_KEYS.to_string(),
        usage: SFN_USAGE.to_string(),
        billing: SFN_BILLING.to_string(),
    };
    let path = skillstar_core::infra::paths::config_dir().join("opencode_scrape.json");
    if let Ok(txt) = std::fs::read_to_string(&path)
        && let Ok(v) = serde_json::from_str::<Value>(&txt)
    {
        if let Some(s) = v
            .get("keys")
            .and_then(Value::as_str)
            .filter(|s| !s.trim().is_empty())
        {
            ids.keys = s.to_string();
        }
        if let Some(s) = v
            .get("usage")
            .and_then(Value::as_str)
            .filter(|s| !s.trim().is_empty())
        {
            ids.usage = s.to_string();
        }
        if let Some(s) = v
            .get("billing")
            .and_then(Value::as_str)
            .filter(|s| !s.trim().is_empty())
        {
            ids.billing = s.to_string();
        }
    }
    ids
}

fn user_agent() -> &'static str {
    "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36"
}

// ── Public entry point ──────────────────────────────────────────────────

pub async fn fetch(
    subscription_id: &str,
    cookies: &[CookieEntry],
    cookie_header: &str,
    plan_tier: Option<&str>,
) -> UsageResult<SubscriptionUsage> {
    let client = crate::http_client::usage_http_client()?;

    // 1. Fetch the Go page SSR to discover the workspace ID.
    let go_page = client
        .get(format!("{CONSOLE_BASE}/workspace/default/go"))
        .header(reqwest::header::COOKIE, cookie_header)
        .header(reqwest::header::USER_AGENT, user_agent())
        .header(reqwest::header::ACCEPT, "text/html")
        .header("Referer", CONSOLE_BASE)
        .send()
        .await
        .map_err(|e| UsageError::Fetcher(format!("OpenCode 页面请求失败：{}", e)))?;

    let status = go_page.status().as_u16();
    if status == 401 || status == 403 {
        return Err(UsageError::AuthRequired);
    }
    if !go_page.status().is_success() {
        return Err(UsageError::Fetcher(format!(
            "OpenCode 页面返回 {}，请检查 Cookie 是否有效。",
            status
        )));
    }

    let final_url = go_page.url().to_string();
    let html = go_page.text().await.unwrap_or_default();
    let workspace_id = extract_workspace_id_from_cookies(cookies)
        .or_else(|| extract_workspace_id_from_url(&final_url))
        .or_else(|| extract_workspace_id(&html))
        .ok_or_else(|| {
            UsageError::Fetcher(
                "无法从 OpenCode 页面提取 workspace ID。请确认已登录并重新粘贴 Cookie。".into(),
            )
        })?;

    // 2. Call server functions in parallel-ish sequence.
    let ids = server_fn_ids();
    let keys_resp = call_server_fn_get(&client, cookie_header, &ids.keys, &workspace_id).await?;
    let usage_resp = call_server_fn_get(&client, cookie_header, &ids.usage, &workspace_id).await?;
    let billing_resp =
        call_server_fn_get(&client, cookie_header, &ids.billing, &workspace_id).await?;

    let keys_text = keys_resp.text().await.unwrap_or_default();
    let usage_text = usage_resp.text().await.unwrap_or_default();
    let billing_text = billing_resp.text().await.unwrap_or_default();

    let keys_body = parse_server_fn_body(&keys_text);
    let usage_body = parse_server_fn_body(&usage_text);
    let billing_body = parse_server_fn_body(&billing_text);

    // We resolved the workspace but none of the server-function responses are
    // parseable — almost always means the server-fn signatures changed upstream.
    // Surface a clear, actionable error instead of silently reporting 0 usage.
    if keys_body.is_none() && usage_body.is_none() && billing_body.is_none() {
        return Err(UsageError::Fetcher(
            "无法解析 OpenCode 数据接口响应（服务端函数签名可能已更新）。请重新粘贴最新 Cookie；\
             若仍失败，可在 ~/.skillstar/config/opencode_scrape.json 配置新的 server-fn id，或等待 SkillStar 更新。"
                .into(),
        ));
    }

    let keys_body = keys_body.unwrap_or_default();
    let usage_body = usage_body.unwrap_or_default();
    let billing_body = billing_body.unwrap_or_default();

    let api_keys = parse_api_keys(&keys_body);
    let billing = parse_billing(&billing_body);
    let monthly = parse_monthly_usage(&usage_body);

    Ok(SubscriptionUsage {
        subscription_id: subscription_id.to_string(),
        fetched_at: Utc::now().timestamp(),
        plan_name: plan_tier
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string())
            .or_else(|| Some("OpenCode".to_string())),
        hourly: None,
        weekly: None,
        monthly,
        balance: billing,
        credits: Vec::new(),
        error: None,
        api_keys,
    })
}

// ── HTTP helpers ─────────────────────────────────────────────────────────

async fn call_server_fn_get(
    client: &reqwest::Client,
    cookie_header: &str,
    server_fn_id: &str,
    workspace_id: &str,
) -> UsageResult<reqwest::Response> {
    let url = format!(
        "{CONSOLE_BASE}/_server?id={server_fn_id}&args={}",
        urlencoding(&build_sfn_args(workspace_id))
    );
    client
        .get(&url)
        .header(reqwest::header::COOKIE, cookie_header)
        .header(reqwest::header::USER_AGENT, user_agent())
        .header(reqwest::header::ACCEPT, "*/*")
        .header(
            "Referer",
            format!("{CONSOLE_BASE}/workspace/{workspace_id}/go"),
        )
        .header("X-Server-Id", server_fn_id)
        .send()
        .await
        .map_err(|e| UsageError::Fetcher(format!("OpenCode _server 请求失败：{}", e)))
}

/// Build the serialized `args` JSON for a single-argument server function call.
fn build_sfn_args(workspace_id: &str) -> String {
    // SolidStart encodes args as `{"t":{"t":9,"i":0,"l":1,"a":[{"t":1,"s":"<workspace_id>"}],"o":0},"f":31,"m":[]}`
    let payload = serde_json::json!({
        "t": {
            "t": 9,
            "i": 0,
            "l": 1,
            "a": [{"t": 1, "s": workspace_id}],
            "o": 0
        },
        "f": 31,
        "m": []
    });
    payload.to_string()
}

// ── Response parsing ─────────────────────────────────────────────────────

/// Extract the JSON value assigned to `$R[0]` from a server function body.
fn parse_server_fn_body(body: &str) -> Option<Value> {
    // Server function responses are JS assignments:
    // `((self.$R=...)["server-fn:N"]=[],($R=>$R[0]=<DATA>)(...))`
    // We look for `$R[0]=` and parse the single JSON value after it.
    let marker = "$R[0]=";
    let pos = body.find(marker)?;
    let after = &body[pos + marker.len()..];
    // Find the value boundary in ONE linear pass (the response can be large;
    // the previous progressive-prefix approach was O(n²) and could stall on a
    // big payload), then parse exactly that slice once.
    let end = json_value_end(after)?;
    serde_json::from_str::<Value>(&after[..end]).ok()
}

/// Return the byte length of the single JSON value at the start of `s`
/// (after leading whitespace), in one linear pass. Handles objects, arrays,
/// strings (with escapes), and primitives. `None` if no complete value.
///
/// Replaces an O(n²) progressive-parse loop: for a `[...]`/`{...}` value the
/// old loop's first successful parse landed at the matching close delimiter,
/// which is exactly the boundary this returns — so behaviour is preserved.
fn json_value_end(s: &str) -> Option<usize> {
    let b = s.as_bytes();
    let mut i = 0;
    while i < b.len() && b[i].is_ascii_whitespace() {
        i += 1;
    }
    if i >= b.len() {
        return None;
    }
    match b[i] {
        b'[' | b'{' => {
            let mut depth = 0i32;
            let mut in_str = false;
            let mut esc = false;
            let mut j = i;
            while j < b.len() {
                let c = b[j];
                if in_str {
                    if esc {
                        esc = false;
                    } else if c == b'\\' {
                        esc = true;
                    } else if c == b'"' {
                        in_str = false;
                    }
                } else {
                    match c {
                        b'"' => in_str = true,
                        b'[' | b'{' => depth += 1,
                        b']' | b'}' => {
                            depth -= 1;
                            if depth == 0 {
                                return Some(j + 1);
                            }
                        }
                        _ => {}
                    }
                }
                j += 1;
            }
            None
        }
        b'"' => {
            let mut j = i + 1;
            let mut esc = false;
            while j < b.len() {
                let c = b[j];
                if esc {
                    esc = false;
                } else if c == b'\\' {
                    esc = true;
                } else if c == b'"' {
                    return Some(j + 1);
                }
                j += 1;
            }
            None
        }
        _ => {
            // primitive (number / true / false / null) — until a delimiter
            let mut j = i;
            while j < b.len() {
                let c = b[j];
                if c == b',' || c == b']' || c == b'}' || c.is_ascii_whitespace() {
                    break;
                }
                j += 1;
            }
            (j > i).then_some(j)
        }
    }
}

/// Extract workspace ID from SSR payload.
fn extract_workspace_id(html: &str) -> Option<String> {
    extract_workspace_id_from_marker(html, r#"session.get["wrk_"#)
        .or_else(|| extract_workspace_id_from_marker(html, r#"/workspace/wrk_"#))
        .or_else(|| extract_workspace_id_from_marker(html, r#"workspace/wrk_"#))
        .or_else(|| extract_workspace_id_from_json_field(html, "workspaceID"))
        .or_else(|| extract_workspace_id_from_json_field(html, "workspaceId"))
        .or_else(|| extract_workspace_id_from_json_field(html, "workspace_id"))
}

fn extract_workspace_id_from_cookies(cookies: &[CookieEntry]) -> Option<String> {
    cookies
        .iter()
        .filter_map(|cookie| cookie.source_url.as_deref())
        .find_map(extract_workspace_id_from_url)
}

fn extract_workspace_id_from_url(url: &str) -> Option<String> {
    let marker = "/workspace/";
    let start = url.find(marker)? + marker.len();
    let rest = &url[start..];
    let candidate = rest.split(['/', '?', '#']).next()?.trim();
    if candidate == "default" || candidate.starts_with("wrk_") {
        Some(candidate.to_string())
    } else {
        None
    }
}

fn extract_workspace_id_from_marker(text: &str, marker: &str) -> Option<String> {
    let start = text.find(marker)? + marker.len();
    let rest = &text[start..];
    let id = rest
        .chars()
        .take_while(|ch| ch.is_ascii_alphanumeric() || *ch == '_' || *ch == '-')
        .collect::<String>();
    normalize_workspace_id(&id)
}

fn extract_workspace_id_from_json_field(text: &str, field: &str) -> Option<String> {
    let marker = format!(r#""{field}":"wrk_"#);
    extract_workspace_id_from_marker(text, &marker)
}

fn normalize_workspace_id(id: &str) -> Option<String> {
    let id = id.trim_matches(|ch: char| ch == '"' || ch == '\\' || ch == '/' || ch.is_whitespace());
    if id.is_empty() {
        return None;
    }
    if id.starts_with("wrk_") {
        Some(id.to_string())
    } else if id
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || ch == '_' || ch == '-')
    {
        Some(format!("wrk_{id}"))
    } else {
        None
    }
}

/// Parse API keys from the keys server function response.
fn parse_api_keys(body: &Value) -> Vec<OpenCodeApiKey> {
    let arr = match body.as_array() {
        Some(a) => a,
        None => return Vec::new(),
    };
    arr.iter()
        .filter_map(|item| {
            let id = item.get("id")?.as_str()?;
            let name = item.get("name")?.as_str()?;
            let display = item.get("keyDisplay")?.as_str()?;
            let email = item.get("email").and_then(|v| v.as_str());
            Some(OpenCodeApiKey {
                id: id.to_string(),
                name: name.to_string(),
                display: display.to_string(),
                email: email.map(|s| s.to_string()),
            })
        })
        .collect()
}

/// Parse billing info from the billing server function response.
fn parse_billing(body: &Value) -> Option<MonetaryBalance> {
    let balance_raw = body.get("balance")?.as_f64()?;
    // balance is in 1e8 units (e.g. 500000000 = $5.00)
    let balance = balance_raw / COST_DIVISOR;
    if balance <= 0.0 && balance_raw == 0.0 {
        return None;
    }
    Some(MonetaryBalance {
        currency: "USD".to_string(),
        total: balance,
        granted: 0.0,
        topped_up: 0.0,
    })
}

/// Parse the monthly aggregated usage from the usage server function response.
fn parse_monthly_usage(body: &Value) -> Option<UsageWindow> {
    let usage_arr = body.get("usage")?.as_array()?;
    if usage_arr.is_empty() {
        return None;
    }

    // Sum all totalCost values (in 1e8 units) for the current month.
    let mut total_cost: f64 = 0.0;
    let mut has_data = false;
    for entry in usage_arr {
        if let Some(cost) = entry.get("totalCost").and_then(|v| v.as_f64()) {
            total_cost += cost;
            has_data = true;
        }
    }

    if !has_data {
        return None;
    }

    let total_dollars = total_cost / COST_DIVISOR;

    // We don't have a monthly limit from this API, so we report usage-only.
    Some(UsageWindow {
        label: "本月".to_string(),
        used: (total_dollars * 100.0).round() as i64, // cents
        total: None,
        percent: None,
        reset_at: next_month_start_epoch(),
        breakdown: Vec::new(),
    })
}

/// Epoch seconds for the first moment of next month (UTC).
fn next_month_start_epoch() -> Option<i64> {
    let now = Utc::now();
    let (y, m) = if now.month() == 12 {
        (now.year() + 1, 1)
    } else {
        (now.year(), now.month() + 1)
    };
    chrono::NaiveDate::from_ymd_opt(y, m, 1)
        .and_then(|d| d.and_hms_opt(0, 0, 0))
        .map(|dt| dt.and_utc().timestamp())
}

// ── Misc helpers ─────────────────────────────────────────────────────────

fn urlencoding(s: &str) -> String {
    let mut out = String::with_capacity(s.len() * 3);
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char)
            }
            _ => out.push_str(&format!("%{:02X}", b)),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_array_value_with_trailing_js() {
        // Realistic server-fn shape: `$R[0]=<JSON>` followed by more JS.
        let body = r#"((self.$R=self.$R||[])["server-fn:0"]=[],($R=>$R[0]=[{"id":"k1","name":"key"}])(self.$R))"#;
        let v = parse_server_fn_body(body).expect("should parse the array");
        let arr = v.as_array().expect("array");
        assert_eq!(arr.len(), 1);
        assert_eq!(arr[0]["id"], "k1");
    }

    #[test]
    fn handles_nested_brackets_and_strings_containing_delimiters() {
        // Strings carrying `]`/`}` and nested structures must not end the scan early.
        let body = r#"prefix $R[0]={"a":[1,2,{"b":"]}]"}],"c":"x"} suffix);"#;
        let v = parse_server_fn_body(body).expect("should parse the object");
        assert_eq!(v["a"][2]["b"], "]}]");
        assert_eq!(v["c"], "x");
    }

    #[test]
    fn handles_escaped_quotes_in_strings() {
        let body = r#"$R[0]=["he said \"hi\" ]"]"#;
        let v = parse_server_fn_body(body).expect("parse");
        assert_eq!(v[0], r#"he said "hi" ]"#);
    }

    #[test]
    fn returns_none_when_marker_absent() {
        assert!(parse_server_fn_body("no marker here []").is_none());
    }

    #[test]
    fn json_value_end_matches_value_boundary() {
        assert_eq!(json_value_end("[1,2,3] trailing"), Some(7));
        assert_eq!(json_value_end(r#"{"k":1} more"#), Some(7));
        assert_eq!(json_value_end(r#""str" more"#), Some(5));
        assert_eq!(json_value_end("  [ ] x"), Some(5));
        assert_eq!(json_value_end("true,next"), Some(4));
        assert_eq!(json_value_end("[unterminated"), None);
    }

    #[test]
    fn large_payload_parses_without_quadratic_blowup() {
        // A big array would have triggered up to ~n full-parse attempts (O(n²))
        // in the old code. The linear scan handles it in one pass.
        let inner: String = (0..5_000).map(|i| format!(r#"{{"i":{i}}},"#)).collect();
        let body = format!("$R[0]=[{}{{\"i\":5000}}]", inner);
        let v = parse_server_fn_body(&body).expect("parse large array");
        assert_eq!(v.as_array().unwrap().len(), 5_001);
    }
}
