//! Antigravity 的响应解析（纯函数，夹具直接驱动）。
//!
//! Cloud Code Assist 的语义和别家相反：`remainingFraction` 是**剩余**份额，
//! 而这张卡的窗口按"已用百分比"记 used，所以这里统一换算成
//! `remaining = round(frac*100)`、`used = 100 - remaining`。

use serde_json::Value;

use crate::usage::snapshot::{Credits, UsageUnit, UsageWindow};

use super::parse;

/// 项目 id 里唯一要排除的值：它是 Google 内部占位项目，不是用户的资源项目。
const EXCLUDED_PROJECT_ID: &str = "aicode-consumers";

/// 默认套餐：provider 没说 tier 时卡片上的字样。
const FREE_PLAN: &str = "FREE";

/// 一个额度窗口：组/桶或单个模型。
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct BucketWindow {
    pub(crate) key: String,
    pub(crate) label: String,
    /// 剩余份额，0..=100（已四舍五入）。
    pub(crate) remaining_percent: f32,
    pub(crate) resets_at: Option<i64>,
}

/// loadCodeAssist 一次响应里能读到的全部信息。
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct PlanInfo {
    pub(crate) plan: String,
    pub(crate) gcp_tos: Option<bool>,
    pub(crate) credits: Option<Credits>,
    pub(crate) project: Option<String>,
    /// 没有项目时用来发起 onboardUser 的 tier。
    pub(crate) onboard_tier: Option<String>,
}

/// 一次 onboardUser 响应：还要轮询，或者已经给出项目 id。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum OnboardStep {
    /// 还没完成，拿这个名字去轮询 operation。
    Pending(String),
    /// 完成了，可能带项目 id。
    Done(Option<String>),
}

/// 把一次 loadCodeAssist 响应拆成上面那些字段。
pub(crate) fn plan_info(value: &Value) -> PlanInfo {
    PlanInfo {
        plan: plan_name(value),
        gcp_tos: gcp_tos(value),
        credits: credits(value),
        project: response_project(value),
        onboard_tier: onboard_tier(value),
    }
}

// ---------------------------------------------------------------------------
// 项目 id
// ---------------------------------------------------------------------------

/// 一个项目字段的值：字符串，或 `{id|projectId|project_id|name}` 对象。
pub(crate) fn project_value(value: &Value) -> Option<String> {
    match value {
        Value::String(text) => usable_project(text),
        Value::Object(_) => ["id", "projectId", "project_id", "name"]
            .iter()
            .find_map(|key| value.get(*key).and_then(project_value)),
        _ => None,
    }
}

fn usable_project(text: &str) -> Option<String> {
    let trimmed = text.trim();
    if trimmed.is_empty() || trimmed == EXCLUDED_PROJECT_ID {
        return None;
    }
    Some(trimmed.to_string())
}

/// loadCodeAssist 响应里的项目：三个可能的位置，按优先级取第一个可用的。
pub(crate) fn response_project(value: &Value) -> Option<String> {
    ["cloudaicompanionProject", "project", "cloudaicompanionProjectId"]
        .iter()
        .find_map(|key| value.get(*key).and_then(project_value))
}

// ---------------------------------------------------------------------------
// 套餐 / ToS / 积分
// ---------------------------------------------------------------------------

/// 套餐名：`paidTier.id` → `currentTier.id` → 默认 allowedTier → `FREE`。
/// 原样保留 provider 的大小写，UI 直接显示。
pub(crate) fn plan_name(value: &Value) -> String {
    tier_id(value.get("paidTier"))
        .or_else(|| tier_id(value.get("currentTier")))
        .or_else(|| default_tier_id(value))
        .unwrap_or_else(|| FREE_PLAN.to_string())
}

fn tier_id(tier: Option<&Value>) -> Option<String> {
    parse::text_at(tier, "/id")
}

fn default_tier(value: &Value) -> Option<&Value> {
    let tiers = value.get("allowedTiers")?.as_array()?;
    tiers
        .iter()
        .find(|tier| {
            tier.get("isDefault")
                .and_then(Value::as_bool)
                .unwrap_or(false)
        })
        .or_else(|| tiers.first())
}

