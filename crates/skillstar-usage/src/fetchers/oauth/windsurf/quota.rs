//! Connect-RPC quota for SeatManagementService.
//!
//! Session rows call `GetCurrentUser` / `GetPlanStatus`. An apiKey with no
//! session calls `GetUserStatus` (cockpit's apiKey leg). Missing credit fields
//! omit the window; a present zero is kept.

use std::collections::HashMap;
use std::time::Duration;

use serde_json::{Value, json};

use super::{AUTH1_API_SERVER, DEFAULT_API_SERVER, SEAT_SERVICE, WindsurfState, decrypt_optional};
use crate::subscription::{Subscription, SubscriptionUsage, UsageWindow};
use crate::{UsageError, UsageResult};

const PROMPT_LABEL: &str = "User Prompt credits";
const ADD_ON_LABEL: &str = "Add-on prompt credits";

#[derive(Debug, Clone, PartialEq, Eq)]
enum Credential {
    Session(String),
    ApiKey(String),
    Missing,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct CreditAmounts {
    pub used: i64,
    pub total: i64,
}

#[derive(Debug, Clone, Default)]
pub(crate) struct PlanSnapshot {
    pub plan_name: Option<String>,
    pub prompt: Option<CreditAmounts>,
    pub add_on: Option<CreditAmounts>,
    pub period_end: Option<i64>,
    pub daily_remaining_percent: Option<i64>,
    pub weekly_remaining_percent: Option<i64>,
    pub daily_reset_at: Option<i64>,
    pub weekly_reset_at: Option<i64>,
    pub email: Option<String>,
}

impl PlanSnapshot {
    pub(crate) fn into_usage(self, subscription_id: &str) -> SubscriptionUsage {
        let mut monthly = self
            .prompt
            .as_ref()
            .map(|amounts| credit_window(PROMPT_LABEL, amounts, self.period_end));
        if let Some(add_on) = self
            .add_on
            .as_ref()
            .map(|amounts| credit_window(ADD_ON_LABEL, amounts, self.period_end))
        {
            if let Some(parent) = monthly.as_mut() {
                parent.breakdown.push(add_on);
            } else {
                monthly = Some(add_on);
            }
        }

        SubscriptionUsage {
            subscription_id: subscription_id.to_string(),
            fetched_at: chrono::Utc::now().timestamp(),
            plan_name: self.plan_name,
            hourly: self.daily_remaining_percent.map(|remaining| {
                percent_window("Daily", remaining, self.daily_reset_at.or(self.period_end))
            }),
            weekly: self.weekly_remaining_percent.map(|remaining| {
                percent_window(
                    "Weekly",
                    remaining,
                    self.weekly_reset_at.or(self.period_end),
                )
            }),
            monthly,
            balance: None,
            credits: Vec::new(),
            error: None,
            api_keys: Vec::new(),
            deepseek_analytics: None,
        }
    }
}

pub(crate) async fn fetch_quota(subscription: &mut Subscription) -> UsageResult<SubscriptionUsage> {
    let access = decrypt_optional(&subscription.access_token_encrypted);
    let state = decrypt_optional(&subscription.provider_state_encrypted)
        .as_deref()
        .map(WindsurfState::parse)
        .unwrap_or_default();
    let api_server = state
        .api_server_url
        .clone()
        .unwrap_or_else(|| default_api_server(access.as_deref(), &state).to_string());

    let snapshot = match quota_credential(access.as_deref(), &state) {
        Credential::Session(token) => fetch_session(&api_server, &token).await?,
        Credential::ApiKey(key) => fetch_api_key(&api_server, &key).await?,
        Credential::Missing => {
            return Err(UsageError::Fetcher(
                "Windsurf 缺少 apiKey 或 session，无法读取配额".into(),
            ));
        }
    };

    if let Some(email) = snapshot.email.clone() {
        if subscription
            .oauth_account_id
            .as_deref()
            .is_none_or(|value| value.trim().is_empty())
        {
            subscription.oauth_account_id = Some(email.clone());
        }
        crate::fetchers::oauth::common::apply_email_title(
            subscription,
            Some(&email),
            &["Windsurf"],
        );
    }

    Ok(snapshot.into_usage(&subscription.id))
}

fn default_api_server(access: Option<&str>, state: &WindsurfState) -> &'static str {
    let session = access.unwrap_or("");
    let key = state.api_key.as_deref().unwrap_or("");
    if session.starts_with("devin-session-token$")
        || key.starts_with("devin-session-token$")
        || state.auth1_token.is_some()
    {
        AUTH1_API_SERVER
    } else {
        DEFAULT_API_SERVER
    }
}

