//! Which intent a group's rules may use, asked once as a turn opens.
//!
//! The model id comes from the group's `classifier` field. An empty id does
//! not match an intent rule and does not look for a local model. The caller
//! posts [`ClassifyCall`] — this module does not open a socket, because the
//! gateway does not own provider URLs. A verdict at or above [`JEV_SURE`]
//! names that intent. Anything lower, a failure, a timeout, or an answer that
//! is not one of the intents names nothing.

use std::collections::HashMap;
use std::sync::{LazyLock, Mutex};
use std::time::{Duration, SystemTime};

use serde_json::Value;

use crate::STICK_KEEP;
use crate::store::doc::ModelGatewayDoc;
use super::rules::{GroupRule, RuleRequest, matches_now, order_with_rules};

/// How sure the classifier must be before an intent counts.
pub const JEV_SURE: f64 = 0.4;
/// How long one ask may take.
pub const CLASSIFY_TIMEOUT: Duration = Duration::from_secs(8);
/// How long the same message and intent list keep an answer.
pub const CLASSIFY_KEEP: Duration = Duration::from_secs(10 * 60);
/// How long a failed classifier sits out.
pub const CLASSIFY_REST: Duration = Duration::from_secs(30);
/// User-Agent on the gateway's own classifier call.
pub const ROUTER_USER_AGENT: &str = "skillstar-router/1";

/// What the caller posts to the classifier model.
pub struct ClassifyCall {
    pub user_agent: &'static str,
    pub timeout: Duration,
    pub body: Vec<u8>,
}

/// What the classifier model sent back.
#[derive(Clone)]
pub enum ClassifyReply {
    /// A body the model returned.
    Bytes(Vec<u8>),
    /// The call failed before an answer.
    Failed,
    /// The call did not answer within [`CLASSIFY_TIMEOUT`].
    TimedOut,
}

/// One turn the caller is routing. `opening` is the user's first message.
pub struct ClassifyTurn<'a> {
    pub id: &'a str,
    pub opening: bool,
    pub message: &'a str,
    pub at: SystemTime,
}

struct Kept {
    intent: String,
    at: SystemTime,
}

struct Memory {
    kept: HashMap<String, Kept>,
    failed: HashMap<String, SystemTime>,
    turns: HashMap<String, Kept>,
}

static MEMORY: LazyLock<Mutex<Memory>> = LazyLock::new(|| {
    Mutex::new(Memory {
        kept: HashMap::new(),
        failed: HashMap::new(),
        turns: HashMap::new(),
    })
});

enum Read {
    Hit(String),
    None,
    Garbage,
}

/// Model id saved on `group/<id>` or a bare id. Missing means do not ask.
pub fn stored_classifier(group_id: &str) -> String {
    let doc = ModelGatewayDoc::open_lenient();
    let id = bare_id(group_id);
    doc.groups()
        .iter()
        .find(|group| group.id.trim() == id)
        .and_then(|group| group.extra("classifier"))
        .and_then(Value::as_str)
        .map(str::trim)
        .unwrap_or("")
        .to_string()
}

/// Move the member whose intent the classifier accepted, once per turn.
///
/// `model` is `provider/model` or `group/<id>`. Empty skips the ask. A turn
/// that is already open reuses the opening verdict and does not ask again.
pub fn order_with_classifier(
    members: &[impl AsRef<str>],
    rules: &[GroupRule],
    request: &RuleRequest,
    model: &str,
    turn: &ClassifyTurn<'_>,
    mut ask: impl FnMut(&ClassifyCall) -> ClassifyReply,
) -> Vec<String> {
    let intent = if turn.opening {
        let intent = opening_verdict(rules, request, model.trim(), turn, &mut ask);
        remember(turn.id, &intent, turn.at);
        intent
    } else {
        recall(turn.id, turn.at)
    };
    let mut judged = request.clone();
    judged.intent = intent;
    order_with_rules(members, rules, &judged)
}