fn default_tier_id(value: &Value) -> Option<String> {
    parse::text(default_tier(value)?.get("id"))
}

/// GCP ToS 账号：`currentTier.usesGcpTos`，其次默认 allowedTier 的同名字段。
/// 它决定额度读走生产端点还是 daily 端点。
pub(crate) fn gcp_tos(value: &Value) -> Option<bool> {
    bool_at(value.get("currentTier"))
        .or_else(|| bool_at(default_tier(value)))
}

fn bool_at(tier: Option<&Value>) -> Option<bool> {
    tier?.get("usesGcpTos")?.as_bool()
}

/// 发起 onboardUser 时选的 tier：默认档优先，其次第一个带 id 的档。
pub(crate) fn onboard_tier(value: &Value) -> Option<String> {
    let tiers = value.get("allowedTiers")?.as_array()?;
    tiers
        .iter()
        .find(|tier| {
            tier.get("isDefault")
                .and_then(Value::as_bool)
                .unwrap_or(false)
        })
        .and_then(|tier| parse::text(tier.get("id")))
        .or_else(|| tiers.iter().find_map(|tier| parse::text(tier.get("id"))))
        .or_else(|| (!tiers.is_empty()).then(|| "LEGACY".to_string()))
}

/// `paidTier.availableCredits` 里只留 `creditAmount` 非空的项。
///
/// 快照只有一个 credits 槽位，所以这里把条目合计成一行：只有一种积分类型时用
/// 它的名字，多种时用「积分」。缺 creditAmount 的条目是"有这种积分但没到账"，
/// 不能当成 0 参与合计。
pub(crate) fn credits(value: &Value) -> Option<Credits> {
    let entries = value.pointer("/paidTier/availableCredits")?.as_array()?;
    let mut total = 0.0;
    let mut types: Vec<String> = Vec::new();
    let mut counted = false;
    for entry in entries {
        let Some(amount) = parse::number(entry.get("creditAmount")) else {
            continue;
        };
        counted = true;
        total += amount;
        if let Some(credit_type) = parse::text(entry.get("creditType"))
            && !types.contains(&credit_type)
        {
            types.push(credit_type);
        }
    }
    if !counted {
        return None;
    }
    let label = match types.as_slice() {
        [only] => only.clone(),
        _ => "积分".to_string(),
    };
    Some(Credits {
        label,
        remaining: Some(total),
        unlimited: false,
        unit: UsageUnit::Count,
    })
}

// ---------------------------------------------------------------------------
// 组 / 桶窗口
// ---------------------------------------------------------------------------

/// `retrieveUserQuotaSummary` 的组与桶：5h / weekly 这类窗口。
pub(crate) fn summary_windows(summary: &Value) -> Vec<BucketWindow> {
    let groups = summary
        .get("groups")
        .or_else(|| summary.get("response").and_then(|value| value.get("groups")))
        .and_then(Value::as_array);
    let Some(groups) = groups else {
        return Vec::new();
    };
    let mut windows = Vec::new();
    for group in groups {
        let group_label = parse::text_in(group, &["displayName", "display_name"])
            .unwrap_or_else(|| "Antigravity".to_string());
        let Some(buckets) = group.get("buckets").and_then(Value::as_array) else {
            continue;
        };
        for bucket in buckets {
            let Some(fraction) = fraction_in(bucket) else {
                continue;
            };
            let bucket_name = parse::text_in(bucket, &["displayName", "display_name"])
                .as_deref()
                .and_then(bucket_label)
                .or_else(|| parse::text_in(bucket, &["window", "windowName", "window_name"]));
            let label = match &bucket_name {
                Some(name) => format!("{group_label} · {name}"),
                None => group_label.clone(),
            };
            let key = parse::text_in(bucket, &["bucketId", "bucket_id"])
                .unwrap_or_else(|| label.clone());
            windows.push(BucketWindow {
                key,
                label,
                remaining_percent: remaining_percent(fraction),
                resets_at: reset_time(bucket.get("resetTime").or_else(|| bucket.get("reset_time"))),
            });
        }
    }
    windows
}

