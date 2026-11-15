//! What one quota read produced: a plain data model the cards render.

use serde::{Deserialize, Serialize};

/// How a window's numbers are meant to read.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum UsageUnit {
    /// Only a percentage is known.
    Percent,
    /// A count (requests, points, credits) with `used` / `total`.
    Count,
    /// Money in cents (the provider's own unit), rendered as dollars.
    UsdCents,
}

/// One quota window: the 5-hour rule, the weekly cap, a monthly allowance, or
/// a per-model bucket.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UsageWindow {
    /// Stable key the UI and the account layer branch on
    /// (`five_hour`, `week`, `month`, or the provider's model id).
    pub key: String,
    /// What the provider calls this window; the UI may localise known keys.
    pub label: String,
    /// Remaining share, 0..=100. Always remaining, never used.
    pub remaining_percent: f32,
    #[serde(default)]
    pub used: Option<f64>,
    #[serde(default)]
    pub total: Option<f64>,
    pub unit: UsageUnit,
    /// Unix seconds when the window resets.
    #[serde(default)]
    pub resets_at: Option<i64>,
}

/// Points or credits that are not a window (Codex credits, Grok reset cards).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Credits {
    pub label: String,
    /// `None` means unlimited.
    #[serde(default)]
    pub remaining: Option<f64>,
    #[serde(default)]
    pub unlimited: bool,
    pub unit: UsageUnit,
}

/// One quota read.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UsageSnapshot {
    /// Unix seconds when the read happened.
    pub fetched_at: i64,
    #[serde(default)]
    pub windows: Vec<UsageWindow>,
    #[serde(default)]
    pub plan: Option<String>,
    #[serde(default)]
    pub credits: Option<Credits>,
    /// Why the numbers are stale, when the last read failed transiently.
    #[serde(default)]
    pub error: Option<String>,
    /// Antigravity only: the account holds a Google Cloud ToS tier, which is
    /// what decides between the daily and the production Cloud Code endpoint.
    /// Kept on the snapshot so the card can show it and the next read can route
    /// without asking the provider again. Absent for every other agent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gcp_tos: Option<bool>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_snapshot_written_before_gcp_tos_existed_still_loads() {
        let stored = r#"{"fetchedAt":1,"windows":[],"plan":"PRO"}"#;
        let snapshot: UsageSnapshot = serde_json::from_str(stored).expect("an old snapshot");
        assert_eq!(snapshot.plan.as_deref(), Some("PRO"));
        assert_eq!(snapshot.gcp_tos, None);
    }

    #[test]
    fn gcp_tos_serialises_as_camel_case_and_stays_out_when_unknown() {
        let known = UsageSnapshot {
            fetched_at: 1,
            gcp_tos: Some(true),
            ..UsageSnapshot::default()
        };
        let json = serde_json::to_string(&known).expect("json");
        assert!(json.contains("\"gcpTos\":true"), "{json}");
        let decoded: UsageSnapshot = serde_json::from_str(&json).expect("round trip");
        assert_eq!(decoded.gcp_tos, Some(true));

        let unknown = serde_json::to_string(&UsageSnapshot::default()).expect("json");
        assert!(!unknown.contains("gcpTos"), "{unknown}");
    }
}