fn opening_verdict(
    rules: &[GroupRule],
    request: &RuleRequest,
    model: &str,
    turn: &ClassifyTurn<'_>,
    ask: &mut impl FnMut(&ClassifyCall) -> ClassifyReply,
) -> String {
    let intents = intents_to_ask(rules, request);
    if model.is_empty() || intents.is_empty() {
        return String::new();
    }
    let key = cache_key(model, &intents, turn.message);
    {
        let memory = lock();
        if let Some(kept) = memory.kept.get(&key)
            && within(kept.at, turn.at, CLASSIFY_KEEP)
        {
            return kept.intent.clone();
        }
        if let Some(failed_at) = memory.failed.get(model)
            && within(*failed_at, turn.at, CLASSIFY_REST)
        {
            return String::new();
        }
    }
    let call = ClassifyCall {
        user_agent: ROUTER_USER_AGENT,
        timeout: CLASSIFY_TIMEOUT,
        body: call_body(model, &intents, turn.message),
    };
    match ask(&call) {
        ClassifyReply::Failed | ClassifyReply::TimedOut => {
            lock().failed.insert(model.to_string(), turn.at);
            String::new()
        }
        ClassifyReply::Bytes(bytes) => match read_answer(&bytes, &intents) {
            Read::Garbage => String::new(),
            Read::None => {
                store_answer(&key, model, String::new(), turn.at);
                String::new()
            }
            Read::Hit(intent) => {
                store_answer(&key, model, intent.clone(), turn.at);
                intent
            }
        },
    }
}

fn intents_to_ask(rules: &[GroupRule], request: &RuleRequest) -> Vec<String> {
    let mut out = Vec::new();
    let mut bare = request.clone();
    bare.intent.clear();
    for rule in rules {
        let named = rule.intent.trim();
        if named.is_empty() {
            if matches_now(rule, &bare) {
                break;
            }
            continue;
        }
        let mut as_this = bare.clone();
        as_this.intent = named.to_string();
        if !matches_now(rule, &as_this) {
            continue;
        }
        if out
            .iter()
            .any(|have: &String| have.eq_ignore_ascii_case(named))
        {
            continue;
        }
        out.push(named.to_string());
    }
    out
}

fn call_body(model: &str, intents: &[String], message: &str) -> Vec<u8> {
    serde_json::to_vec(&serde_json::json!({
        "model": model,
        "intents": intents,
        "message": message,
    }))
    .unwrap_or_default()
}

fn read_answer(bytes: &[u8], intents: &[String]) -> Read {
    let Ok(value) = serde_json::from_slice::<Value>(bytes) else {
        return Read::Garbage;
    };
    let Some(intent) = value.get("intent").and_then(Value::as_str) else {
        return Read::Garbage;
    };
    let Some(confidence) = value.get("confidence").and_then(Value::as_f64) else {
        return Read::Garbage;
    };
    if !confidence.is_finite() {
        return Read::Garbage;
    }
    if confidence < JEV_SURE {
        return Read::None;
    }
    let intent = intent.trim();
    if intent.is_empty() {
        return Read::None;
    }
    if intents
        .iter()
        .any(|candidate| candidate.eq_ignore_ascii_case(intent))
    {
        Read::Hit(intent.to_string())
    } else {
        Read::Garbage
    }
}

fn store_answer(key: &str, model: &str, intent: String, at: SystemTime) {
    let mut memory = lock();
    memory.failed.remove(model);
    memory.kept.insert(key.to_string(), Kept { intent, at });
    prune(&mut memory, at);
}

fn remember(id: &str, intent: &str, at: SystemTime) {
    if id.is_empty() {
        return;
    }
    let mut memory = lock();
    memory.turns.insert(
        id.to_string(),
        Kept {
            intent: intent.to_string(),
            at,
        },
    );
    prune(&mut memory, at);
}

fn recall(id: &str, at: SystemTime) -> String {
    lock()
        .turns
        .get(id)
        .filter(|kept| within(kept.at, at, STICK_KEEP))
        .map(|kept| kept.intent.clone())
        .unwrap_or_default()
}

fn cache_key(model: &str, intents: &[String], message: &str) -> String {
    let mut key = String::new();
    key.push_str(model);
    key.push('\0');
    for intent in intents {
        key.push_str(&intent.to_ascii_lowercase());
        key.push('\0');
    }
    key.push_str(message);
    key
}

fn within(earlier: SystemTime, now: SystemTime, window: Duration) -> bool {
    now.duration_since(earlier)
        .is_ok_and(|elapsed| elapsed < window)
}

fn prune(memory: &mut Memory, now: SystemTime) {
    if memory.kept.len() > 4096 {
        memory
            .kept
            .retain(|_, kept| within(kept.at, now, CLASSIFY_KEEP));
    }
    if memory.turns.len() > 4096 {
        memory
            .turns
            .retain(|_, kept| within(kept.at, now, STICK_KEEP));
    }
}

fn lock() -> std::sync::MutexGuard<'static, Memory> {
    MEMORY
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn bare_id(group_id: &str) -> &str {
    group_id
        .trim()
        .strip_prefix(crate::GROUP_PREFIX)
        .unwrap_or(group_id.trim())
        .trim()
}