/// 桶名去掉末尾冗余状态词并归一常见周期短名：`Gemini Models remaining` → `Gemini Models`、
/// `Five Hour Limit` → `5h`、`Weekly Limit` → `Weekly`。
fn bucket_label(value: &str) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return None;
    }
    let lower = trimmed.to_ascii_lowercase();
    let stripped = [" remaining", " left", " available", " limit"]
        .into_iter()
        .find_map(|suffix| {
            lower
                .ends_with(suffix)
                .then(|| trimmed[..trimmed.len() - suffix.len()].trim())
        })
        .unwrap_or(trimmed);
    if stripped.is_empty() {
        return None;
    }
    Some(match stripped.to_ascii_lowercase().as_str() {
        "five hour" | "five-hour" | "5 hour" | "5-hour" | "5 hours" => "5h".to_string(),
        "weekly" | "week" | "7 day" | "7-day" | "7 days" => "Weekly".to_string(),
        _ => stripped.to_string(),
    })
}

// ---------------------------------------------------------------------------
// 模型明细
// ---------------------------------------------------------------------------

/// `fetchAvailableModels` 的 per-model 额度。
///
/// 只收名字像模型的条目：这张卡上出现 `chat_editor` 之类内部模型只会让人困惑。
/// 只有 `resetTime` 没有 `remainingFraction` 的条目是"重置时间上下文"，不是额度，
/// 不能画成 100% 已用。
pub(crate) fn model_windows(models: &Value) -> Vec<BucketWindow> {
    let entries = models
        .get("models")
        .and_then(Value::as_object)
        .or_else(|| models.as_object());
    let Some(entries) = entries else {
        return Vec::new();
    };
    let mut windows = Vec::new();
    for (id, entry) in entries {
        if !is_model_entry(id, entry) {
            continue;
        }
        let Some(quota) = model_quota(entry) else {
            continue;
        };
        let Some(fraction) = fraction_in(quota) else {
            continue;
        };
        let label = parse::text_in(entry, &["displayName", "display_name"])
            .unwrap_or_else(|| id.clone());
        windows.push(BucketWindow {
            key: id.clone(),
            label,
            remaining_percent: remaining_percent(fraction),
            resets_at: reset_time(quota.get("resetTime").or_else(|| quota.get("reset_time"))),
        });
    }
    windows
}

/// 模型条目上可能出现的名字字段，任一命中就算模型。
fn is_model_entry(id: &str, entry: &Value) -> bool {
    const MODEL_MARKERS: [&str; 4] = ["gemini", "claude", "gpt", "imagen"];
    let candidates = [
        Some(id),
        entry.get("model").and_then(Value::as_str),
        entry.get("modelId").and_then(Value::as_str),
        entry.get("model_id").and_then(Value::as_str),
        entry.get("name").and_then(Value::as_str),
        entry.get("displayName").and_then(Value::as_str),
    ];
    candidates.into_iter().flatten().any(|candidate| {
        let normalized = candidate.to_ascii_lowercase();
        MODEL_MARKERS
            .iter()
            .any(|marker| normalized.contains(marker))
    })
}

fn model_quota(entry: &Value) -> Option<&Value> {
    entry
        .get("quotaInfo")
        .or_else(|| entry.get("quota_info"))
        .filter(|value| value.is_object())
}

// ---------------------------------------------------------------------------
// 数值与时间
// ---------------------------------------------------------------------------

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

/// 一个窗口从桶/模型换算成快照里的窗口：remaining 与 used 互补。
pub(crate) fn usage_window(window: &BucketWindow) -> UsageWindow {
    let remaining = window.remaining_percent;
    UsageWindow {
        key: window.key.clone(),
        label: window.label.clone(),
        remaining_percent: remaining,
        used: Some(f64::from(100.0 - remaining)),
        total: Some(100.0),
        unit: UsageUnit::Percent,
        resets_at: window.resets_at,
    }
}

/// 把一批桶/模型窗口换算成快照里的窗口。
pub(crate) fn usage_windows(windows: &[BucketWindow]) -> Vec<UsageWindow> {
    windows.iter().map(usage_window).collect()
}
