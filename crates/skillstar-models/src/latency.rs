//! Provider latency testing.
//!
//! Probes provider base URLs for reachability:
//! - OpenAI-compatible bases: `GET {base}/models` with Bearer auth
//! - Anthropic-compatible bases (path contains `/anthropic`): `POST {base}/messages`
//!   with a minimal Messages API payload (avoids false 404 on `GET .../models`)

use serde::{Deserialize, Serialize};
use skillstar_core::infra::http_client::probe_http_client;
use tokio::time::Instant;

/// Default timeout for latency tests (10 seconds).
const DEFAULT_TIMEOUT_MS: u64 = 10_000;

/// Default model for Anthropic-style reachability probes.
const ANTHROPIC_PROBE_MODEL: &str = "deepseek-chat";

/// Result of a single provider latency test.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LatencyResult {
    pub provider_id: String,
    pub app_id: String,
    /// Measured latency in milliseconds. `None` for timeout or network error.
    pub latency_ms: Option<u64>,
    /// `"ok"` | `"timeout"` | `"error"`
    pub status: String,
    /// Error description when status is `"error"`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error_message: Option<String>,
    /// ISO 8601 timestamp of when the test was performed.
    pub tested_at: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum EndpointProbeKind {
    OpenAiModelsList,
    AnthropicMessages,
}

/// Whether the base URL should use Anthropic Messages probing instead of OpenAI `/models`.
pub fn is_anthropic_compatible_url(base_url: &str) -> bool {
    let lower = base_url.trim().to_lowercase();
    lower.contains("/anthropic")
}

fn probe_kind_for_base_url(base_url: &str) -> EndpointProbeKind {
    if is_anthropic_compatible_url(base_url) {
        EndpointProbeKind::AnthropicMessages
    } else {
        EndpointProbeKind::OpenAiModelsList
    }
}

fn openai_models_probe_url(base_url: &str) -> String {
    let trimmed = base_url.trim().trim_end_matches('/');
    if trimmed.is_empty() {
        return String::new();
    }
    if trimmed.ends_with("/models") {
        trimmed.to_string()
    } else {
        format!("{trimmed}/models")
    }
}

fn anthropic_messages_probe_url(base_url: &str) -> String {
    let trimmed = base_url.trim().trim_end_matches('/');
    if trimmed.is_empty() {
        return String::new();
    }
    if trimmed.ends_with("/messages") {
        trimmed.to_string()
    } else {
        format!("{trimmed}/messages")
    }
}

struct HttpProbeOutcome {
    latency_ms: Option<u64>,
    status_code: Option<u16>,
    error_message: Option<String>,
    is_timeout: bool,
    network_error: Option<String>,
}

fn is_auth_http_status(status_code: u16) -> bool {
    status_code == 401 || status_code == 403
}

fn endpoint_error_for_status(status_code: u16) -> Option<String> {
    if status_code < 400 {
        None
    } else if is_auth_http_status(status_code) {
        Some("鉴权失败，请检查 API Key".to_string())
    } else {
        Some(format!("HTTP {status_code}"))
    }
}

fn build_probe_client(timeout: std::time::Duration) -> Result<reqwest::Client, String> {
    probe_http_client(timeout).map_err(|e| e.to_string())
}

fn format_network_probe_error(err: &reqwest::Error) -> String {
    if err.is_timeout() {
        "请求超时，请检查网络或在设置中启用代理".to_string()
    } else if err.is_connect() {
        "无法连接服务器，请在设置 → 代理 中启用本地代理".to_string()
    } else {
        err.to_string()
    }
}

async fn send_base_url_probe(
    client: &reqwest::Client,
    base_url: &str,
    api_key: Option<&str>,
) -> HttpProbeOutcome {
    let start = Instant::now();
    let key = api_key.unwrap_or("");
    let result = send_reachability_probe(client, base_url, key).await;

    match result {
        Ok(response) => {
            let elapsed_ms = start.elapsed().as_millis() as u64;
            let status_code = response.status().as_u16();
            HttpProbeOutcome {
                latency_ms: Some(elapsed_ms),
                status_code: Some(status_code),
                error_message: endpoint_error_for_status(status_code),
                is_timeout: false,
                network_error: None,
            }
        }
        Err(e) => HttpProbeOutcome {
            latency_ms: None,
            status_code: None,
            error_message: None,
            is_timeout: e.is_timeout(),
            network_error: Some(format_network_probe_error(&e)),
        },
    }
}