fn quota_credential(access: Option<&str>, state: &WindsurfState) -> Credential {
    let access = access.map(str::trim).filter(|value| !value.is_empty());
    if let Some(token) = access {
        let same_as_key = state.api_key.as_deref() == Some(token);
        let session_shaped = token.starts_with("devin-session-token$") || !same_as_key;
        if session_shaped && !token.starts_with("sk-ws-") {
            return Credential::Session(token.to_string());
        }
    }
    if let Some(key) = state
        .api_key
        .clone()
        .or_else(|| access.map(str::to_string))
        .filter(|value| !value.is_empty())
    {
        return Credential::ApiKey(key);
    }
    Credential::Missing
}

async fn fetch_session(base: &str, token: &str) -> UsageResult<PlanSnapshot> {
    let plan = seat_call(
        base,
        "GetPlanStatus",
        json!({
            "authToken": token,
            "includeTopUpStatus": true,
        }),
    )
    .await?;
    let user = match seat_call(
        base,
        "GetCurrentUser",
        json!({
            "authToken": token,
            "includeSubscription": true,
        }),
    )
    .await
    {
        Ok(value) => Some(value),
        Err(UsageError::AuthRequired) => return Err(UsageError::AuthRequired),
        Err(_) => None,
    };
    Ok(snapshot_from_responses(Some(&plan), user.as_ref()))
}

async fn fetch_api_key(base: &str, api_key: &str) -> UsageResult<PlanSnapshot> {
    let status = seat_call(
        base,
        "GetUserStatus",
        json!({
            "metadata": {
                "apiKey": api_key,
                "ideName": "Windsurf",
                "ideVersion": "1.0.0",
                "extensionName": "codeium.windsurf",
                "extensionVersion": "1.0.0",
                "locale": "en",
                "os": ide_os(),
                "disableTelemetry": true,
            }
        }),
    )
    .await?;
    Ok(snapshot_from_responses(Some(&status), None))
}

fn ide_os() -> &'static str {
    match std::env::consts::OS {
        "macos" => "darwin",
        other => other,
    }
}

pub(crate) async fn seat_call(base: &str, method: &str, body: Value) -> UsageResult<Value> {
    let client = crate::fetchers::http_client()?;
    let url = format!(
        "{}/{SEAT_SERVICE}/{method}",
        base.trim().trim_end_matches('/')
    );
    let response = client
        .post(url)
        .header("Accept", "application/json")
        .header("Content-Type", "application/json")
        .header("Connect-Protocol-Version", "1")
        .timeout(Duration::from_secs(20))
        .json(&body)
        .send()
        .await
        .map_err(|err| UsageError::transport(&format!("Windsurf {method}"), err))?;
    let status = response.status().as_u16();
    let bytes = response
        .bytes()
        .await
        .map_err(|err| UsageError::transport(&format!("Windsurf {method}"), err))?;
    decode_seat_body(method, status, &bytes)
}

pub(crate) fn decode_seat_body(method: &str, status: u16, bytes: &[u8]) -> UsageResult<Value> {
    if status == 401 || invalid_grant(bytes) {
        return Err(UsageError::AuthRequired);
    }
    if !(200..300).contains(&status) {
        let text = String::from_utf8_lossy(bytes);
        return Err(UsageError::http_status(
            &format!("Windsurf {method}"),
            status,
            &text,
        ));
    }
    if let Ok(value) = serde_json::from_slice::<Value>(bytes) {
        return Ok(value);
    }
    if let Ok(value) = parse_plan_status_proto(bytes) {
        return Ok(value);
    }
    Err(UsageError::Fetcher(format!(
        "Windsurf {method} 响应不是 JSON 或 planStatus protobuf"
    )))
}

fn invalid_grant(body: &[u8]) -> bool {
    let Ok(value) = serde_json::from_slice::<Value>(body) else {
        return false;
    };
    match value.get("error") {
        Some(Value::String(code)) => code.eq_ignore_ascii_case("invalid_grant"),
        Some(Value::Object(map)) => map
            .get("status")
            .and_then(Value::as_str)
            .is_some_and(|code| code.eq_ignore_ascii_case("invalid_grant")),
        _ => false,
    }
}

pub(crate) fn snapshot_from_responses(plan: Option<&Value>, user: Option<&Value>) -> PlanSnapshot {
    let mut snapshot = plan.map(snapshot_from_json).unwrap_or_default();
    let email = user
        .and_then(email_from_payload)
        .or_else(|| plan.and_then(email_from_payload));
    if snapshot.email.is_none() {
        snapshot.email = email;
    }
    if snapshot.plan_name.is_none()
        && let Some(user) = user
    {
        snapshot.plan_name = plan_name_from(user, plan_status_value(user));
    }
    snapshot
}

