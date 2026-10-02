//! Session affinity stays with the last answerer, or leaves the route order alone.

use std::fs;
use std::path::PathBuf;
use std::time::{Duration, SystemTime};

use serde_json::Value;
use skillstar_gateway::{
    AffinityChoice, AffinityMode, AffinityStick, AffinityTurn, AffinityWhy, AllowanceSnapshot,
    RouteCandidate, RouteMode, affinity, keep_first, route_mode, session_id,
};

fn epoch() -> SystemTime {
    SystemTime::UNIX_EPOCH + Duration::from_secs(1_700_000_000)
}

#[test]
fn affinity_session() {
    let fixture = fixture("session.json");
    let choice = decide(&fixture, None);
    assert_choice(&fixture, &choice);
    let candidates = candidates_of(&fixture);
    let (routed, _) = route_mode(RouteMode::Order, &candidates, 0);
    assert_eq!(
        keep_first(&routed, "b", choice.kept),
        vec!["b".to_string(), "a".to_string(), "c".to_string()]
    );
}

#[test]
fn affinity_turn() {
    let fixture = fixture("turn.json");
    assert_choice(&fixture, &decide(&fixture, None));
    let across = &fixture["across"];
    let choice = decide(&fixture, across["within"].as_bool());
    assert_eq!(choice.why.as_str(), across["why"].as_str().unwrap());
    assert_eq!(choice.kept, across["kept"].as_bool().unwrap());
    assert_eq!(choice.order, strings(&across["expect"]));
}

#[test]
fn affinity_off() {
    let fixture = fixture("off.json");
    let choice = decide(&fixture, None);
    assert_choice(&fixture, &choice);
    let candidates = candidates_of(&fixture);
    let (routed, next) = route_mode(RouteMode::Smart, &candidates, 3);
    assert_eq!(next, 3, "off does not advance rotate");
    let who = fixture["stick"]["who"].as_str().unwrap();
    assert_eq!(keep_first(&routed, who, choice.kept), routed);
    assert_ne!(
        routed, choice.order,
        "smart order differs, and off leaves it"
    );
}

#[test]
fn affinity_cache() {
    assert_fixture("cache.json");
}

#[test]
fn affinity_cold() {
    assert_fixture("cold.json");
    let now = epoch();
    let candidates = pair();
    let turn = AffinityTurn { within: false };
    let warm = stick_at("b", now - Duration::from_secs(5 * 60), 1024);
    let choice = affinity(AffinityMode::Auto, &candidates, Some(&warm), turn, now);
    assert_eq!(choice.why, AffinityWhy::Cache, "five minutes is still warm");
    let cold = stick_at(
        "b",
        now - Duration::from_secs(5 * 60) - Duration::from_millis(1),
        1024,
    );
    let choice = affinity(AffinityMode::Auto, &candidates, Some(&cold), turn, now);
    assert_eq!(choice.why, AffinityWhy::Cold);
    assert!(!choice.kept);
}

#[test]
fn affinity_no_cache() {
    assert_fixture("no-cache.json");
}

#[test]
fn affinity_first() {
    assert_fixture("first.json");
}

#[test]
fn affinity_expires_after_24h() {
    let now = epoch();
    let candidates = pair();
    let turn = AffinityTurn { within: false };
    let day = Duration::from_secs(24 * 60 * 60);
    let still = stick_at("b", now - day, 4096);
    let choice = affinity(AffinityMode::Session, &candidates, Some(&still), turn, now);
    assert_eq!(
        choice.why,
        AffinityWhy::Session,
        "exactly 24 hours still hits"
    );
    assert_eq!(choice.order, vec!["b".to_string(), "a".to_string()]);
    let gone = stick_at("b", now - day - Duration::from_millis(1), 4096);
    let choice = affinity(AffinityMode::Session, &candidates, Some(&gone), turn, now);
    assert_eq!(choice.why, AffinityWhy::First);
    assert!(!choice.kept);
    assert_eq!(choice.order, vec!["a".to_string(), "b".to_string()]);
}

#[test]
fn affinity_resting_is_not_skipped() {
    let now = epoch();
    let candidates = pair();
    let resting = AffinityStick {
        who: "b".to_string(),
        at: now,
        cache_read: 4096,
        resting: true,
    };
    let choice = affinity(
        AffinityMode::Session,
        &candidates,
        Some(&resting),
        AffinityTurn { within: false },
        now,
    );
    assert_eq!(choice.why, AffinityWhy::Resting);
    assert!(!choice.kept);
    assert_eq!(choice.order, vec!["a".to_string(), "b".to_string()]);
}

