use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use serde_json::Value;
use skillstar_gateway::{
    ClassifyCall, ClassifyReply, ClassifyTurn, GroupRule, RuleRequest, order_with_classifier,
    stored_classifier,
};

fn epoch() -> SystemTime {
    SystemTime::UNIX_EPOCH + Duration::from_secs(1_700_000_000)
}

fn answer(intent: &str, confidence: f64) -> ClassifyReply {
    ClassifyReply::Bytes(
        serde_json::to_vec(&serde_json::json!({
            "intent": intent,
            "confidence": confidence,
        }))
        .unwrap(),
    )
}

fn rules_of(fixture: &Value) -> Vec<GroupRule> {
    fixture["rules"]
        .as_array()
        .unwrap()
        .iter()
        .map(|rule| serde_json::from_value(rule.clone()).unwrap())
        .collect()
}

fn members_of(fixture: &Value) -> Vec<String> {
    fixture["members"]
        .as_array()
        .unwrap()
        .iter()
        .map(|item| item.as_str().unwrap().to_string())
        .collect()
}

fn body_of(call: &ClassifyCall) -> Value {
    serde_json::from_slice(&call.body).unwrap()
}

#[test]
fn classify_accepts_at_0_4() {
    let root = scratch("classify-accept");
    let _env = EnvRestore::sandbox(&root);
    let fixture = load_fixture("classify/ask.json");
    let path = gateway_path(&root);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    let doc = serde_json::json!({ "groups": [fixture["group"]] });
    fs::write(&path, serde_json::to_vec_pretty(&doc).unwrap()).unwrap();
    let model = stored_classifier("group/pick");
    assert_eq!(model, "c/cls");
    assert_eq!(stored_classifier("missing"), "");

    let members = members_of(&fixture);
    let rules = rules_of(&fixture);
    let mut asked = 0;
    let order = order_with_classifier(
        &members,
        &rules,
        &RuleRequest::default(),
        &model,
        &ClassifyTurn {
            id: "accept",
            opening: true,
            message: fixture["message"].as_str().unwrap(),
            at: epoch(),
        },
        |call| {
            asked += 1;
            let body = body_of(call);
            assert_eq!(body["model"], "c/cls");
            assert_eq!(body["intents"], fixture["intents"]);
            answer("writing tests", 0.4)
        },
    );
    assert_eq!(asked, 1);
    assert_eq!(order, vec!["vision/m".to_string(), "text/m".to_string()]);
}

#[test]
fn classify_rejects_below_0_4() {
    let fixture = load_fixture("classify/ask.json");
    let members = members_of(&fixture);
    let mut asked = 0;
    let order = order_with_classifier(
        &members,
        &rules_of(&fixture),
        &RuleRequest::default(),
        "c/low",
        &ClassifyTurn {
            id: "reject",
            opening: true,
            message: "a different sentence",
            at: epoch(),
        },
        |_| {
            asked += 1;
            answer("writing tests", 0.39)
        },
    );
    assert_eq!(asked, 1);
    assert_eq!(order, members);
}

#[test]
fn classify_timeout_8s_skips_intent() {
    let fixture = load_fixture("classify/ask.json");
    let members = members_of(&fixture);
    let mut asked = 0;
    let order = order_with_classifier(
        &members,
        &rules_of(&fixture),
        &RuleRequest::default(),
        "c/slow",
        &ClassifyTurn {
            id: "timeout",
            opening: true,
            message: "rename this package",
            at: epoch(),
        },
        |call| {
            asked += 1;
            assert_eq!(call.timeout, Duration::from_secs(8));
            ClassifyReply::TimedOut
        },
    );
    assert_eq!(asked, 1);
    assert_eq!(order, members);
}

