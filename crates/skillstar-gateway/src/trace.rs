//! The last calls this process forwarded, and the hand-off into the
//! persistent usage ledger.
//!
//! The ring lives in memory. A restart starts it empty, and nothing under
//! the data directory is read back; it stays the UI's tail cache and the
//! degradation target for a failed ledger append. The ring's record is the
//! clock time, the agent, the model the agent asked for, the status, and the
//! completion-token count. It does not carry a secret or an upstream URL.
//!
//! `note_turn` is the ledger's hot path: it feeds the ring, then appends one
//! [`Record`](crate::ledger::Record) line. Usage and the answered model are
//! read from a JSON body whole, or from an SSE body's `data:` frames — the
//! last frame that carries usage is the tail frame that matters, the one
//! before the stream ends.

use std::sync::Mutex;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use serde_json::Value;

use crate::ledger::{ErrorKind, Record, TokenCounts};
use crate::route::rules::{Caller, request_agent};

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
    let mut recent = RECENT
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
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

/// The session-candidate header values, taken while the request is still
/// owned. `affinity::session_id` applies its precedence over the pairs.
pub(crate) fn session_headers(headers: &hyper::HeaderMap) -> Vec<(&'static str, String)> {
    crate::route::affinity::SESSION_HEADERS
        .iter()
        .filter_map(|name| {
            headers
                .get(*name)
                .and_then(|value| value.to_str().ok())
                .map(|value| (*name, value.to_string()))
        })
        .collect()
}

/// The session id of one request: its candidate headers first, then the
/// body-derived fallback, exactly as affinity ranks them.
pub(crate) fn session_of(sessions: &[(&'static str, String)], body: &[u8]) -> String {
    let pairs: Vec<(&str, &str)> = sessions
        .iter()
        .map(|(name, value)| (*name, value.as_str()))
        .collect();
    crate::route::affinity::session_id(&pairs, body)
}

/// What dispatch knew when a turn started. The response side — status, the
/// body the agent received, and the raw upstream bytes — reaches
/// [`note_turn`] as arguments when the turn ends, so the borrowed request
/// facts never have to outlive the forward.
pub(crate) struct TurnFacts<'a> {
    /// Unix milliseconds read when the request entered dispatch.
    pub at: i64,
    pub started: Instant,
    pub authorization: &'a str,
    pub user_agent: &'a str,
    /// `affinity::session_id` of the request: the session header that was
    /// present, or the body-derived id. Empty means unknown.
    pub session: &'a str,
    /// Ledger account label: a subscription id, `key:<fingerprint>`, or empty.
    pub account: &'a str,
    /// The inbound path.
    pub endpoint: &'a str,
    /// The inbound body; the asked model is read from it.
    pub inbound: &'a [u8],
}

/// Debug without the request's secrets. The authorization and the user agent
/// may carry a bearer token, and the inbound body is the agent's content;
/// none of them belongs in a Debug output, so none of them is printed.
impl std::fmt::Debug for TurnFacts<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TurnFacts")
            .field("at", &self.at)
            .field("session", &self.session)
            .field("account", &self.account)
            .field("endpoint", &self.endpoint)
            .finish_non_exhaustive()
    }
}

/// The response side of one finished turn, as the turn state machine hands
/// it to the ledger. `won` carries the winning candidate's attribution;
/// `error_kind` is the machine's word-list verdict, refining what the status
/// and the body alone settle.
pub(crate) struct TurnEnd<'a> {
    pub status: u16,
    pub body: &'a [u8],
    /// The reply as the upstream sent it, or `None` for a refusal the
    /// gateway generated itself.
    pub upstream_raw: Option<&'a [u8]>,
    /// `(catalog, account)` of the winning candidate, when routing ran and
    /// a candidate answered.
    pub won: Option<(&'a str, &'a str)>,
    /// The refined error kind. `None` falls back to
    /// [`ErrorKind::classify`](crate::ledger::ErrorKind::classify).
    pub error_kind: Option<ErrorKind>,
}