pub(crate) fn snapshot_from_json(root: &Value) -> PlanSnapshot {
    let status = plan_status_value(root);
    let info = plan_info(root, status);
    let prompt = credit_amounts(
        pick_i64(
            status,
            &["availablePromptCredits", "available_prompt_credits"],
        ),
        pick_i64(status, &["usedPromptCredits", "used_prompt_credits"]),
        pick_i64(info, &["monthlyPromptCredits", "monthly_prompt_credits"]),
    );
    let add_on = credit_amounts(
        pick_i64(
            status,
            &[
                "availableFlexCredits",
                "available_flex_credits",
                "flexCreditsAvailable",
                "flex_credits_available",
                "availableAddOnCredits",
                "available_add_on_credits",
                "addOnCreditsAvailable",
                "add_on_credits_available",
                "availableTopUpCredits",
                "available_top_up_credits",
                "topUpCreditsAvailable",
                "top_up_credits_available",
            ],
        ),
        pick_i64(
            status,
            &[
                "usedFlexCredits",
                "used_flex_credits",
                "usedAddOnCredits",
                "used_add_on_credits",
                "usedTopUpCredits",
                "used_top_up_credits",
            ],
        ),
        pick_i64(
            info,
            &[
                "monthlyFlexCreditPurchaseAmount",
                "monthly_flex_credit_purchase_amount",
                "monthlyAddOnCredits",
                "monthly_add_on_credits",
                "monthlyTopUpCredits",
                "monthly_top_up_credits",
            ],
        ),
    );

    PlanSnapshot {
        plan_name: plan_name_from(root, status),
        prompt,
        add_on,
        period_end: pick_timestamp(status, &["planEnd", "plan_end"]),
        daily_remaining_percent: pick_i64(
            status,
            &[
                "dailyQuotaRemainingPercent",
                "daily_quota_remaining_percent",
                "dailyRemainingPercent",
            ],
        ),
        weekly_remaining_percent: pick_i64(
            status,
            &[
                "weeklyQuotaRemainingPercent",
                "weekly_quota_remaining_percent",
                "weeklyRemainingPercent",
            ],
        ),
        daily_reset_at: pick_i64(
            status,
            &[
                "dailyQuotaResetAtUnix",
                "daily_quota_reset_at_unix",
                "dailyResetAtUnix",
            ],
        ),
        weekly_reset_at: pick_i64(
            status,
            &[
                "weeklyQuotaResetAtUnix",
                "weekly_quota_reset_at_unix",
                "weeklyResetAtUnix",
            ],
        ),
        email: email_from_payload(root),
    }
}

fn plan_name_from(root: &Value, status: Option<&Value>) -> Option<String> {
    let info = plan_info(root, status);
    pick_name(info, &["planName", "plan_name", "teamsTier", "teams_tier"])
        .or_else(|| pick_name(status, &["planName", "plan_name"]))
        .or_else(|| pick_name(Some(root), &["planName", "plan_name"]))
}

fn pick_name(value: Option<&Value>, keys: &[&str]) -> Option<String> {
    let name = super::pick_string(value, keys)?;
    if name.eq_ignore_ascii_case("unknown") {
        None
    } else {
        Some(name)
    }
}

fn plan_status_value(root: &Value) -> Option<&Value> {
    for path in [
        &["planStatus"][..],
        &["userStatus", "planStatus"],
        &["user", "planStatus"],
    ] {
        if let Some(value) = dig(root, path)
            && value.is_object()
        {
            return Some(value);
        }
    }
    if root.get("planInfo").is_some()
        || root.get("availablePromptCredits").is_some()
        || root.get("available_prompt_credits").is_some()
        || root.get("dailyQuotaRemainingPercent").is_some()
        || root.get("weeklyQuotaRemainingPercent").is_some()
    {
        return Some(root);
    }
    None
}

fn plan_info<'a>(root: &'a Value, status: Option<&'a Value>) -> Option<&'a Value> {
    status
        .and_then(|status| status.get("planInfo").or_else(|| status.get("plan_info")))
        .or_else(|| root.get("planInfo"))
        .or_else(|| root.get("plan_info"))
        .or_else(|| dig(root, &["userStatus", "planInfo"]))
}

fn email_from_payload(root: &Value) -> Option<String> {
    super::pick_string(root.get("user"), &["email"])
        .or_else(|| super::pick_string(root.get("userStatus"), &["email"]))
        .or_else(|| super::pick_string(Some(root), &["email"]))
}