#[test]
fn affinity_spent_is_not_kept() {
    let now = epoch();
    let mut candidates = pair();
    candidates[1].allowance = Some(AllowanceSnapshot {
        percent: 98.0,
        renews_at: None,
    });
    let stick = stick_at("b", now, 4096);
    let choice = affinity(
        AffinityMode::Session,
        &candidates,
        Some(&stick),
        AffinityTurn { within: false },
        now,
    );
    assert_eq!(choice.why, AffinityWhy::Spent);
    assert!(!choice.kept);
    candidates[1].allowance = Some(AllowanceSnapshot {
        percent: 97.0,
        renews_at: None,
    });
    let choice = affinity(
        AffinityMode::Session,
        &candidates,
        Some(&stick),
        AffinityTurn { within: false },
        now,
    );
    assert_eq!(choice.why, AffinityWhy::Session);
    assert!(choice.kept);
}

#[test]
fn affinity_gone_when_the_answerer_left() {
    let now = epoch();
    let candidates = pair();
    let stick = stick_at("missing", now, 4096);
    let choice = affinity(
        AffinityMode::Session,
        &candidates,
        Some(&stick),
        AffinityTurn { within: false },
        now,
    );
    assert_eq!(choice.why, AffinityWhy::Gone);
    assert_eq!(choice.order, vec!["a".to_string(), "b".to_string()]);
}

#[test]
fn affinity_ignores_magpie_session_header() {
    let body = br#"{"messages":[{"role":"user","content":"hi"}]}"#;
    let only_magpie = session_id(&[("X-Magpie-Session", "magpie-session")], body);
    assert_ne!(only_magpie, "magpie-session");
    assert!(only_magpie.starts_with("skillstar-"));
    assert_eq!(
        only_magpie,
        session_id(&[("X-Magpie-Session", "other")], body)
    );
    assert_eq!(
        session_id(
            &[
                ("X-Magpie-Session", "magpie-session"),
                ("X-Skillstar-Session", " mine ")
            ],
            body
        ),
        "mine"
    );
    assert_eq!(
        session_id(
            &[
                ("x-skillstar-session", "own"),
                ("X-Session-Affinity", "ses_1")
            ],
            body
        ),
        "own"
    );
    assert_eq!(
        session_id(&[("X-Session-Affinity", "ses_1")], b"{}"),
        "ses_1"
    );
    assert_eq!(
        session_id(
            &[("X-Skillstar-Session", "   "), ("x-session-id", "agent")],
            body
        ),
        "agent"
    );
}