/// Test the latency of a provider by probing its base URL.
pub async fn test_provider_latency(
    provider_id: &str,
    app_id: &str,
    base_url: &str,
    api_key: &str,
    timeout_ms: Option<u64>,
) -> LatencyResult {
    let timeout = std::time::Duration::from_millis(timeout_ms.unwrap_or(DEFAULT_TIMEOUT_MS));
    let tested_at = chrono::Utc::now().to_rfc3339();

    let client = match build_probe_client(timeout) {
        Ok(c) => c,
        Err(e) => {
            return LatencyResult {
                provider_id: provider_id.to_string(),
                app_id: app_id.to_string(),
                latency_ms: None,
                status: "error".to_string(),
                error_message: Some(format!("Failed to build HTTP client: {e}")),
                tested_at,
            };
        }
    };

    let outcome = send_base_url_probe(&client, base_url, Some(api_key)).await;

    if let Some(net) = outcome.network_error {
        return LatencyResult {
            provider_id: provider_id.to_string(),
            app_id: app_id.to_string(),
            latency_ms: None,
            status: if outcome.is_timeout {
                "timeout".to_string()
            } else {
                "error".to_string()
            },
            error_message: Some(net),
            tested_at,
        };
    }

    let status_code = outcome.status_code.unwrap_or(0);
    if status_code < 400 || is_auth_http_status(status_code) {
        LatencyResult {
            provider_id: provider_id.to_string(),
            app_id: app_id.to_string(),
            latency_ms: outcome.latency_ms,
            status: "ok".to_string(),
            error_message: outcome.error_message,
            tested_at,
        }
    } else {
        LatencyResult {
            provider_id: provider_id.to_string(),
            app_id: app_id.to_string(),
            latency_ms: outcome.latency_ms,
            status: "error".to_string(),
            error_message: outcome.error_message,
            tested_at,
        }
    }
}

/// Result of probing a single API endpoint URL.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EndpointLatencyResult {
    pub url: String,
    pub latency_ms: Option<u64>,
    pub status: Option<u16>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

async fn probe_endpoint_once(
    url: &str,
    api_key: Option<&str>,
    timeout: std::time::Duration,
) -> EndpointLatencyResult {
    let raw = url.trim().to_string();
    if raw.is_empty() {
        return EndpointLatencyResult {
            url: raw,
            latency_ms: None,
            status: None,
            error: Some("URL 不能为空".to_string()),
        };
    }

    if url::Url::parse(&raw).is_err() {
        return EndpointLatencyResult {
            url: raw,
            latency_ms: None,
            status: None,
            error: Some("URL 格式无效".to_string()),
        };
    }

    let client = match build_probe_client(timeout) {
        Ok(c) => c,
        Err(e) => {
            return EndpointLatencyResult {
                url: raw,
                latency_ms: None,
                status: None,
                error: Some(format!("Failed to build HTTP client: {e}")),
            };
        }
    };

    let outcome = send_base_url_probe(&client, &raw, api_key).await;

    if let Some(net) = outcome.network_error {
        return EndpointLatencyResult {
            url: raw,
            latency_ms: None,
            status: None,
            error: Some(net),
        };
    }

    EndpointLatencyResult {
        url: raw,
        latency_ms: outcome.latency_ms,
        status: outcome.status_code,
        error: outcome.error_message,
    }
}

/// Probe multiple provider endpoints in parallel.
pub async fn test_endpoints_latency(
    urls: Vec<String>,
    api_key: Option<String>,
    timeout_ms: Option<u64>,
) -> Vec<EndpointLatencyResult> {
    if urls.is_empty() {
        return vec![];
    }

    let timeout = std::time::Duration::from_millis(timeout_ms.unwrap_or(DEFAULT_TIMEOUT_MS));
    let key = api_key.filter(|k| !k.trim().is_empty());

    let mut results = Vec::with_capacity(urls.len());
    for url in urls {
        results.push(probe_endpoint_once(&url, key.as_deref(), timeout).await);
    }
    results
}