/// Record one finished turn: the ring first (it stays green whatever the
/// ledger does), then one ledger line. `upstream_raw` is the reply as the
/// upstream sent it, kept from before the protocol translation back — usage
/// and the answered model survive there even when the agent-side rebuild of
/// the body failed. `None` marks a refusal the gateway generated itself.
pub(crate) fn note_turn(facts: &TurnFacts<'_>, end: &TurnEnd<'_>) {
    note_forward(
        facts.authorization,
        facts.user_agent,
        facts.inbound,
        end.status,
        end.body,
    );
    let answered = answered_facts(end.body, end.upstream_raw);
    let (catalog, account) = end.won.unwrap_or(("", ""));
    let account = if account.is_empty() {
        facts.account.to_string()
    } else {
        account.to_string()
    };
    crate::ledger::append(&Record {
        at: facts.at,
        agent: request_agent(&Caller {
            authorization: facts.authorization,
            user_agent: facts.user_agent,
            ..Caller::default()
        }),
        session: facts.session.to_string(),
        model_asked: json_string(facts.inbound, "model"),
        model_answered: answered.model,
        // The winning candidate's catalog; empty when routing did not run.
        catalog: catalog.to_string(),
        account,
        tokens: answered.tokens,
        status: end.status,
        latency_ms: facts.started.elapsed().as_millis() as u64,
        error_kind: end
            .error_kind
            .or_else(|| ErrorKind::classify(end.status, end.body, end.upstream_raw.is_none())),
        endpoint: facts.endpoint.to_string(),
    });
}

/// Now in Unix milliseconds. A turn's `at` is read here at dispatch entry.
pub(crate) fn unix_millis() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.as_millis() as i64)
        .unwrap_or(0)
}

/// Usage and the model one response carries.
pub(crate) struct ResponseFacts {
    pub tokens: TokenCounts,
    pub model: String,
    /// Whether any usage object was seen at all. Absent usage is zero counts,
    /// not a zero reading.
    pub has_usage: bool,
}

/// Facts from the body the agent received, falling back to the raw upstream
/// bytes when the agent-side body has neither usage nor a model name.
fn answered_facts(body: &[u8], upstream_raw: Option<&[u8]>) -> ResponseFacts {
    let mut facts = response_facts(body);
    let Some(raw) = upstream_raw else {
        return facts;
    };
    if facts.has_usage && !facts.model.is_empty() {
        return facts;
    }
    let from_raw = response_facts(raw);
    if !facts.has_usage {
        facts.tokens = from_raw.tokens;
        facts.has_usage = from_raw.has_usage;
    }
    if facts.model.is_empty() {
        facts.model = from_raw.model;
    }
    facts
}

/// Read one response body: a JSON body whole, an SSE body by its frames.
fn response_facts(body: &[u8]) -> ResponseFacts {
    match serde_json::from_slice::<Value>(body) {
        Ok(value) => json_facts(&value),
        Err(_) => sse_facts(body),
    }
}

fn json_facts(value: &Value) -> ResponseFacts {
    let mut facts = ResponseFacts {
        tokens: TokenCounts::default(),
        model: model_of(value),
        has_usage: false,
    };
    if let Some(usage) = usage_of(value) {
        merge_usage(&mut facts.tokens, usage);
        facts.has_usage = true;
    }
    facts
}

/// Walk an SSE body's `data:` lines, newest wins: the frame before
/// `[DONE]` that carries usage is the tail frame the counts come from. A
/// conversation that names input tokens in its first frame and output tokens
/// in its last keeps both, because only fields a frame actually names
/// overwrite.
fn sse_facts(body: &[u8]) -> ResponseFacts {
    let mut facts = ResponseFacts {
        tokens: TokenCounts::default(),
        model: String::new(),
        has_usage: false,
    };
    for line in body.split(|byte| *byte == b'\n') {
        let line = line.strip_suffix(b"\r").unwrap_or(line);
        let Some(rest) = line.strip_prefix(b"data:") else {
            continue;
        };
        let data = rest.trim_ascii();
        if data.is_empty() || data == b"[DONE]" {
            continue;
        }
        let Ok(value) = serde_json::from_slice::<Value>(data) else {
            continue;
        };
        if let Some(usage) = usage_of(&value) {
            merge_usage(&mut facts.tokens, usage);
            facts.has_usage = true;
        }
        let model = model_of(&value);
        if !model.is_empty() {
            facts.model = model;
        }
    }
    facts
}

