//! Whether a conversation stays with the candidate that answered it last.
//!
//! Empty mode is auto: stay through the rest of a turn, and across turns
//! only while the last answer read enough of the vendor cache and has not
//! gone cold. Session stays until the memory expires. Turn stays only while
//! the agent is still inside the turn. Off never stays.
//!
//! The caller passes the mode, the last stick, the turn, and the clock.
//! A resting stick is labeled and left in place; skipping it is later.

use std::time::{Duration, SystemTime};

use serde_json::Value;

use super::order::{RouteCandidate, USED_SHARE};

/// Tokens read from the vendor cache that make a conversation worth keeping
/// across turns.
pub const CACHE_WORTH: u64 = 1024;

/// How long a cached prompt is treated as still warm.
pub const CACHE_COLD: Duration = Duration::from_secs(5 * 60);

/// How long the last answerer is remembered.
pub const STICK_KEEP: Duration = Duration::from_secs(24 * 60 * 60);

/// Header names a session can arrive under, in precedence order:
/// `X-Skillstar-Session` wins, then the agent-native headers in list order.
/// The dispatch-side session pipe reads the same list.
pub(crate) const SESSION_HEADERS: &[&str] = &[
    "X-Skillstar-Session",
    "x-opencode-session",
    "x-session-affinity",
    "x-session-id",
    "session_id",
    "session-id",
    "x-claude-code-session-id",
];

/// How long a conversation stays with the candidate that answered it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AffinityMode {
    Auto,
    Session,
    Turn,
    Off,
}

impl AffinityMode {
    /// `session`, `turn`, and `off` stay themselves. Every other string,
    /// including empty, is auto.
    pub fn parse(raw: &str) -> Self {
        match raw {
            "session" => Self::Session,
            "turn" => Self::Turn,
            "off" => Self::Off,
            _ => Self::Auto,
        }
    }

    /// File and control spelling. Auto is the word the control sends; the
    /// file omits that word.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::Session => "session",
            Self::Turn => "turn",
            Self::Off => "off",
        }
    }

    /// The four control values. Empty and any other word are rejected here.
    /// [`parse`](Self::parse) still treats those as auto when reading a file.
    pub fn from_control(raw: &str) -> Option<Self> {
        match raw {
            "auto" => Some(Self::Auto),
            "session" => Some(Self::Session),
            "turn" => Some(Self::Turn),
            "off" => Some(Self::Off),
            _ => None,
        }
    }
}

/// Why this request did or did not stay. The strings match the reference
/// comments: `session`, `turn`, `cache`, `off`, `first`, `new-turn`,
/// `no-cache`, `cold`, `resting`, `spent`, `gone`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AffinityWhy {
    Session,
    Turn,
    Cache,
    Off,
    First,
    NewTurn,
    NoCache,
    Cold,
    Resting,
    Spent,
    Gone,
}

impl AffinityWhy {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Session => "session",
            Self::Turn => "turn",
            Self::Cache => "cache",
            Self::Off => "off",
            Self::First => "first",
            Self::NewTurn => "new-turn",
            Self::NoCache => "no-cache",
            Self::Cold => "cold",
            Self::Resting => "resting",
            Self::Spent => "spent",
            Self::Gone => "gone",
        }
    }

    fn keeps(self) -> bool {
        matches!(self, Self::Session | Self::Turn | Self::Cache)
    }
}

/// The last answerer, as the caller remembers it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AffinityStick {
    pub who: String,
    pub at: SystemTime,
    pub cache_read: u64,
    pub resting: bool,
}

/// Where this request sits in the conversation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AffinityTurn {
    pub within: bool,
}

/// Stay decision for one request. `order` is the candidate ids, with the
/// answerer moved first only when `kept` is set.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AffinityChoice {
    pub why: AffinityWhy,
    pub kept: bool,
    pub order: Vec<String>,
}

