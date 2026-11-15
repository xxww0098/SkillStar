//! Gemini CLI 的额度响应解析（纯函数，夹具直接驱动）。
//!
//! 与 Antigravity 同属 Google Cloud Code Assist：`loadCodeAssist` 的套餐/tier/
//! 项目/积分契约完全相同，所以那部分直接复用 [super::antigravity_parse]（同一份
//! JSON），这里只补 Gemini CLI 独有的 `retrieveUserQuota` 桶形状。
//!
//! 语义与别家相反：`remainingFraction` 是**剩余**份额，快照里的窗口按 0..=100
//! 的剩余百分比记，所以这里只做乘法与四舍五入，绝不翻转成"已用"。

use serde_json::Value;

use super::antigravity_parse::BucketWindow;
use super::parse;

/// `retrieveUserQuota` 的桶：每个模型（可再按 tokenType 分桶）一条。
///
/// 没有 modelId 的桶不能悄悄丢掉：它是"这个账号整体还剩多少"的那一条，用
/// tokenType 或固定名当 key 保留下来。
pub(crate) fn quota_windows(payload: &Value) -> Vec<BucketWindow> {
    let Some(buckets) = payload.get("buckets").and_then(Value::as_array) else {
        return Vec::new();
    };
    let mut windows = Vec::new();
    for bucket in buckets {
        let Some(fraction) = fraction_in(bucket) else {
            continue;
        };
        let token_type = parse::text_in(bucket, &["tokenType", "token_type"]);
        let model = parse::text_in(bucket, &["modelId", "model_id"]);
        let label = match (&model, &token_type) {
            (Some(model), _) => model.clone(),
            (None, Some(token_type)) => format!("Gemini · {token_type}"),
            (None, None) => "Gemini".to_string(),
        };
        let key = match (&model, &token_type) {
            (Some(model), Some(token_type)) => {
                format!("{model}:{}", token_type.to_ascii_lowercase())
            }
            (Some(model), None) => model.clone(),
            (None, Some(token_type)) => token_type.to_ascii_lowercase(),
            (None, None) => label.clone(),
        };
        windows.push(BucketWindow {
            key,
            label,
            remaining_percent: remaining_percent(fraction),
            resets_at: reset_time(
                bucket
                    .get("resetTime")
                    .or_else(|| bucket.get("reset_time")),
            ),
        });
    }
    windows
}

/// 剩余份额：数字、数字字符串、或带 `%` 的字符串。
fn fraction_in(node: &Value) -> Option<f64> {
    let remaining = node
        .get("remainingFraction")
        .or_else(|| node.get("remaining_fraction"))
        .or_else(|| node.get("remaining"))?;
    if let Some(number) = parse::number(Some(remaining)) {
        return Some(number);
    }
    let text = remaining.as_str()?.trim();
    let percent = text.strip_suffix('%')?.trim();
    percent.parse::<f64>().ok().map(|value| value / 100.0)
}

/// 剩余份额换算成百分数（0..=100，四舍五入）。
fn remaining_percent(fraction: f64) -> f32 {
    if !fraction.is_finite() {
        return 0.0;
    }
    (fraction * 100.0).round().clamp(0.0, 100.0) as f32
}

/// 重置时间只认 RFC3339：provider 偶尔回一个本地化字符串，那不是时间点。
fn reset_time(value: Option<&Value>) -> Option<i64> {
    let text = value?.as_str()?.trim();
    chrono::DateTime::parse_from_rfc3339(text)
        .ok()
        .map(|at| at.timestamp())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn a_bucket_becomes_a_remaining_window() {
        let payload = json!({
            "buckets": [{
                "modelId": "gemini-2.5-pro",
                "tokenType": "REQUESTS",
                "remainingFraction": 0.25,
                "resetTime": "2026-11-16T05:00:00Z"
            }]
        });
        let windows = quota_windows(&payload);
        assert_eq!(windows.len(), 1);
        let window = &windows[0];
        assert_eq!(window.key, "gemini-2.5-pro:requests");
        assert_eq!(window.label, "gemini-2.5-pro");
        assert_eq!(window.remaining_percent, 25.0, "remainingFraction is remaining, never used");
        assert_eq!(window.resets_at, Some(1_794_805_200));
    }

    #[test]
    fn a_bucket_without_a_model_keeps_its_own_row() {
        let payload = json!({ "buckets": [{ "remainingFraction": 0.5 }] });
        let windows = quota_windows(&payload);
        assert_eq!(windows.len(), 1);
        assert_eq!(windows[0].key, "Gemini");
        assert_eq!(windows[0].remaining_percent, 50.0);

        let typed = json!({ "buckets": [{ "tokenType": "TOKENS", "remainingFraction": "0.4" }] });
        let windows = quota_windows(&typed);
        assert_eq!(windows[0].key, "tokens");
        assert_eq!(windows[0].label, "Gemini · TOKENS");
        assert_eq!(windows[0].remaining_percent, 40.0);
    }

    #[test]
    fn an_unreadable_or_missing_bucket_list_yields_no_windows() {
        assert!(quota_windows(&json!({})).is_empty());
        assert!(quota_windows(&json!({ "buckets": "nope" })).is_empty());
        // 没有剩余份额的桶是上下文条目，不是额度。
        let only_reset = json!({ "buckets": [{ "modelId": "gemini-2.5-pro", "resetTime": "2026-11-16T05:00:00Z" }] });
        assert!(quota_windows(&only_reset).is_empty());
        // 本地化时间不是时间点。
        let localized = json!({ "buckets": [{ "remainingFraction": 1.0, "resetTime": "明天早上" }] });
        assert_eq!(quota_windows(&localized)[0].resets_at, None);
    }
}