/// The usage object of one JSON value: top-level first (Chat, and
/// Anthropic's non-streamed reply), then inside `message` (Anthropic's
/// `message_start` frame).
fn usage_of(value: &Value) -> Option<&Value> {
    value
        .get("usage")
        .or_else(|| value.pointer("/message/usage"))
        .filter(|usage| usage.is_object())
}

fn model_of(value: &Value) -> String {
    value
        .get("model")
        .or_else(|| value.pointer("/message/model"))
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string()
}

/// Fold one usage object into the counts, both vendor vocabularies together.
/// Only fields the object names overwrite, so split frames add up.
fn merge_usage(tokens: &mut TokenCounts, usage: &Value) {
    if let Some(count) = named(usage, &["input_tokens", "prompt_tokens"]) {
        tokens.input = count;
    }
    if let Some(count) = named(usage, &["output_tokens", "completion_tokens"]) {
        tokens.output = count;
    }
    if let Some(count) = named(usage, &["cache_read_input_tokens"]) {
        tokens.cache_read = count;
    }
    if let Some(count) = named(usage, &["cache_creation_input_tokens"]) {
        tokens.cache_write = count;
    }
    if let Some(count) = named(usage, &["reasoning_tokens"]) {
        tokens.reasoning = count;
    }
    if let Some(count) = usage
        .pointer("/prompt_tokens_details/cached_tokens")
        .and_then(Value::as_u64)
    {
        tokens.cache_read = count;
    }
    if let Some(count) = usage
        .pointer("/completion_tokens_details/reasoning_tokens")
        .and_then(Value::as_u64)
    {
        tokens.reasoning = count;
    }
}