fn dig<'a>(value: &'a Value, path: &[&str]) -> Option<&'a Value> {
    let mut current = value;
    for key in path {
        current = current.get(*key)?;
    }
    Some(current)
}

/// Honest bar only. A lone remaining value is not turned into `used: 0`.
pub(crate) fn credit_amounts(
    available: Option<i64>,
    used: Option<i64>,
    monthly: Option<i64>,
) -> Option<CreditAmounts> {
    let available = available.filter(|value| *value >= 0);
    let used = used.filter(|value| *value >= 0);
    let monthly = monthly.filter(|value| *value > 0);
    let (used, total) = match (available, used, monthly) {
        (_, Some(used), Some(total)) => (used, total),
        (Some(available), Some(used), None) => (used, available.saturating_add(used)),
        (Some(available), None, Some(total)) if total >= available => (total - available, total),
        _ => return None,
    };
    Some(CreditAmounts { used, total })
}

fn credit_window(label: &str, amounts: &CreditAmounts, reset_at: Option<i64>) -> UsageWindow {
    UsageWindow {
        label: label.to_string(),
        used: amounts.used,
        total: Some(amounts.total),
        percent: percent(amounts.used, amounts.total),
        reset_at,
        breakdown: Vec::new(),
    }
}

fn percent_window(label: &str, remaining_percent: i64, reset_at: Option<i64>) -> UsageWindow {
    let remaining = remaining_percent.clamp(0, 100);
    let used = 100 - remaining;
    UsageWindow {
        label: label.to_string(),
        used,
        total: Some(100),
        percent: Some(used as i32),
        reset_at,
        breakdown: Vec::new(),
    }
}

fn percent(used: i64, total: i64) -> Option<i32> {
    if total <= 0 {
        return None;
    }
    Some(((used.clamp(0, total) as i128) * 100 / total as i128) as i32)
}

fn pick_i64(value: Option<&Value>, keys: &[&str]) -> Option<i64> {
    let obj = value?.as_object()?;
    for key in keys {
        if let Some(number) = obj.get(*key).and_then(json_i64) {
            return Some(number);
        }
    }
    None
}

fn pick_timestamp(value: Option<&Value>, keys: &[&str]) -> Option<i64> {
    let obj = value?.as_object()?;
    for key in keys {
        let Some(raw) = obj.get(*key) else {
            continue;
        };
        if let Some(number) = json_i64(raw) {
            return Some(number);
        }
        if let Some(number) = raw.get("seconds").and_then(json_i64) {
            return Some(number);
        }
    }
    None
}

fn json_i64(value: &Value) -> Option<i64> {
    match value {
        Value::Number(number) => number.as_i64().or_else(|| {
            number
                .as_f64()
                .filter(|value| value.is_finite())
                .map(|value| value.round() as i64)
        }),
        Value::String(text) => text.trim().parse::<i64>().ok().or_else(|| {
            text.trim()
                .parse::<f64>()
                .ok()
                .filter(|value| value.is_finite())
                .map(|value| value.round() as i64)
        }),
        _ => None,
    }
}

#[derive(Debug, Clone)]
enum ProtoField {
    Varint(u64),
    Bytes(Vec<u8>),
}

/// Auth1 `GetPlanStatus` protobuf, field numbers copied from cockpit.
///
/// Missing fields stay absent. Plan name `"Unknown"` is not invented here;
/// the JSON mapper drops that placeholder.
pub(crate) fn parse_plan_status_proto(bytes: &[u8]) -> Result<Value, ()> {
    let root = parse_fields(bytes)?;
    let plan_status = proto_bytes(&root, 1).ok_or(())?;
    let fields = parse_fields(&plan_status)?;
    let plan_info = proto_bytes(&fields, 1)
        .and_then(|bytes| parse_fields(&bytes).ok())
        .and_then(|info| proto_string(&info, 2));
    let plan_end = proto_bytes(&fields, 3)
        .and_then(|bytes| parse_fields(&bytes).ok())
        .and_then(|stamp| proto_varint(&stamp, 1));

    let mut status = serde_json::Map::new();
    if let Some(name) = plan_info.filter(|name| !name.trim().is_empty()) {
        status.insert("planInfo".to_string(), json!({ "planName": name.trim() }));
    }
    if let Some(seconds) = plan_end {
        status.insert("planEnd".to_string(), json!({ "seconds": seconds }));
    }
    insert_varint(&mut status, &fields, 14, "dailyQuotaRemainingPercent");
    insert_varint(&mut status, &fields, 15, "weeklyQuotaRemainingPercent");
    insert_varint(&mut status, &fields, 16, "overageBalanceMicros");
    insert_varint(&mut status, &fields, 17, "dailyQuotaResetAtUnix");
    insert_varint(&mut status, &fields, 18, "weeklyQuotaResetAtUnix");
    Ok(json!({ "planStatus": Value::Object(status) }))
}