/// Reachability probe used by `test_provider_connection` when `model` is empty.
pub async fn send_reachability_probe(
    client: &reqwest::Client,
    base_url: &str,
    api_key: &str,
) -> Result<reqwest::Response, reqwest::Error> {
    let kind = probe_kind_for_base_url(base_url);
    let key = (!api_key.trim().is_empty()).then_some(api_key);

    match kind {
        EndpointProbeKind::OpenAiModelsList => {
            let url = openai_models_probe_url(base_url);
            let mut request = client.get(&url);
            if let Some(k) = key {
                request = request.header("Authorization", format!("Bearer {k}"));
            }
            request.send().await
        }
        EndpointProbeKind::AnthropicMessages => {
            let url = anthropic_messages_probe_url(base_url);
            let body = serde_json::json!({
                "model": ANTHROPIC_PROBE_MODEL,
                "max_tokens": 1,
                "messages": [{"role": "user", "content": "hi"}]
            });
            let mut request = client
                .post(&url)
                .header("content-type", "application/json")
                .header("anthropic-version", "2023-06-01")
                .json(&body);
            if let Some(k) = key {
                request = request
                    .header("x-api-key", k)
                    .header("Authorization", format!("Bearer {k}"));
            }
            request.send().await
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    fn arb_status() -> impl Strategy<Value = String> {
        prop_oneof![
            Just("ok".to_string()),
            Just("timeout".to_string()),
            Just("error".to_string()),
        ]
    }

    fn arb_consistent_latency_result() -> impl Strategy<Value = LatencyResult> {
        (
            "[a-zA-Z0-9_-]{1,32}",
            prop_oneof![Just("claude".to_string()), Just("codex".to_string())],
            arb_status(),
            proptest::option::of("[a-zA-Z0-9 ]{1,64}"),
        )
            .prop_map(|(provider_id, app_id, status, error_message)| {
                let latency_ms = match status.as_str() {
                    "ok" => Some(1),
                    _ => None,
                };
                LatencyResult {
                    provider_id,
                    app_id,
                    latency_ms,
                    status,
                    error_message,
                    tested_at: "2025-01-01T00:00:00+00:00".to_string(),
                }
            })
            .prop_flat_map(|result| {
                if result.status == "ok" {
                    (1u64..=30_000u64)
                        .prop_map(move |ms| {
                            let mut r = result.clone();
                            r.latency_ms = Some(ms);
                            r
                        })
                        .boxed()
                } else {
                    Just(result).boxed()
                }
            })
    }

    fn arb_arbitrary_latency_result() -> impl Strategy<Value = LatencyResult> {
        (
            "[a-zA-Z0-9_-]{1,32}",
            prop_oneof![Just("claude".to_string()), Just("codex".to_string())],
            arb_status(),
            proptest::option::of(1u64..=30_000u64),
            proptest::option::of("[a-zA-Z0-9 ]{1,64}"),
        )
            .prop_map(|(provider_id, app_id, status, latency_ms, error_message)| {
                LatencyResult {
                    provider_id,
                    app_id,
                    latency_ms,
                    status,
                    error_message,
                    tested_at: "2025-01-01T00:00:00+00:00".to_string(),
                }
            })
    }

    fn check_latency_consistency(result: &LatencyResult) -> Result<(), String> {
        match result.status.as_str() {
            "ok" => match result.latency_ms {
                Some(ms) if ms > 0 => Ok(()),
                Some(0) => Err("status is 'ok' but latency_ms is 0 (must be > 0)".to_string()),
                None => Err("status is 'ok' but latency_ms is None".to_string()),
                Some(ms) => Err(format!("status is 'ok' but latency_ms is {ms}")),
            },
            "timeout" | "error" => match result.latency_ms {
                None => Ok(()),
                Some(ms) => Err(format!(
                    "status is '{}' but latency_ms is Some({ms})",
                    result.status
                )),
            },
            other => Err(format!("Unknown status: '{other}'")),
        }
    }

    proptest! {
        #[test]
        fn prop_latency_result_consistency_valid(result in arb_consistent_latency_result()) {
            prop_assert!(check_latency_consistency(&result).is_ok());
        }

        #[test]
        fn prop_latency_result_consistency_invariant_detection(
            result in arb_arbitrary_latency_result()
        ) {
            let check = check_latency_consistency(&result);
            match result.status.as_str() {
                "ok" => {
                    if let Some(ms) = result.latency_ms {
                        if ms > 0 {
                            prop_assert!(check.is_ok());
                        } else {
                            prop_assert!(check.is_err());
                        }
                    } else {
                        prop_assert!(check.is_err());
                    }
                }
                "timeout" | "error" => {
                    if result.latency_ms.is_none() {
                        prop_assert!(check.is_ok());
                    } else {
                        prop_assert!(check.is_err());
                    }
                }
                _ => prop_assert!(check.is_err()),
            }
        }
    }

    #[test]
    fn detects_anthropic_compatible_urls() {
        assert!(is_anthropic_compatible_url(
            "https://api.deepseek.com/anthropic"
        ));
        assert!(!is_anthropic_compatible_url("https://api.deepseek.com/v1"));
    }

    #[test]
    fn builds_anthropic_messages_probe_url() {
        assert_eq!(
            anthropic_messages_probe_url("https://api.deepseek.com/anthropic"),
            "https://api.deepseek.com/anthropic/messages"
        );
    }

    #[test]
    fn builds_openai_models_probe_url() {
        assert_eq!(
            openai_models_probe_url("https://api.deepseek.com/v1"),
            "https://api.deepseek.com/v1/models"
        );
    }

    #[test]
    fn test_latency_result_serialization() {
        let result = LatencyResult {
            provider_id: "p1".to_string(),
            app_id: "claude".to_string(),
            latency_ms: Some(150),
            status: "ok".to_string(),
            error_message: None,
            tested_at: "2025-01-01T00:00:00+00:00".to_string(),
        };
        let json = serde_json::to_string(&result).unwrap();
        let parsed: LatencyResult = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.provider_id, "p1");
        assert_eq!(parsed.latency_ms, Some(150));
        assert_eq!(parsed.status, "ok");
    }
}
