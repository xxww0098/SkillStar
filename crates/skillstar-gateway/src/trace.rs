//! The last calls this process forwarded.
//!
//! The ring lives in memory. A restart starts empty because nothing is written
//! under the data directory, and nothing is read back. The record is the clock
//! time, the agent, the model the agent asked for, the status, and the
//! completion-token count. It does not carry a secret or an upstream URL.

use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::Value;

use crate::rules::{Caller, request_agent};

/// How many forwarded calls the ring keeps.
pub const TRACE_KEEP: usize = 60;

/// One forwarded call, oldest first once the ring is full.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RecentCall {
    pub at: String,
    pub agent: String,
    pub model: String,
    pub status: u16,
    /// Completion tokens from this response's usage. `None` when usage did not
    /// say; that is not stored as zero.
    pub completion_tokens: Option<u64>,
}

static RECENT: Mutex<Vec<RecentCall>> = Mutex::new(Vec::new());

/// Append one call, dropping the oldest once the ring is past [`TRACE_KEEP`].
pub fn note_recent_call(call: RecentCall) {
    let mut recent = RECENT.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    recent.push(call);
    if recent.len() > TRACE_KEEP {
        let extra = recent.len() - TRACE_KEEP;
        recent.drain(0..extra);
    }
}

/// Calls still in this process, oldest first.
pub fn recent_calls() -> Vec<RecentCall> {
    RECENT
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .clone()
}

/// Drop the ring. Tests use this so a later case does not see an earlier one.
pub fn clear_recent_calls() {
    RECENT
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .clear();
}

pub(crate) fn note_forward(
    authorization: &str,
    user_agent: &str,
    inbound: &[u8],
    status: u16,
    body: &[u8],
) {
    note_recent_call(RecentCall {
        at: stamp_now(),
        agent: request_agent(&Caller {
            authorization,
            user_agent,
            ..Caller::default()
        }),
        model: json_string(inbound, "model"),
        status,
        completion_tokens: completion_tokens(body),
    });
}

fn json_string(bytes: &[u8], key: &str) -> String {
    serde_json::from_slice::<Value>(bytes)
        .ok()
        .and_then(|value| value.get(key).and_then(Value::as_str).map(str::to_string))
        .unwrap_or_default()
}

/// Completion count only. Prompt tokens and a missing usage field stay out.
fn completion_tokens(body: &[u8]) -> Option<u64> {
    let value: Value = serde_json::from_slice(body).ok()?;
    let usage = value.get("usage")?;
    usage
        .get("completion_tokens")
        .or_else(|| usage.get("output_tokens"))
        .and_then(Value::as_u64)
}

fn stamp_now() -> String {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.as_secs())
        .unwrap_or(0);
    format_utc(secs)
}

fn format_utc(secs: u64) -> String {
    let days = secs / 86_400;
    let tod = secs % 86_400;
    let hour = tod / 3_600;
    let minute = (tod % 3_600) / 60;
    let second = tod % 60;
    let (year, month, day) = civil_from_days(days);
    format!("{year:04}-{month:02}-{day:02} {hour:02}:{minute:02}:{second:02}")
}

/// Howard Hinnant's `civil_from_days`, for a day count since the Unix epoch.
fn civil_from_days(days: u64) -> (u64, u64, u64) {
    let z = days + 719_468;
    let era = z / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = if month <= 2 { y + 1 } else { y };
    (year, month, day)
}

#[cfg(test)]
mod tests {
    use super::*;

    static LOCK: Mutex<()> = Mutex::new(());

    #[test]
    fn utc_stamp_is_a_clock_reading() {
        assert_eq!(format_utc(0), "1970-01-01 00:00:00");
        assert_eq!(format_utc(1_700_000_000), "2023-11-14 22:13:20");
    }

    #[test]
    fn usage_tokens_are_the_completion_count() {
        assert_eq!(
            completion_tokens(br#"{"usage":{"prompt_tokens":10,"completion_tokens":5}}"#),
            Some(5)
        );
        assert_eq!(
            completion_tokens(br#"{"usage":{"output_tokens":3}}"#),
            Some(3)
        );
        assert_eq!(completion_tokens(br#"{"usage":{"prompt_tokens":9}}"#), None);
        assert_eq!(completion_tokens(b"not-json"), None);
    }

    #[test]
    fn note_forward_drops_the_bearer_secret() {
        let _guard = LOCK.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        clear_recent_calls();
        note_forward(
            "Bearer sk-secret-value",
            "",
            br#"{"model":"m1"}"#,
            200,
            br#"{"usage":{"prompt_tokens":9},"upstream":"https://api.openai.com/v1"}"#,
        );
        let call = recent_calls().pop().expect("one call");
        assert_eq!(call.model, "m1");
        assert_eq!(call.agent, "other");
        assert_eq!(call.status, 200);
        assert_eq!(call.completion_tokens, None);
        let text = format!("{call:?}");
        assert!(!text.contains("sk-secret-value"), "{text}");
        assert!(!text.contains("https://"), "{text}");
        assert!(!text.contains("api.openai.com"), "{text}");
        clear_recent_calls();
    }
}