fn insert_varint(
    status: &mut serde_json::Map<String, Value>,
    fields: &HashMap<u32, Vec<ProtoField>>,
    field: u32,
    name: &str,
) {
    if let Some(value) = proto_varint(fields, field) {
        status.insert(name.to_string(), json!(value));
    }
}

fn parse_fields(data: &[u8]) -> Result<HashMap<u32, Vec<ProtoField>>, ()> {
    let mut fields: HashMap<u32, Vec<ProtoField>> = HashMap::new();
    let mut offset = 0usize;
    while offset < data.len() {
        let (tag, next) = read_varint(data, offset)?;
        if tag == 0 {
            break;
        }
        let field = (tag >> 3) as u32;
        let wire = (tag & 0x7) as u8;
        match wire {
            0 => {
                let (value, next) = read_varint(data, next)?;
                fields
                    .entry(field)
                    .or_default()
                    .push(ProtoField::Varint(value));
                offset = next;
            }
            2 => {
                let (len, content) = read_varint(data, next)?;
                let len = len as usize;
                if content + len > data.len() {
                    return Err(());
                }
                fields
                    .entry(field)
                    .or_default()
                    .push(ProtoField::Bytes(data[content..content + len].to_vec()));
                offset = content + len;
            }
            _ => {
                offset = skip_field(data, next, wire)?;
            }
        }
    }
    Ok(fields)
}

fn proto_varint(fields: &HashMap<u32, Vec<ProtoField>>, field: u32) -> Option<u64> {
    fields.get(&field).and_then(|items| {
        items.iter().find_map(|item| match item {
            ProtoField::Varint(value) => Some(*value),
            ProtoField::Bytes(_) => None,
        })
    })
}

fn proto_bytes(fields: &HashMap<u32, Vec<ProtoField>>, field: u32) -> Option<Vec<u8>> {
    fields.get(&field).and_then(|items| {
        items.iter().find_map(|item| match item {
            ProtoField::Bytes(value) => Some(value.clone()),
            ProtoField::Varint(_) => None,
        })
    })
}

fn proto_string(fields: &HashMap<u32, Vec<ProtoField>>, field: u32) -> Option<String> {
    proto_bytes(fields, field).and_then(|bytes| String::from_utf8(bytes).ok())
}

fn read_varint(data: &[u8], offset: usize) -> Result<(u64, usize), ()> {
    let mut result = 0u64;
    let mut shift = 0;
    let mut pos = offset;
    while pos < data.len() && shift < 64 {
        let byte = data[pos];
        result |= u64::from(byte & 0x7f) << shift;
        pos += 1;
        if byte & 0x80 == 0 {
            return Ok((result, pos));
        }
        shift += 7;
    }
    Err(())
}

fn skip_field(data: &[u8], offset: usize, wire: u8) -> Result<usize, ()> {
    match wire {
        0 => read_varint(data, offset).map(|(_, next)| next),
        1 => {
            if offset + 8 > data.len() {
                Err(())
            } else {
                Ok(offset + 8)
            }
        }
        2 => {
            let (len, content) = read_varint(data, offset)?;
            let end = content + len as usize;
            if end > data.len() { Err(()) } else { Ok(end) }
        }
        5 => {
            if offset + 4 > data.len() {
                Err(())
            } else {
                Ok(offset + 4)
            }
        }
        _ => Err(()),
    }
}

#[cfg(test)]
pub(crate) fn encode_varint(mut value: u64) -> Vec<u8> {
    let mut bytes = Vec::new();
    while value >= 0x80 {
        bytes.push((value as u8 & 0x7f) | 0x80);
        value >>= 7;
    }
    bytes.push(value as u8);
    bytes
}

#[cfg(test)]
pub(crate) fn field_varint(field: u32, value: u64) -> Vec<u8> {
    let mut bytes = encode_varint(u64::from(field) << 3);
    bytes.extend(encode_varint(value));
    bytes
}

#[cfg(test)]
pub(crate) fn field_bytes(field: u32, value: &[u8]) -> Vec<u8> {
    let mut bytes = encode_varint((u64::from(field) << 3) | 2);
    bytes.extend(encode_varint(value.len() as u64));
    bytes.extend_from_slice(value);
    bytes
}

#[cfg(test)]
#[path = "quota_tests.rs"]
mod tests;