#[test]
fn classify_once_per_turn() {
    let fixture = load_fixture("classify/ask.json");
    let members = members_of(&fixture);
    let rules = rules_of(&fixture);
    let mut asked = 0;
    let opening = ClassifyTurn {
        id: "once",
        opening: true,
        message: fixture["message"].as_str().unwrap(),
        at: epoch(),
    };
    let first = order_with_classifier(
        &members,
        &rules,
        &RuleRequest::default(),
        "c/once",
        &opening,
        |_| {
            asked += 1;
            answer("writing tests", 0.4)
        },
    );
    let mid = ClassifyTurn {
        id: "once",
        opening: false,
        message: "tool result, not the user's words",
        at: epoch() + Duration::from_secs(1),
    };
    let second = order_with_classifier(
        &members,
        &rules,
        &RuleRequest::default(),
        "c/once",
        &mid,
        |_| {
            asked += 1;
            answer("writing tests", 0.4)
        },
    );
    assert_eq!(asked, 1);
    let moved = vec!["vision/m".to_string(), "text/m".to_string()];
    assert_eq!(first, moved);
    assert_eq!(second, moved);
}

#[test]
fn classify_cache_10m_and_rest_30s() {
    let fixture = load_fixture("classify/ask.json");
    let members = members_of(&fixture);
    let rules = rules_of(&fixture);
    let start = epoch();
    let mut asked = 0;
    let moved = vec!["vision/m".to_string(), "text/m".to_string()];

    let cached = |id: &str, message: &str, at: SystemTime, asked: &mut usize| {
        order_with_classifier(
            &members,
            &rules,
            &RuleRequest::default(),
            "c/cache",
            &ClassifyTurn {
                id,
                opening: true,
                message,
                at,
            },
            |_| {
                *asked += 1;
                answer("writing tests", 0.4)
            },
        )
    };
    assert_eq!(cached("cache-1", "same words", start, &mut asked), moved);
    assert_eq!(
        cached(
            "cache-2",
            "same words",
            start + Duration::from_secs(10 * 60) - Duration::from_secs(1),
            &mut asked
        ),
        moved
    );
    assert_eq!(
        asked, 1,
        "the same message is not asked again inside 10 minutes"
    );
    assert_eq!(
        cached(
            "cache-3",
            "other words",
            start + Duration::from_secs(60),
            &mut asked
        ),
        moved
    );
    assert_eq!(asked, 2, "a different message is asked");
    assert_eq!(
        cached(
            "cache-4",
            "same words",
            start + Duration::from_secs(10 * 60),
            &mut asked
        ),
        moved
    );
    assert_eq!(asked, 3, "the kept answer expires at 10 minutes");

    let mut failed = 0;
    let rest = |id: &str, at: SystemTime, reply: ClassifyReply, failed: &mut usize| {
        order_with_classifier(
            &members,
            &rules,
            &RuleRequest::default(),
            "c/rest",
            &ClassifyTurn {
                id,
                opening: true,
                message: "rename this package",
                at,
            },
            |_| {
                *failed += 1;
                reply.clone()
            },
        )
    };
    assert_eq!(
        rest("rest-1", start, ClassifyReply::Failed, &mut failed),
        members
    );
    assert_eq!(
        rest(
            "rest-2",
            start + Duration::from_secs(29),
            answer("writing tests", 0.9),
            &mut failed
        ),
        members
    );
    assert_eq!(failed, 1, "a failure is not asked again inside 30 seconds");
    assert_eq!(
        rest(
            "rest-3",
            start + Duration::from_secs(30),
            answer("writing tests", 0.9),
            &mut failed
        ),
        moved
    );
    assert_eq!(failed, 2);
}