/// The first present, non-negative integer among `keys`.
fn named(usage: &Value, keys: &[&str]) -> Option<u64> {
    keys.iter()
        .find_map(|key| usage.get(*key).and_then(Value::as_u64))
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

    #[test]
    fn turn_facts_debug_drops_the_bearer_secret() {
        let started = Instant::now();
        let facts = TurnFacts {
            at: 1_700_000_000_000,
            started,
            authorization: "Bearer sk-secret-value",
            user_agent: "agent/1.0",
            session: "s1",
            account: "key:0123abcd",
            endpoint: "/v1/chat/completions",
            inbound: br#"{"model":"m1","messages":[{"role":"user","content":"private"}]}"#,
        };
        let text = format!("{facts:?}");
        assert!(!text.contains("sk-secret-value"), "{text}");
        assert!(!text.contains("agent/1.0"), "{text}");
        assert!(!text.contains("private"), "{text}");
        assert!(!text.contains("Bearer"), "{text}");
        assert!(text.contains("key:0123abcd"), "the account fingerprint prints: {text}");
        assert!(text.contains("/v1/chat/completions"), "{text}");
    }

    #[test]
    fn json_bodies_read_usage_and_model_both_vocabularies() {
        let chat = response_facts(
            br#"{"id":"c1","model":"m1","usage":{"prompt_tokens":10,"completion_tokens":5,"prompt_tokens_details":{"cached_tokens":4},"completion_tokens_details":{"reasoning_tokens":2}}}"#,
        );
        assert!(chat.has_usage);
        assert_eq!(chat.model, "m1");
        assert_eq!(
            chat.tokens,
            TokenCounts {
                input: 10,
                output: 5,
                cache_read: 4,
                reasoning: 2,
                ..TokenCounts::default()
            }
        );
        let anthropic = response_facts(
            br#"{"type":"message","model":"m2","usage":{"input_tokens":7,"output_tokens":3,"cache_read_input_tokens":6,"cache_creation_input_tokens":2}}"#,
        );
        assert!(anthropic.has_usage);
        assert_eq!(anthropic.model, "m2");
        assert_eq!(
            anthropic.tokens,
            TokenCounts {
                input: 7,
                output: 3,
                cache_read: 6,
                cache_write: 2,
                ..TokenCounts::default()
            }
        );
        // No usage object: zeros mean absence, and has_usage says so.
        let bare = response_facts(br#"{"model":"m1","choices":[]}"#);
        assert!(!bare.has_usage);
        assert_eq!(bare.tokens, TokenCounts::default());
        assert_eq!(bare.model, "m1");
        let empty = response_facts(b"no upstream");
        assert!(!empty.has_usage);
        assert_eq!(empty.model, "");
    }

    #[test]
    fn sse_bodies_read_the_usage_tail_frame() {
        let stream = concat!(
            "event: ping\n\n",
            "data: {\"id\":\"c1\",\"model\":\"m9\",\"choices\":[{\"delta\":{\"role\":\"assistant\",\"content\":\"He\"}}]}\n\n",
            "data: {\"id\":\"c1\",\"model\":\"m9\",\"choices\":[{\"delta\":{\"content\":\"llo\"}}],\"usage\":null}\n\n",
            "data: {\"id\":\"c1\",\"model\":\"m9\",\"choices\":[],\"usage\":{\"prompt_tokens\":7,\"completion_tokens\":3,\"prompt_tokens_details\":{\"cached_tokens\":4},\"completion_tokens_details\":{\"reasoning_tokens\":2}}}\n\n",
            "data: [DONE]\n\n",
        );
        let facts = response_facts(stream.as_bytes());
        assert!(facts.has_usage);
        assert_eq!(facts.model, "m9");
        assert_eq!(
            facts.tokens,
            TokenCounts {
                input: 7,
                output: 3,
                cache_read: 4,
                reasoning: 2,
                ..TokenCounts::default()
            }
        );
    }

    #[test]
    fn sse_frames_merge_the_fields_they_name() {
        // Anthropic's own stream shape: input in message_start, output in the
        // message_delta before message_stop.
        let stream = concat!(
            "event: message_start\n",
            "data: {\"type\":\"message_start\",\"message\":{\"model\":\"m2\",\"usage\":{\"input_tokens\":9,\"cache_read_input_tokens\":5}}}\n\n",
            "event: message_delta\n",
            "data: {\"type\":\"message_delta\",\"delta\":{\"stop_reason\":\"end_turn\"},\"usage\":{\"output_tokens\":4}}\n\n",
            "event: message_stop\n",
            "data: {\"type\":\"message_stop\"}\n\n",
        );
        let facts = response_facts(stream.as_bytes());
        assert!(facts.has_usage);
        assert_eq!(facts.model, "m2");
        assert_eq!(
            facts.tokens,
            TokenCounts {
                input: 9,
                output: 4,
                cache_read: 5,
                ..TokenCounts::default()
            }
        );
    }

    #[test]
    fn raw_upstream_bytes_answer_what_the_agent_body_cannot() {
        // The rebuild back to the agent failed; the raw reply still says
        // what was spent.
        let raw = b"data: {\"model\":\"m2\",\"usage\":{\"prompt_tokens\":8,\"completion_tokens\":6}}\n\ndata: [DONE]\n\n";
        let facts = answered_facts(b"upstream body", Some(raw));
        assert!(facts.has_usage);
        assert_eq!(facts.model, "m2");
        assert_eq!(facts.tokens.input, 8);
        assert_eq!(facts.tokens.output, 6);
        // The agent-side body wins where it already answers.
        let agent_body = br#"{"model":"m3","usage":{"input_tokens":1,"output_tokens":1}}"#;
        let facts = answered_facts(agent_body, Some(raw));
        assert_eq!(facts.model, "m3");
        assert_eq!(facts.tokens.input, 1);
        // A local refusal has no raw bytes to fall back on.
        let facts = answered_facts(b"no upstream", None);
        assert!(!facts.has_usage);
        assert_eq!(facts.model, "");
    }
}