/// Decide whether `stick` stays in front of `candidates`.
///
/// A stick older than [`STICK_KEEP`] is treated as absent. Fewer than one
/// fresh stick leaves the list as given.
pub fn affinity(
    mode: AffinityMode,
    candidates: &[RouteCandidate<'_>],
    stick: Option<&AffinityStick>,
    turn: AffinityTurn,
    now: SystemTime,
) -> AffinityChoice {
    let fresh = stick.filter(|stick| age(now, stick.at) <= STICK_KEEP);
    let index = fresh.and_then(|stick| {
        candidates
            .iter()
            .position(|candidate| candidate.id == stick.who)
    });
    let why = why_of(mode, candidates, fresh, index, turn, now);
    let mut order: Vec<String> = candidates
        .iter()
        .map(|candidate| candidate.id.to_string())
        .collect();
    let kept = why.keeps();
    if kept && let Some(index) = index {
        order = move_front(order, index);
    }
    AffinityChoice { why, kept, order }
}

/// Move `who` to the front of an order that routing already produced.
///
/// Off, and every other reason that does not keep, returns `order` unchanged.
pub fn keep_first(order: &[String], who: &str, kept: bool) -> Vec<String> {
    if !kept {
        return order.to_vec();
    }
    let Some(index) = order.iter().position(|id| id == who) else {
        return order.to_vec();
    };
    move_front(order.to_vec(), index)
}

/// Session id for a stick key.
///
/// `X-Skillstar-Session` wins, then the agent headers in list order.
/// `X-Magpie-Session` is not a session header. With none of those set, the
/// id is derived from the first user turn so a later turn of the same
/// conversation hits the same stick.
pub fn session_id(headers: &[(&str, &str)], body: &[u8]) -> String {
    for name in SESSION_HEADERS {
        if let Some(value) = header(headers, name) {
            let trimmed = value.trim();
            if !trimmed.is_empty() {
                return trimmed.to_string();
            }
        }
    }
    derived_session(body)
}

fn why_of(
    mode: AffinityMode,
    candidates: &[RouteCandidate<'_>],
    stick: Option<&AffinityStick>,
    index: Option<usize>,
    turn: AffinityTurn,
    now: SystemTime,
) -> AffinityWhy {
    if mode == AffinityMode::Off {
        return AffinityWhy::Off;
    }
    let Some(stick) = stick else {
        return AffinityWhy::First;
    };
    let Some(index) = index else {
        return AffinityWhy::Gone;
    };
    if stick.resting {
        return AffinityWhy::Resting;
    }
    if spent(&candidates[index]) {
        return AffinityWhy::Spent;
    }
    if mode == AffinityMode::Session {
        return AffinityWhy::Session;
    }
    if turn.within {
        return AffinityWhy::Turn;
    }
    if mode == AffinityMode::Turn {
        return AffinityWhy::NewTurn;
    }
    if stick.cache_read < CACHE_WORTH {
        return AffinityWhy::NoCache;
    }
    if age(now, stick.at) > CACHE_COLD {
        return AffinityWhy::Cold;
    }
    AffinityWhy::Cache
}

fn spent(candidate: &RouteCandidate<'_>) -> bool {
    candidate
        .allowance
        .is_some_and(|snapshot| snapshot.percent >= USED_SHARE)
}

fn age(now: SystemTime, at: SystemTime) -> Duration {
    now.duration_since(at).unwrap_or(Duration::ZERO)
}

fn move_front(mut order: Vec<String>, index: usize) -> Vec<String> {
    if index == 0 || index >= order.len() {
        return order;
    }
    let who = order.remove(index);
    order.insert(0, who);
    order
}

fn header<'a>(headers: &[(&'a str, &'a str)], name: &str) -> Option<&'a str> {
    headers
        .iter()
        .find(|(key, _)| key.eq_ignore_ascii_case(name))
        .map(|(_, value)| *value)
}

fn derived_session(body: &[u8]) -> String {
    let bytes = serde_json::from_slice::<Value>(body)
        .ok()
        .and_then(|value| first_turn_bytes(&value))
        .unwrap_or_else(|| body.to_vec());
    format!("skillstar-{:016x}", fnv1a64(&bytes))
}

fn first_turn_bytes(value: &Value) -> Option<Vec<u8>> {
    let (items, gemini) = nonempty(value, "messages")
        .map(|items| (items, false))
        .or_else(|| nonempty(value, "contents").map(|items| (items, true)))
        .or_else(|| nonempty(value, "input").map(|items| (items, false)))?;
    let chosen = items
        .iter()
        .find(|item| is_user(item, gemini))
        .unwrap_or(&items[0]);
    serde_json::to_vec(chosen).ok()
}

fn nonempty<'a>(value: &'a Value, key: &str) -> Option<&'a Vec<Value>> {
    value
        .get(key)
        .and_then(Value::as_array)
        .filter(|items| !items.is_empty())
}

fn is_user(item: &Value, gemini: bool) -> bool {
    let role = item.get("role").and_then(Value::as_str).unwrap_or("");
    role == "user" || (gemini && role.is_empty())
}

fn fnv1a64(bytes: &[u8]) -> u64 {
    let mut hash = 0xcbf29ce484222325u64;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash
}
