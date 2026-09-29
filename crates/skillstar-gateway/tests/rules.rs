use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use serde_json::Value;
use skillstar_gateway::{
    Caller, ClassifyReply, ClassifyTurn, GroupRule, RouteMode, RouteOwner, RuleRequest,
    expand_group, order_with_classifier, order_with_rules, outbound_log, request_agent, save_group,
    stored_route_mode, stored_rules,
};

#[test]
fn rules_first_match_wins() {
    let root = scratch("rules-first");
    let _env = EnvRestore::sandbox(&root);
    let path = gateway_path(&root);
    let outer = fs::read(fixture_path("group/outer.json")).unwrap();
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(&path, &outer).unwrap();

    let expanded = expand_group("group/outer", &[]);
    let busy = RuleRequest {
        tokens: 9_000,
        images: true,
        thinking: true,
        effort: "max".to_string(),
        agent: "codex".to_string(),
        intent: "writing tests".to_string(),
    };
    assert_eq!(
        order_with_rules(&expanded, &stored_rules("outer"), &busy),
        expanded,
        "no rules leave the expanded order alone"
    );

    let fixture = load_fixture("rules/first.json");
    fs::write(&path, serde_json::to_vec_pretty(&fixture).unwrap()).unwrap();
    let rules = stored_rules("pick");
    assert_eq!(rules.len(), 3);
    let members = strings(&fixture["groups"][0]["members"]);
    for case in fixture["cases"].as_array().unwrap() {
        assert_eq!(
            order_with_rules(&members, &rules, &request_from(case)),
            strings(&case["order"]),
            "tokens {case}"
        );
    }

    save_group("pick", &["img", "long", "longer", "extra"]).unwrap();
    assert_eq!(
        stored_rules("pick"),
        rules,
        "saving members keeps the rules"
    );
    assert_eq!(
        stored_route_mode(RouteOwner::Group, "pick"),
        RouteMode::Rotate
    );
    assert_eq!(
        stored_route_mode(RouteOwner::Provider, "keep"),
        RouteMode::Order
    );
    assert!(!root.join("data").join("model_providers.json").exists());
}

#[test]
fn rules_images_tokens_effort_agent() {
    let matched = load_fixture("rules/match.json");
    let members = strings(&matched["members"]);
    for case in matched["cases"].as_array().unwrap() {
        let rule: GroupRule = serde_json::from_value(case["rule"].clone()).unwrap();
        let order = order_with_rules(&members, &[rule], &request_from(&case["request"]));
        let moved = order.first().map(String::as_str) == Some("vision/m");
        assert_eq!(moved, case["match"].as_bool().unwrap(), "{case}");
    }

    let agents = load_fixture("rules/agent.json");
    for case in agents["cases"].as_array().unwrap() {
        let caller = Caller {
            authorization: case["authorization"].as_str().unwrap_or(""),
            api_key: case["api_key"].as_str().unwrap_or(""),
            goog_key: case["goog_key"].as_str().unwrap_or(""),
            query_key: case["query_key"].as_str().unwrap_or(""),
            user_agent: case["user_agent"].as_str().unwrap_or(""),
        };
        assert_eq!(
            request_agent(&caller),
            case["agent"].as_str().unwrap(),
            "{case}"
        );
    }
    let codex = request_agent(&Caller {
        authorization: "Bearer skillstar-codex",
        user_agent: "claude-cli/2.1.0",
        ..Caller::default()
    });
    let rule = GroupRule {
        use_member: "vision/m".to_string(),
        agents: vec!["codex".to_string()],
        ..GroupRule::default()
    };
    assert_eq!(
        order_with_rules(
            &["text/m", "vision/m"],
            &[rule],
            &RuleRequest {
                agent: codex,
                ..RuleRequest::default()
            }
        ),
        vec!["vision/m".to_string(), "text/m".to_string()]
    );
}

#[test]
fn rules_with_intent_do_not_match_yet() {
    let fixture = load_fixture("rules/intent.json");
    let before = outbound_log();
    let rules = fixture["rules"]
        .as_array()
        .unwrap()
        .iter()
        .map(|rule| serde_json::from_value(rule.clone()).unwrap())
        .collect::<Vec<GroupRule>>();
    let mut asked = 0;
    let order = order_with_classifier(
        &strings(&fixture["members"]),
        &rules,
        &request_from(&fixture["request"]),
        "",
        &ClassifyTurn {
            id: "classifier-off",
            opening: true,
            message: "writing tests",
            at: SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(1_700_000_000),
        },
        |_| {
            asked += 1;
            ClassifyReply::Failed
        },
    );
    assert_eq!(asked, 0, "a group with no classifier does not ask");
    assert_eq!(order, strings(&fixture["members"]));
    assert_eq!(outbound_log(), before, "an intent rule does not call out");
}

fn request_from(value: &Value) -> RuleRequest {
    RuleRequest {
        tokens: value.get("tokens").and_then(Value::as_u64).unwrap_or(0),
        images: value
            .get("images")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        thinking: value
            .get("thinking")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        effort: value
            .get("effort")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string(),
        agent: value
            .get("agent")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string(),
        intent: value
            .get("intent")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string(),
    }
}

fn strings(value: &Value) -> Vec<String> {
    value
        .as_array()
        .unwrap()
        .iter()
        .map(|item| item.as_str().unwrap().to_string())
        .collect()
}

fn load_fixture(name: &str) -> Value {
    serde_json::from_slice(&fs::read(fixture_path(name)).unwrap()).unwrap()
}

fn fixture_path(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/magpie")
        .join(name)
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
