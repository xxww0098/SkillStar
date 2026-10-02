//! Which member a group's rules try first.
//!
//! Rules are checked in order. Tokens, images, effort, and the calling agent
//! all have to hold. The first rule that holds moves its member to the front
//! of an already expanded list. The rest stay where they were.
//!
//! A rule that names an intent matches only when `request.intent` is that
//! verdict. This function does not ask a classifier. No match, or a rule
//! that names nobody on the list, leaves the expanded order alone.

use serde::Deserialize;
use serde_json::Value;

use crate::PLACEHOLDER_BEARER;
use crate::store::doc::ModelGatewayDoc;

const EFFORTS: [&str; 5] = ["low", "medium", "high", "xhigh", "max"];

/// One rule stored on a group. `use` is the member it wants first.
#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize)]
pub struct GroupRule {
    #[serde(rename = "use", default)]
    pub use_member: String,
    #[serde(default)]
    pub tokens: u64,
    #[serde(default)]
    pub images: bool,
    #[serde(default)]
    pub effort: String,
    #[serde(default)]
    pub agents: Vec<String>,
    #[serde(default)]
    pub intent: String,
}

/// What a rule can see in one request. `intent` is a classifier verdict.
/// Leave it empty when no classifier has accepted one.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RuleRequest {
    pub tokens: u64,
    pub images: bool,
    pub thinking: bool,
    pub effort: String,
    pub agent: String,
    pub intent: String,
}

/// Where a request says which agent sent it.
#[derive(Clone, Copy, Debug, Default)]
pub struct Caller<'a> {
    pub authorization: &'a str,
    pub api_key: &'a str,
    pub goog_key: &'a str,
    pub query_key: &'a str,
    pub user_agent: &'a str,
}

/// Move the first matching rule's member to the front.
///
/// `members` is the expanded order. An empty rule list returns that order.
pub fn order_with_rules(
    members: &[impl AsRef<str>],
    rules: &[GroupRule],
    request: &RuleRequest,
) -> Vec<String> {
    let members: Vec<String> = members
        .iter()
        .map(|member| member.as_ref().to_string())
        .collect();
    let Some(rule) = rules.iter().find(|rule| matches_now(rule, request)) else {
        return members;
    };
    let Some(index) = members.iter().position(|member| member == &rule.use_member) else {
        return members;
    };
    if index == 0 {
        return members;
    }
    let mut ordered = members;
    let chosen = ordered.remove(index);
    ordered.insert(0, chosen);
    ordered
}

/// Rules saved on `group/<id>` or a bare id. A missing file is an empty list.
/// This does not create or rewrite the file.
pub fn stored_rules(group_id: &str) -> Vec<GroupRule> {
    let doc = ModelGatewayDoc::open_lenient();
    let id = bare_id(group_id);
    doc.groups()
        .iter()
        .find(|group| group.id.trim() == id)
        .map(|group| rules_in(group.extra("rules")))
        .unwrap_or_default()
}

/// Agent named by `skillstar-<id>`, otherwise by the User-Agent product token.
pub fn request_agent(caller: &Caller<'_>) -> String {
    if let Some(id) = named_bearer(caller_key(caller)) {
        return agent_product(id);
    }
    if is_claude_desktop(caller.user_agent) {
        return "claude-desktop".to_string();
    }
    agent_product(caller.user_agent)
}

pub(crate) fn matches_now(rule: &GroupRule, request: &RuleRequest) -> bool {
    let intent = rule.intent.trim();
    if !intent.is_empty() && !intent.eq_ignore_ascii_case(request.intent.trim()) {
        return false;
    }
    if rule.tokens > 0 && request.tokens < rule.tokens {
        return false;
    }
    if rule.images && !request.images {
        return false;
    }
    if !effort_ok(rule.effort.trim(), request) {
        return false;
    }
    if !agent_ok(&rule.agents, &request.agent) {
        return false;
    }
    rule.tokens > 0
        || rule.images
        || !rule.effort.trim().is_empty()
        || !rule.agents.is_empty()
        || !intent.is_empty()
}

fn effort_ok(effort: &str, request: &RuleRequest) -> bool {
    match effort {
        "" => true,
        "on" => request.thinking || !request.effort.is_empty(),
        need => effort_rank(request.effort.trim())
            .is_some_and(|have| effort_rank(need).is_some_and(|need_rank| have >= need_rank)),
    }
}

fn effort_rank(effort: &str) -> Option<usize> {
    EFFORTS.iter().position(|level| *level == effort)
}

fn agent_ok(agents: &[String], agent: &str) -> bool {
    agents.is_empty() || agents.iter().any(|name| name.eq_ignore_ascii_case(agent))
}

/// The row-level `rules` value, shaped into rules. Entries that do not
/// deserialize are dropped, as they always were.
fn rules_in(rules: Option<&Value>) -> Vec<GroupRule> {
    rules
        .and_then(Value::as_array)
        .map(|rules| {
            rules
                .iter()
                .filter_map(|rule| serde_json::from_value(rule.clone()).ok())
                .collect()
        })
        .unwrap_or_default()
}

fn named_bearer(key: &str) -> Option<&str> {
    let rest = key.trim().strip_prefix(PLACEHOLDER_BEARER)?;
    let id = rest.strip_prefix('-')?.trim();
    if id.is_empty() { None } else { Some(id) }
}

fn caller_key<'a>(caller: &Caller<'a>) -> &'a str {
    let authorization = caller.authorization.trim();
    if !authorization.is_empty() {
        return authorization
            .strip_prefix("Bearer ")
            .unwrap_or(authorization)
            .trim();
    }
    if !caller.api_key.is_empty() {
        return caller.api_key;
    }
    if !caller.goog_key.is_empty() {
        return caller.goog_key;
    }
    caller.query_key
}

fn agent_product(raw: &str) -> String {
    let trimmed = raw.trim();
    let before_slash = trimmed.split_once('/').map_or(trimmed, |(name, _)| name);
    let name = before_slash
        .split_once(' ')
        .map_or(before_slash, |(name, _)| name);
    if name.is_empty() {
        "other".to_string()
    } else {
        name.to_string()
    }
}

fn is_claude_desktop(user_agent: &str) -> bool {
    user_agent.starts_with("Mozilla/") && user_agent.contains(" Claude/")
}

fn bare_id(group_id: &str) -> &str {
    group_id
        .trim()
        .strip_prefix(crate::GROUP_PREFIX)
        .unwrap_or(group_id.trim())
        .trim()
}