#[test]
fn affinity_session_follows_the_first_user_turn() {
    let turn1 = br#"{"messages":[{"role":"system","content":"s"},{"role":"user","content":"fix the bug"}]}"#;
    let turn2 = br#"{"messages":[{"role":"system","content":"s"},{"role":"user","content":"fix the bug"},{"role":"assistant","content":"done"},{"role":"user","content":"thanks"}]}"#;
    let other =
        br#"{"messages":[{"role":"system","content":"s"},{"role":"user","content":"write docs"}]}"#;
    let a = session_id(&[], turn1);
    let b = session_id(&[], turn2);
    let c = session_id(&[], other);
    assert_eq!(a, b);
    assert_ne!(a, c);
    assert!(a.starts_with("skillstar-"));
    assert!(!a.starts_with("magpie-"));

    let responses1 = br#"{"input":[{"role":"user","content":"hi"}]}"#;
    let responses2 =
        br#"{"input":[{"role":"user","content":"hi"},{"type":"function_call_output","output":"x"}]}"#;
    assert_eq!(session_id(&[], responses1), session_id(&[], responses2));

    let gemini1 = br#"{"model":"m","contents":[{"role":"user","parts":[{"text":"fix the bug"}]}]}"#;
    let gemini2 = br#"{"model":"m","systemInstruction":{"parts":[{"text":"help"}]},"contents":[{"role":"user","parts":[{"text":"fix the bug"}]},{"role":"model","parts":[{"text":"done"}]},{"role":"user","parts":[{"text":"thanks"}]}]}"#;
    let gemini_other =
        br#"{"model":"m","contents":[{"role":"user","parts":[{"text":"write docs"}]}]}"#;
    assert_eq!(session_id(&[], gemini1), session_id(&[], gemini2));
    assert_ne!(session_id(&[], gemini1), session_id(&[], gemini_other));

    let roleless = r#"{"parts":[{"text":"fix the bug"}]}"#;
    let roleless1 = format!(r#"{{"contents":[{roleless}]}}"#);
    let roleless2 = format!(
        r#"{{"contents":[{roleless},{{"role":"model","parts":[{{"text":"done"}}]}},{{"role":"user","parts":[{{"text":"thanks"}}]}}]}}"#
    );
    let roleless_other = br#"{"contents":[{"parts":[{"text":"write docs"}]},{"role":"user","parts":[{"text":"thanks"}]}]}"#;
    assert_eq!(
        session_id(&[], roleless1.as_bytes()),
        session_id(&[], roleless2.as_bytes())
    );
    assert_ne!(
        session_id(&[], roleless1.as_bytes()),
        session_id(&[], roleless_other)
    );

    let chat_preface = br#"{"messages":[{"content":"preface"},{"role":"user","content":"hi"}]}"#;
    let chat_user = br#"{"messages":[{"role":"user","content":"hi"}]}"#;
    assert_eq!(session_id(&[], chat_preface), session_id(&[], chat_user));
    let responses_preface = br#"{"input":[{"content":"preface"},{"role":"user","content":"hi"}]}"#;
    let responses_user = br#"{"input":[{"role":"user","content":"hi"}]}"#;
    assert_eq!(
        session_id(&[], responses_preface),
        session_id(&[], responses_user)
    );
}

fn assert_fixture(name: &str) {
    let fixture = fixture(name);
    assert_choice(&fixture, &decide(&fixture, None));
}

fn assert_choice(fixture: &Value, choice: &AffinityChoice) {
    assert_eq!(
        choice.why.as_str(),
        fixture["why"].as_str().unwrap(),
        "{choice:?}"
    );
    assert_eq!(choice.kept, fixture["kept"].as_bool().unwrap());
    assert_eq!(choice.order, strings(&fixture["expect"]));
}

fn decide(fixture: &Value, within: Option<bool>) -> AffinityChoice {
    let now = epoch();
    let mode = AffinityMode::parse(fixture["mode"].as_str().unwrap());
    let candidates = candidates_of(fixture);
    let stick = stick_of(&fixture["stick"], now);
    let within = within.unwrap_or_else(|| fixture["within"].as_bool().unwrap());
    affinity(
        mode,
        &candidates,
        stick.as_ref(),
        AffinityTurn { within },
        now,
    )
}

fn candidates_of(fixture: &Value) -> Vec<RouteCandidate<'_>> {
    fixture["candidates"]
        .as_array()
        .unwrap()
        .iter()
        .map(|candidate| RouteCandidate {
            id: candidate["id"].as_str().unwrap(),
            allowance: candidate
                .get("used")
                .and_then(Value::as_f64)
                .map(|used| AllowanceSnapshot {
                    percent: used,
                    renews_at: None,
                }),
        })
        .collect()
}

fn stick_of(value: &Value, now: SystemTime) -> Option<AffinityStick> {
    if value.is_null() {
        return None;
    }
    let age = Duration::from_millis(value["age_ms"].as_u64().unwrap());
    Some(stick_at(
        value["who"].as_str().unwrap(),
        now.checked_sub(age).unwrap(),
        value["cache_read"].as_u64().unwrap(),
    ))
    .map(|mut stick| {
        stick.resting = value["resting"].as_bool().unwrap();
        stick
    })
}

fn stick_at(who: &str, at: SystemTime, cache_read: u64) -> AffinityStick {
    AffinityStick {
        who: who.to_string(),
        at,
        cache_read,
        resting: false,
    }
}

fn pair() -> Vec<RouteCandidate<'static>> {
    vec![
        RouteCandidate {
            id: "a",
            allowance: None,
        },
        RouteCandidate {
            id: "b",
            allowance: None,
        },
    ]
}

fn strings(value: &Value) -> Vec<String> {
    value
        .as_array()
        .unwrap()
        .iter()
        .map(|id| id.as_str().unwrap().to_string())
        .collect()
}

fn fixture(name: &str) -> Value {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/magpie/affinity")
        .join(name);
    let bytes = fs::read(&path).unwrap_or_else(|error| panic!("read {}: {error}", path.display()));
    serde_json::from_slice(&bytes).unwrap()
}
