//! Pure helpers every fetcher shares: numbers a provider wrote as strings,
//! reset times in four dialects, and the arithmetic that turns a provider's
//! "used" into this crate's "remaining".

use serde_json::Value;

/// Unix seconds from a reset timestamp written as epoch seconds, epoch
/// milliseconds, a numeric string, or RFC 3339. Claude ships all four across
/// its plan windows; Cursor writes ISO timestamps where Codex writes seconds.
pub(crate) fn epoch_seconds(value: Option<&Value>) -> Option<i64> {
    let value = value?;
    match value {
        Value::String(text) => {
            let text = text.trim();
            if text.is_empty() {
                return None;
            }
            if let Ok(raw) = text.parse::<i64>() {
                return scale_epoch(raw);
            }
            if let Ok(raw) = text.parse::<f64>() {
                return scale_epoch(raw as i64);
            }
            chrono::DateTime::parse_from_rfc3339(text)
                .ok()
                .map(|at| at.timestamp())
        }
        _ => scale_epoch(integer(Some(value))?),
    }
}

/// Epoch values above this bound are milliseconds, not seconds.
const EPOCH_MILLISECONDS_ABOVE: i64 = 10_000_000_000;

fn scale_epoch(raw: i64) -> Option<i64> {
    if raw <= 0 {
        return None;
    }
    Some(if raw > EPOCH_MILLISECONDS_ABOVE {
        raw / 1000
    } else {
        raw
    })
}

/// A finite number from a JSON value, accepting the string-encoded form
/// providers ship in some builds.
pub(crate) fn number(value: Option<&Value>) -> Option<f64> {
    match value? {
        Value::Number(number) => number.as_f64().filter(|value| value.is_finite()),
        Value::String(text) => text
            .trim()
            .parse::<f64>()
            .ok()
            .filter(|value| value.is_finite()),
        _ => None,
    }
}

/// The first of the given keys on a node that holds a finite number.
pub(crate) fn number_in(node: &Value, keys: &[&str]) -> Option<f64> {
    keys.iter().find_map(|key| number(node.get(*key)))
}

/// A finite number at a JSON pointer such as /breakdown/total.
pub(crate) fn number_at(node: &Value, pointer: &str) -> Option<f64> {
    number(node.pointer(pointer))
}

/// An integer, accepting a float that names a whole number.
pub(crate) fn integer(value: Option<&Value>) -> Option<i64> {
    match value? {
        Value::Number(number) => number
            .as_i64()
            .or_else(|| number.as_f64().map(|value| value as i64)),
        Value::String(text) => text.trim().parse::<i64>().ok(),
        _ => None,
    }
}

/// A trimmed, non-empty string at a JSON value.
pub(crate) fn text(value: Option<&Value>) -> Option<String> {
    let text = value?.as_str()?.trim();
    (!text.is_empty()).then(|| text.to_string())
}

/// A trimmed, non-empty string at a JSON pointer.
pub(crate) fn text_at(value: Option<&Value>, pointer: &str) -> Option<String> {
    text(value?.pointer(pointer))
}

/// The first of the given keys on a node that holds a non-empty string. Where a
/// provider ships the same field in camelCase and snake_case.
pub(crate) fn text_in(node: &Value, keys: &[&str]) -> Option<String> {
    keys.iter().find_map(|key| text(node.get(*key)))
}

/// Remaining share from a provider's used percentage, clamped to 0..=100:
/// every endpoint reports consumption, this crate reports what is left.
pub(crate) fn remaining_from_used(used_percent: f64) -> f32 {
    if !used_percent.is_finite() {
        return 0.0;
    }
    (100.0 - used_percent).clamp(0.0, 100.0) as f32
}

/// Remaining share from a spent/limit pair (money in cents, or counts).
pub(crate) fn remaining_from_amounts(spent: f64, limit: f64) -> f32 {
    if !limit.is_finite() || limit <= 0.0 {
        return 0.0;
    }
    (((limit - spent) / limit) * 100.0).clamp(0.0, 100.0) as f32
}

/// A window label from the length the provider reports (18000 becomes 5h).
pub(crate) fn window_label(seconds: i64) -> String {
    if seconds % 86_400 == 0 {
        format!("{}d", seconds / 86_400)
    } else if seconds % 3_600 == 0 {
        format!("{}h", seconds / 3_600)
    } else if seconds % 60 == 0 {
        format!("{}m", seconds / 60)
    } else {
        format!("{seconds}s")
    }
}

/// The head of a reply body, so an error message can name what arrived
/// without dumping a whole page into a log line.
pub(crate) fn snippet(body: &str) -> String {
    const SNIPPET_CHARS: usize = 200;
    let head: String = body.chars().take(SNIPPET_CHARS).collect();
    head.trim().to_string()
}
