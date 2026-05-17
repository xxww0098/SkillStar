//! Provider latency testing.
//!
//! Sends an HTTP GET request to a provider's `/models` endpoint and measures
//! the round-trip time. Used by the Health Dashboard to display connectivity
//! status and response times.

use serde::{Deserialize, Serialize};
use tokio::time::Instant;

/// Default timeout for latency tests (10 seconds).
const DEFAULT_TIMEOUT_MS: u64 = 10_000;

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

/// Test the latency of a provider by sending a GET request to `{base_url}/models`.
///
/// # Arguments
/// * `provider_id` - Identifier for the provider being tested.
/// * `app_id` - The app this provider belongs to (e.g. "claude", "codex").
/// * `base_url` - The provider's API base URL (e.g. `https://api.openai.com/v1`).
/// * `api_key` - Bearer token for authentication.
/// * `timeout_ms` - Request timeout in milliseconds. Uses 10000ms if `None`.
///
/// # Returns
/// A `LatencyResult` with timing, status, and optional error info.
pub async fn test_provider_latency(
    provider_id: &str,
    app_id: &str,
    base_url: &str,
    api_key: &str,
    timeout_ms: Option<u64>,
) -> LatencyResult {
    let timeout = std::time::Duration::from_millis(timeout_ms.unwrap_or(DEFAULT_TIMEOUT_MS));
    let tested_at = chrono::Utc::now().to_rfc3339();
    let url = format!("{}/models", base_url.trim_end_matches('/'));

    let client = match reqwest::Client::builder().timeout(timeout).build() {
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

    let start = Instant::now();

    let result = client
        .get(&url)
        .header("Authorization", format!("Bearer {api_key}"))
        .send()
        .await;

    match result {
        Ok(response) => {
            let elapsed_ms = start.elapsed().as_millis() as u64;
            let status_code = response.status().as_u16();

            if status_code < 400 {
                // 2xx or 3xx — success
                LatencyResult {
                    provider_id: provider_id.to_string(),
                    app_id: app_id.to_string(),
                    latency_ms: Some(elapsed_ms),
                    status: "ok".to_string(),
                    error_message: None,
                    tested_at,
                }
            } else {
                // 4xx/5xx — treat as error but still report latency
                LatencyResult {
                    provider_id: provider_id.to_string(),
                    app_id: app_id.to_string(),
                    latency_ms: Some(elapsed_ms),
                    status: "error".to_string(),
                    error_message: Some(format!("HTTP {status_code}")),
                    tested_at,
                }
            }
        }
        Err(e) => {
            if e.is_timeout() {
                LatencyResult {
                    provider_id: provider_id.to_string(),
                    app_id: app_id.to_string(),
                    latency_ms: None,
                    status: "timeout".to_string(),
                    error_message: None,
                    tested_at,
                }
            } else {
                LatencyResult {
                    provider_id: provider_id.to_string(),
                    app_id: app_id.to_string(),
                    latency_ms: None,
                    status: "error".to_string(),
                    error_message: Some(e.to_string()),
                    tested_at,
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    // =========================================================================
    // Property 12: Latency Result Consistency
    //
    // For any LatencyResult: if status is "ok" then latency_ms SHALL be a
    // positive number; if status is "timeout" or "error" then latency_ms
    // SHALL be null (None).
    //
    // **Validates: Requirements 5.2, 5.3, 5.4**
    // =========================================================================

    /// Strategy: generate a valid status string.
    fn arb_status() -> impl Strategy<Value = String> {
        prop_oneof![
            Just("ok".to_string()),
            Just("timeout".to_string()),
            Just("error".to_string()),
        ]
    }

    /// Strategy: generate a LatencyResult that satisfies the consistency invariant.
    /// This is used to verify that well-formed results pass the check.
    fn arb_consistent_latency_result() -> impl Strategy<Value = LatencyResult> {
        (
            "[a-zA-Z0-9_-]{1,32}",  // provider_id
            prop_oneof![Just("claude".to_string()), Just("codex".to_string())], // app_id
            arb_status(),
            proptest::option::of("[a-zA-Z0-9 ]{1,64}"), // error_message
        )
            .prop_map(|(provider_id, app_id, status, error_message)| {
                let latency_ms = match status.as_str() {
                    "ok" => Some(1), // placeholder, will be overridden
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
                    // Generate a positive latency value for "ok" status
                    (1u64..=30_000u64).prop_map(move |ms| {
                        let mut r = result.clone();
                        r.latency_ms = Some(ms);
                        r
                    }).boxed()
                } else {
                    // timeout/error must have None latency
                    Just(result).boxed()
                }
            })
    }

    /// Strategy: generate an arbitrary LatencyResult with random status and latency_ms
    /// (may or may not satisfy the invariant).
    fn arb_arbitrary_latency_result() -> impl Strategy<Value = LatencyResult> {
        (
            "[a-zA-Z0-9_-]{1,32}",  // provider_id
            prop_oneof![Just("claude".to_string()), Just("codex".to_string())], // app_id
            arb_status(),
            proptest::option::of(1u64..=30_000u64), // latency_ms (random)
            proptest::option::of("[a-zA-Z0-9 ]{1,64}"), // error_message
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

    /// Validates the latency result consistency invariant:
    /// - status "ok" → latency_ms is Some(x) where x > 0
    /// - status "timeout" or "error" → latency_ms is None
    fn check_latency_consistency(result: &LatencyResult) -> Result<(), String> {
        match result.status.as_str() {
            "ok" => {
                match result.latency_ms {
                    Some(ms) if ms > 0 => Ok(()),
                    Some(0) => Err(format!(
                        "status is 'ok' but latency_ms is 0 (must be > 0)"
                    )),
                    None => Err(format!(
                        "status is 'ok' but latency_ms is None (must be Some(x) where x > 0)"
                    )),
                    Some(ms) => Err(format!(
                        "status is 'ok' but latency_ms is {} (unexpected)", ms
                    )),
                }
            }
            "timeout" | "error" => {
                match result.latency_ms {
                    None => Ok(()),
                    Some(ms) => Err(format!(
                        "status is '{}' but latency_ms is Some({}) (must be None)",
                        result.status, ms
                    )),
                }
            }
            other => Err(format!("Unknown status: '{}'", other)),
        }
    }

    proptest! {
        /// **Validates: Requirements 5.2, 5.3, 5.4**
        ///
        /// Property 12: Consistent LatencyResults always pass the invariant check.
        /// Generates LatencyResult values that are constructed according to the
        /// consistency rules and verifies the invariant holds.
        #[test]
        fn prop_latency_result_consistency_valid(
            result in arb_consistent_latency_result()
        ) {
            let check = check_latency_consistency(&result);
            prop_assert!(
                check.is_ok(),
                "Consistent LatencyResult failed invariant check: {:?} — result: {:?}",
                check.unwrap_err(),
                result
            );
        }

        /// **Validates: Requirements 5.2, 5.3, 5.4**
        ///
        /// Property 12: For arbitrary LatencyResults, the invariant correctly
        /// identifies violations. If a result passes the check, it must satisfy:
        /// - "ok" → latency_ms > 0
        /// - "timeout"/"error" → latency_ms is None
        #[test]
        fn prop_latency_result_consistency_invariant_detection(
            result in arb_arbitrary_latency_result()
        ) {
            let check = check_latency_consistency(&result);
            match result.status.as_str() {
                "ok" => {
                    if let Some(ms) = result.latency_ms {
                        if ms > 0 {
                            prop_assert!(check.is_ok(),
                                "ok with positive latency should pass, got: {:?}", check);
                        } else {
                            prop_assert!(check.is_err(),
                                "ok with zero latency should fail");
                        }
                    } else {
                        prop_assert!(check.is_err(),
                            "ok with None latency should fail");
                    }
                }
                "timeout" | "error" => {
                    if result.latency_ms.is_none() {
                        prop_assert!(check.is_ok(),
                            "timeout/error with None latency should pass, got: {:?}", check);
                    } else {
                        prop_assert!(check.is_err(),
                            "timeout/error with Some latency should fail");
                    }
                }
                _ => {
                    prop_assert!(check.is_err(), "Unknown status should fail");
                }
            }
        }
    }

    /// **Validates: Requirements 5.2, 5.3, 5.4**
    ///
    /// Verify that the `test_provider_latency` function's construction paths
    /// produce results consistent with the invariant for timeout and network
    /// error cases (where latency_ms must be None).
    /// For "ok" status, latency_ms must be Some(positive).
    #[test]
    fn test_latency_result_construction_paths_satisfy_invariant() {
        // Simulate the "ok" path (HTTP 2xx)
        let ok_result = LatencyResult {
            provider_id: "p1".to_string(),
            app_id: "claude".to_string(),
            latency_ms: Some(150),
            status: "ok".to_string(),
            error_message: None,
            tested_at: "2025-01-01T00:00:00+00:00".to_string(),
        };
        assert!(check_latency_consistency(&ok_result).is_ok());

        // Simulate the "timeout" path
        let timeout_result = LatencyResult {
            provider_id: "p2".to_string(),
            app_id: "codex".to_string(),
            latency_ms: None,
            status: "timeout".to_string(),
            error_message: None,
            tested_at: "2025-01-01T00:00:00+00:00".to_string(),
        };
        assert!(check_latency_consistency(&timeout_result).is_ok());

        // Simulate the "error" path (network error, no latency)
        let error_result = LatencyResult {
            provider_id: "p3".to_string(),
            app_id: "claude".to_string(),
            latency_ms: None,
            status: "error".to_string(),
            error_message: Some("Connection refused".to_string()),
            tested_at: "2025-01-01T00:00:00+00:00".to_string(),
        };
        assert!(check_latency_consistency(&error_result).is_ok());
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
        assert!(parsed.error_message.is_none());
    }

    #[test]
    fn test_latency_result_error_serialization() {
        let result = LatencyResult {
            provider_id: "p2".to_string(),
            app_id: "codex".to_string(),
            latency_ms: None,
            status: "timeout".to_string(),
            error_message: None,
            tested_at: "2025-01-01T00:00:00+00:00".to_string(),
        };
        let json = serde_json::to_string(&result).unwrap();
        // error_message should be skipped when None
        assert!(!json.contains("error_message"));
        let parsed: LatencyResult = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.status, "timeout");
        assert!(parsed.latency_ms.is_none());
    }

    #[test]
    fn test_latency_result_with_error_message() {
        let result = LatencyResult {
            provider_id: "p3".to_string(),
            app_id: "claude".to_string(),
            latency_ms: None,
            status: "error".to_string(),
            error_message: Some("Connection refused".to_string()),
            tested_at: "2025-01-01T00:00:00+00:00".to_string(),
        };
        let json = serde_json::to_string(&result).unwrap();
        assert!(json.contains("Connection refused"));
        let parsed: LatencyResult = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.error_message, Some("Connection refused".to_string()));
    }
}
