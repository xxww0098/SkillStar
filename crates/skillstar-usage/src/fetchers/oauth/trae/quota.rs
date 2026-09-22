//! `GetUserInfo` plus pay entitlement. Missing dollar fields omit the window.
//! Amounts are USD cents under the `Monthly credits` label so the card renders
//! `$used / $total`.

use serde_json::{Value, json};

use super::http::{self, cloudide_headers, pay_headers};
use super::{epoch_seconds, pick_f64, pick_i64, pick_string};
use crate::subscription::{CreditInfo, UsageWindow};
use crate::trae_platform::TraePlatformKind;
use crate::{UsageError, UsageResult};

pub(super) const USER_INFO_PATH: &str = "/cloudide/api/v3/trae/GetUserInfo";
const MONTHLY: &str = "Monthly credits";

pub(super) struct QuotaSnapshot {
    pub plan_name: Option<String>,
    pub user_id: Option<String>,
    pub email: Option<String>,
    pub login_region: Option<String>,
    pub monthly: Option<UsageWindow>,
    pub credits: Vec<CreditInfo>,
}

pub(super) async fn read_quota(
    client: &reqwest::Client,
    kind: TraePlatformKind,
    origins: &[String],
    access_token: &str,
) -> UsageResult<QuotaSnapshot> {
    let profile = post_first(
        client,
        origins,
        &[USER_INFO_PATH],
        &cloudide_headers(access_token),
        &json!({}),
        "Trae GetUserInfo",
    )
    .await;
    let pay = post_first(
        client,
        origins,
        kind.pay_status_paths(),
        &pay_headers(access_token),
        &json!({}),
        "Trae ide_user_pay_status",
    )
    .await;
    let usage = post_first(
        client,
        origins,
        kind.ent_usage_paths(),
        &pay_headers(access_token),
        &json!({ "require_usage": true }),
        "Trae ide_user_ent_usage",
    )
    .await;

    let mut errors = Vec::new();
    let profile = unwrap_leg(profile, &mut errors);
    let pay = unwrap_leg(pay, &mut errors);
    let mut usage = unwrap_leg(usage, &mut errors);
    if usage.is_none() && !kind.current_entitlement_paths().is_empty() {
        match post_first(
            client,
            origins,
            kind.current_entitlement_paths(),
            &pay_headers(access_token),
            &json!({ "require_usage": true }),
            "Trae user_current_entitlement_list",
        )
        .await
        {
            Ok(value) => usage = Some(value),
            Err(err) => errors.push(err),
        }
    }
    if profile.is_none() && pay.is_none() && usage.is_none() {
        return Err(fold_errors(errors));
    }
    Ok(snapshot_from(
        kind,
        profile.as_ref(),
        pay.as_ref(),
        usage.as_ref(),
    ))
}

fn unwrap_leg(result: Result<Value, UsageError>, errors: &mut Vec<UsageError>) -> Option<Value> {
    match result {
        Ok(value) => Some(value),
        Err(err) => {
            errors.push(err);
            None
        }
    }
}

fn fold_errors(errors: Vec<UsageError>) -> UsageError {
    let mut folded: Option<UsageError> = None;
    for err in errors {
        folded = Some(http::prefer_error(folded, err));
    }
    folded.unwrap_or_else(|| UsageError::Fetcher("Trae 配额请求失败".into()))
}

fn snapshot_from(
    kind: TraePlatformKind,
    profile: Option<&Value>,
    pay: Option<&Value>,
    usage: Option<&Value>,
) -> QuotaSnapshot {
    let user_id = profile.and_then(|value| {
        pick_string(
            value,
            &[&["UserID"], &["userId"], &["user_id"], &["uid"], &["id"]],
        )
    });
    let email = profile.and_then(|value| {
        pick_string(value, &[&["NonPlainTextEmail"], &["Email"], &["email"]])
            .filter(|value| value.contains('@'))
    });
    let login_region = profile
        .and_then(|value| {
            pick_string(
                value,
                &[
                    &["loginRegion"],
                    &["userRegion", "region"],
                    &["storeRegion"],
                    &["AIRegion"],
                ],
            )
        })
        .as_deref()
        .and_then(normalize_login_region);
    let raw_plan = pay.and_then(|value| {
        pick_string(
            value,
            &[
                &["user_pay_identity_str"],
                &["identityStr"],
                &["identity_str"],
            ],
        )
    });
    let pack = usage.and_then(|value| select_pack(kind, value));
    let plan_from_pack = pack.and_then(|pack| {
        pick_string(pack, &[&["usage", "identity_str"], &["identity_str"]])
            .or_else(|| product_label(kind, pack))
    });
    let reset = pack.and_then(pack_reset);
    let basic = pack.and_then(|pack| {
        dollar_window(
            pick_f64(
                pack,
                &[&["usage", "basic_usage_amount"], &["usage", "basic_usage"]],
            ),
            pick_f64(
                pack,
                &[
                    &["entitlement_base_info", "quota", "basic_usage_limit"],
                    &["entitlement_base_info", "quota", "basic_quota"],
                ],
            ),
            reset,
        )
    });
    let bonus_used = pack.and_then(|pack| {
        pick_f64(
            pack,
            &[&["usage", "bonus_usage_amount"], &["usage", "bonus_usage"]],
        )
    });
    let bonus_total = pack.and_then(|pack| {
        pick_f64(
            pack,
            &[
                &["entitlement_base_info", "quota", "bonus_usage_limit"],
                &["entitlement_base_info", "quota", "bonus_quota"],
            ],
        )
    });
    let mut credits = Vec::new();
    let monthly = if basic.is_some() {
        if let Some(credit) = dollar_credit("Bonus", bonus_used, bonus_total) {
            credits.push(credit);
        }
        basic
    } else {
        dollar_window(bonus_used, bonus_total, reset)
    };
    if let Some(pack) = pack
        && let Some(pay_go) =
            pick_f64(pack, &[&["usage", "pay_go_amount"]]).filter(|amount| *amount > 0.0)
    {
        credits.push(CreditInfo {
            credit_type: "On-demand".into(),
            credit_amount: Some(format!("${pay_go:.2}")),
            minimum_credit_amount_for_usage: None,
        });
    }
    QuotaSnapshot {
        plan_name: raw_plan.or(plan_from_pack),
        user_id,
        email,
        login_region,
        monthly,
        credits,
    }
}

