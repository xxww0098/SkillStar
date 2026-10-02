//! Public Responses body for a Codex loopback post.
//!
//! The CLI still calls `/backend-api/codex/responses`. The bytes appended to
//! the configured origin follow the public Responses route: fields that route
//! rejects are dropped, a system message becomes a developer message, `store`
//! is false, `stream` is true, and the cache key stays in the body. This
//! module does not pick a host, read an account, or add an affinity header.

use serde_json::{Map, Value};

const FAST_SUFFIX: &str = "-fast";
/// Cache key when the caller did not send one. This install's own label.
const STABLE_SESSION: &str = "skillstar-chatgpt";
const USAGE_PAGE: &str = "https://chatgpt.com/settings/usage";
const QUOTA_CODE: &str = "subscription_sharing_usage_limit_exceeded";

/// Fields the public Responses route rejects. A caller-supplied `service_tier`
/// is one of them; a Fast picker row may put `priority` back afterwards.
const UNSUPPORTED: &[&str] = &[
    "background",
    "conversation",
    "max_output_tokens",
    "max_tool_calls",
    "metadata",
    "moderation",
    "multi_agent",
    "prompt",
    "prompt_cache_retention",
    "prompt_cache_options",
    "safety_identifier",
    "temperature",
    "top_logprobs",
    "top_p",
    "truncation",
    "user",
    "previous_response_id",
    "service_tier",
];

/// Rewrite a JSON object into the public Responses body. Anything else is
/// returned unchanged.
pub(crate) fn shape_responses(body: &[u8]) -> Vec<u8> {
    let Ok(Value::Object(mut map)) = serde_json::from_slice(body) else {
        return body.to_vec();
    };
    for field in UNSUPPORTED {
        map.remove(*field);
    }
    if let Some(model) = map.get("model").and_then(Value::as_str).map(str::to_string) {
        let peeled = peel_fast(&model);
        if peeled.fast {
            map.insert(
                "service_tier".to_string(),
                Value::String("priority".to_string()),
            );
        }
        map.insert("model".to_string(), Value::String(peeled.model));
    }
    if let Some(Value::Array(items)) = map.get_mut("input") {
        for item in items.iter_mut() {
            rewrite_system(item);
        }
    }
    map.insert("store".to_string(), Value::Bool(false));
    map.insert("stream".to_string(), Value::Bool(true));
    apply_cache(&mut map);
    serde_json::to_vec(&Value::Object(map)).unwrap_or_else(|_| body.to_vec())
}

/// A 429 whose code is the plan limit, worded with the settings page.
/// Every other body returns `None` and is forwarded as it arrived.
pub(crate) fn quota_reply(status: u16, body: &[u8]) -> Option<Vec<u8>> {
    if status != 429 {
        return None;
    }
    let value: Value = serde_json::from_slice(body).ok()?;
    let code = value
        .pointer("/error/code")
        .or_else(|| value.get("code"))
        .and_then(Value::as_str)?;
    if code != QUOTA_CODE {
        return None;
    }
    let detail = value
        .pointer("/error/message")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|text| !text.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| format!("ChatGPT plan usage limit reached (HTTP {status})"));
    let message = format!("{detail} — manage usage at {USAGE_PAGE}");
    serde_json::to_vec(&serde_json::json!({
        "error": { "message": message, "code": QUOTA_CODE }
    }))
    .ok()
}

fn peel_fast(model_id: &str) -> Peeled {
    let lower = model_id.to_ascii_lowercase();
    if !lower.ends_with(FAST_SUFFIX) {
        return Peeled {
            model: model_id.to_string(),
            fast: false,
        };
    }
    let base = &model_id[..model_id.len() - FAST_SUFFIX.len()];
    if fast_tier(base) {
        Peeled {
            model: base.to_string(),
            fast: true,
        }
    } else {
        Peeled {
            model: model_id.to_string(),
            fast: false,
        }
    }
}

/// Static rows that offer a `-fast` picker id. A slug outside this list keeps
/// the suffix; this route does not invent a Fast tier for it.
fn fast_tier(model_id: &str) -> bool {
    matches!(
        model_id.trim().to_ascii_lowercase().as_str(),
        "gpt-6.1-sol"
            | "gpt-6-astra"
            | "gpt-6-sol"
            | "gpt-6-luna"
            | "gpt-5.6-sol"
            | "gpt-5.6-terra"
            | "gpt-5.6-luna"
            | "gpt-5.5"
    )
}

struct Peeled {
    model: String,
    fast: bool,
}

fn rewrite_system(item: &mut Value) {
    let Some(obj) = item.as_object() else {
        return;
    };
    if obj.get("role").and_then(Value::as_str) != Some("system") {
        return;
    }
    if let Some(kind) = obj.get("type")
        && kind.as_str() != Some("message")
    {
        return;
    }
    let Some(obj) = item.as_object_mut() else {
        return;
    };
    obj.insert("role".to_string(), Value::String("developer".to_string()));
}

fn apply_cache(map: &mut Map<String, Value>) {
    let from_key = map
        .get("prompt_cache_key")
        .and_then(Value::as_str)
        .and_then(cache_session_id);
    let from_session = map
        .get("session_id")
        .and_then(Value::as_str)
        .and_then(cache_session_id);
    let key = from_key
        .or(from_session)
        .unwrap_or_else(|| STABLE_SESSION.to_string());
    map.insert("prompt_cache_key".to_string(), Value::String(key));
    map.remove("session_id");
}

fn cache_session_id(key: &str) -> Option<String> {
    let cleaned: String = key
        .trim()
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || matches!(ch, '.' | '_' | ':' | '-') {
                ch
            } else {
                '-'
            }
        })
        .collect();
    if cleaned.is_empty() {
        None
    } else {
        Some(cleaned.chars().take(64).collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_unknown_fast_suffix_stays_on_the_model_id() {
        let body = br#"{"model":"gpt-unknown-fast","service_tier":"flex"}"#;
        let value: Value = serde_json::from_slice(&shape_responses(body)).unwrap();
        assert_eq!(value["model"], "gpt-unknown-fast");
        assert!(value.get("service_tier").is_none());
    }

    #[test]
    fn a_dirty_session_id_becomes_the_cache_key_when_none_was_sent() {
        let body = br#"{"model":"m","session_id":"  turn/9  "}"#;
        let value: Value = serde_json::from_slice(&shape_responses(body)).unwrap();
        assert_eq!(value["prompt_cache_key"], "turn-9");
        assert!(value.get("session_id").is_none());
    }

    #[test]
    fn a_non_object_body_is_not_rewritten() {
        let body = b"[1,2]";
        assert_eq!(shape_responses(body), body);
    }

    #[test]
    fn only_the_plan_limit_is_rewritten() {
        let other = br#"{"error":{"code":"rate_limit_exceeded","message":"later"}}"#;
        assert!(quota_reply(429, other).is_none());
        assert!(quota_reply(400, br#"{"error":{"code":"subscription_sharing_usage_limit_exceeded"}}"#).is_none());
        let replaced = quota_reply(
            429,
            br#"{"code":"subscription_sharing_usage_limit_exceeded"}"#,
        )
        .expect("plan limit");
        let value: Value = serde_json::from_slice(&replaced).unwrap();
        assert_eq!(
            value["error"]["message"],
            "ChatGPT plan usage limit reached (HTTP 429) — manage usage at https://chatgpt.com/settings/usage"
        );
    }
}