#[test]
fn classify_garbage_skips_intent() {
    let fixture = load_fixture("classify/ask.json");
    let members = members_of(&fixture);
    let rules = rules_of(&fixture);
    let mut asked = 0;
    let first = order_with_classifier(
        &members,
        &rules,
        &RuleRequest::default(),
        "c/odd",
        &ClassifyTurn {
            id: "odd-1",
            opening: true,
            message: "not a number",
            at: epoch(),
        },
        |_| {
            asked += 1;
            ClassifyReply::Bytes(b"maybe".to_vec())
        },
    );
    let second = order_with_classifier(
        &members,
        &rules,
        &RuleRequest::default(),
        "c/odd",
        &ClassifyTurn {
            id: "odd-2",
            opening: true,
            message: "not a number",
            at: epoch() + Duration::from_secs(1),
        },
        |_| {
            asked += 1;
            answer("writing tests", 0.9)
        },
    );
    assert_eq!(first, members);
    assert_eq!(
        asked, 2,
        "an answer that is not a verdict does not rest the model"
    );
    assert_eq!(second, vec!["vision/m".to_string(), "text/m".to_string()]);
}

#[test]
fn classify_user_agent() {
    let fixture = load_fixture("classify/ask.json");
    let model = fixture["groupModel"].as_str().unwrap();
    let mut seen = false;
    let order = order_with_classifier(
        &members_of(&fixture),
        &rules_of(&fixture),
        &RuleRequest::default(),
        model,
        &ClassifyTurn {
            id: "agent",
            opening: true,
            message: fixture["message"].as_str().unwrap(),
            at: epoch(),
        },
        |call| {
            seen = true;
            assert_eq!(call.user_agent, "skillstar-router/1");
            let body = body_of(call);
            assert_eq!(body["model"], model);
            assert_eq!(body["intents"], fixture["intents"]);
            answer("writing tests", 0.8)
        },
    );
    assert!(seen);
    assert_eq!(order, vec!["vision/m".to_string(), "text/m".to_string()]);
}

#[test]
fn gateway_manifest_has_no_decision_dep() {
    let manifest =
        fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("Cargo.toml")).unwrap();
    assert!(
        !manifest.contains("skillstar-decision"),
        "the gateway must not depend on the decision crate"
    );
    let skillstar_deps: Vec<_> = manifest
        .lines()
        .filter(|line| line.contains("skillstar-") && !line.contains("skillstar-gateway"))
        .collect();
    assert_eq!(
        skillstar_deps,
        ["skillstar-core = { version = \"0.0.0\", path = \"../skillstar-core\" }"]
    );
}

fn load_fixture(name: &str) -> Value {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/magpie")
        .join(name);
    serde_json::from_slice(&fs::read(path).unwrap()).unwrap()
}

fn gateway_path(root: &Path) -> PathBuf {
    root.join("data").join("config").join("model_gateway.json")
}

fn scratch(label: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let path =
        std::env::temp_dir().join(format!("skillstar-{label}-{}-{nanos}", std::process::id()));
    fs::create_dir_all(&path).unwrap();
    path
}

struct EnvRestore {
    saved: Vec<(&'static str, Option<OsString>)>,
    root: PathBuf,
}

impl EnvRestore {
    fn sandbox(root: &Path) -> Self {
        let home = root.join("home");
        let data = root.join("data");
        fs::create_dir_all(&home).unwrap();
        fs::create_dir_all(&data).unwrap();
        let pairs = [
            ("HOME", home.as_path()),
            ("USERPROFILE", home.as_path()),
            ("SKILLSTAR_TOOL_SYNC_HOME", home.as_path()),
            ("SKILLSTAR_DATA_DIR", data.as_path()),
        ];
        let saved = pairs
            .into_iter()
            .map(|(key, value)| {
                let previous = std::env::var_os(key);
                unsafe { std::env::set_var(key, value) };
                (key, previous)
            })
            .collect();
        Self {
            saved,
            root: root.to_path_buf(),
        }
    }
}

impl Drop for EnvRestore {
    fn drop(&mut self) {
        for (key, previous) in self.saved.drain(..) {
            unsafe {
                match previous {
                    Some(value) => std::env::set_var(key, value),
                    None => std::env::remove_var(key),
                }
            }
        }
        let _ = fs::remove_dir_all(&self.root);
    }
}