fn select_pack<'a>(kind: TraePlatformKind, usage: &'a Value) -> Option<&'a Value> {
    let packs = usage
        .get("user_entitlement_pack_list")
        .or_else(|| super::payload_root(usage).get("user_entitlement_pack_list"))
        .and_then(Value::as_array)?;
    let usable: Vec<&Value> = packs
        .iter()
        .filter(|pack| product_type(pack) != Some(3))
        .collect();
    if usable.is_empty() {
        return None;
    }
    let order: &[i64] = if kind.is_cn() {
        &[100, 6, 5, 4, 1, 9, 8, 0]
    } else {
        &[6, 4, 1, 9, 8, 0]
    };
    for product in order {
        if let Some(pack) = usable
            .iter()
            .copied()
            .find(|pack| product_type(pack) == Some(*product))
        {
            return Some(pack);
        }
    }
    usable.first().copied()
}

fn product_type(pack: &Value) -> Option<i64> {
    pick_i64(
        pack,
        &[
            &["entitlement_base_info", "product_type"],
            &["product_type"],
        ],
    )
}

fn product_label(kind: TraePlatformKind, pack: &Value) -> Option<String> {
    let product = product_type(pack)?;
    let name = match product {
        100 if kind.is_cn() => "CNExpress",
        6 => "Ultra",
        5 | 4 => "Pro+",
        1 | 9 => "Pro",
        8 => "Lite",
        0 => "Free",
        _ => return None,
    };
    Some(name.to_string())
}

fn pack_reset(pack: &Value) -> Option<i64> {
    pick_i64(pack, &[&["entitlement_base_info", "end_time"]])
        .and_then(epoch_seconds)
        .filter(|ts| *ts > 0)
        .map(|ts| ts.saturating_add(1))
        .or_else(|| {
            pick_i64(pack, &[&["detail", "subscription_renew_time"]]).and_then(epoch_seconds)
        })
}

/// Both sides are required. An explicit zero used stays zero; a missing side
/// omits the window instead of inventing 0.
fn dollar_window(
    used: Option<f64>,
    total: Option<f64>,
    reset_at: Option<i64>,
) -> Option<UsageWindow> {
    let used = cents(used)?;
    let total = cents(total).filter(|total| *total > 0)?;
    Some(UsageWindow {
        label: MONTHLY.to_string(),
        used,
        total: Some(total),
        percent: Some(((used.clamp(0, total) as i128) * 100 / total as i128) as i32),
        reset_at,
        breakdown: Vec::new(),
    })
}

fn dollar_credit(label: &str, used: Option<f64>, total: Option<f64>) -> Option<CreditInfo> {
    let used = used.filter(|value| value.is_finite() && *value >= 0.0)?;
    let total = total.filter(|value| value.is_finite() && *value > 0.0)?;
    Some(CreditInfo {
        credit_type: label.to_string(),
        credit_amount: Some(format!("${used:.2} / ${total:.2}")),
        minimum_credit_amount_for_usage: None,
    })
}

fn cents(amount: Option<f64>) -> Option<i64> {
    let amount = amount.filter(|value| value.is_finite() && *value >= 0.0)?;
    Some((amount * 100.0).round().clamp(0.0, i64::MAX as f64) as i64)
}

pub(super) fn normalize_login_region(raw: &str) -> Option<String> {
    let value = raw.trim();
    if value.is_empty() {
        return None;
    }
    let lower = value.to_ascii_lowercase();
    let normalized = match lower.as_str() {
        "china-north" => "cn",
        "singapore-central" => "sg",
        "us-east" | "us-east-1" => "us",
        other => other,
    };
    Some(normalized.to_string())
}

async fn post_first(
    client: &reqwest::Client,
    origins: &[String],
    paths: &[&str],
    headers: &[(&'static str, String)],
    body: &Value,
    label: &str,
) -> Result<Value, UsageError> {
    let mut last: Option<UsageError> = None;
    for path in paths {
        for origin in origins {
            let url = format!("{}{path}", origin.trim_end_matches('/'));
            match http::post_json(client, &url, headers, body, label).await {
                Ok(value) => return Ok(value),
                Err(err) => last = Some(http::prefer_error(last, err)),
            }
        }
    }
    Err(last.unwrap_or_else(|| UsageError::Fetcher(format!("{label} 没有可用地址"))))
}
