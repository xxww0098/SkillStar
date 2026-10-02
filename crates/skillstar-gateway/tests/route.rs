use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use serde_json::{Value, json};
use skillstar_gateway::{
    AllowanceSnapshot, RouteCandidate, RouteMode, RouteOwner, outbound_log, route_mode,
    route_smart, stored_route_mode,
};

#[test]
fn route_smart_puts_used_share_behind() {
    let fixture = fixture("smart-one.json");
    let order = ordered(&fixture);
    assert_eq!(order, expect(&fixture), "smart order");

    let share = fixture["used_share"].as_f64().unwrap();
    let pos = |id: &str| order.iter().position(|item| item == id).unwrap();
    for candidate in fixture["candidates"].as_array().unwrap() {
        let id = candidate["id"].as_str().unwrap();
        let Some(used) = candidate.get("used").and_then(Value::as_f64) else {
            continue;
        };
        for other in fixture["candidates"].as_array().unwrap() {
            let other_id = other["id"].as_str().unwrap();
            let Some(other_used) = other.get("used").and_then(Value::as_f64) else {
                continue;
            };
            if used >= share && other_used < share {
                assert!(
                    pos(other_id) < pos(id),
                    "{other_id} at {other_used} should stay ahead of {id} at {used}"
                );
            }
        }
    }
}

#[test]
fn route_smart_unknown_snapshot_is_not_exhausted() {
    let fixture = fixture("smart-one.json");
    let order = ordered(&fixture);
    let share = fixture["used_share"].as_f64().unwrap();
    let pos = |id: &str| order.iter().position(|item| item == id).unwrap();
    let unknown = pos("unknown");
    for candidate in fixture["candidates"].as_array().unwrap() {
        let id = candidate["id"].as_str().unwrap();
        if id == "unknown" {
            assert!(candidate.get("used").is_none());
            continue;
        }
        let used = candidate["used"].as_f64().unwrap();
        if used < share {
            assert!(
                pos(id) < unknown,
                "{id} still has room, so it leads unknown"
            );
        } else {
            assert!(
                unknown < pos(id),
                "unknown is not used up, so it leads {id}"
            );
        }
    }
}

#[test]
fn route_mode_empty_is_smart() {
    let fixture = fixture("empty-smart.json");
    assert_eq!(fixture["mode"].as_str().unwrap(), "");
    let (order, next) = decide(&fixture);
    assert_eq!(order, expect(&fixture), "empty string uses smart");
    assert_eq!(next, fixture["next_turn"].as_u64().unwrap());
    assert_eq!(order, route_smart(&candidates_of(&fixture)));
}

#[test]
fn route_mode_order_is_listed_order() {
    let fixture = fixture("order-listed.json");
    assert_eq!(fixture["mode"].as_str().unwrap(), "order");
    let (order, next) = decide(&fixture);
    assert_eq!(order, expect(&fixture), "order keeps the listed ids");
    assert_eq!(next, fixture["next_turn"].as_u64().unwrap());
    assert_ne!(order, route_smart(&candidates_of(&fixture)));
}

#[test]
fn route_mode_rotate_advances() {
    let fixture = fixture("rotate-four.json");
    let candidates = candidates_of(&fixture);
    let mut turn = 0;
    for step in fixture["steps"].as_array().unwrap() {
        assert_eq!(turn, step["turn"].as_u64().unwrap());
        let (order, next) = route_mode(RouteMode::Rotate, &candidates, turn);
        assert_eq!(order, expect(step), "rotate at {turn}");
        assert_eq!(next, step["next_turn"].as_u64().unwrap());
        turn = next;
    }
    let (again, next) = route_mode(RouteMode::Rotate, &candidates, 0);
    assert_eq!(again, expect(&fixture["steps"][0]));
    assert_eq!(next, 1, "the same turn does not move a hidden counter");

    let one = RouteCandidate {
        id: fixture["one"]["id"].as_str().unwrap(),
        allowance: None,
    };
    let stuck = fixture["one"]["turn"].as_u64().unwrap();
    let (order, next) = route_mode(RouteMode::Rotate, &[one], stuck);
    assert_eq!(order, vec![one.id.to_string()]);
    assert_eq!(next, stuck, "one candidate does not advance the turn");
    let (order, next) = route_mode(RouteMode::Rotate, &[], stuck);
    assert!(order.is_empty());
    assert_eq!(next, stuck);
}

#[test]
fn route_mode_usage_reads_snapshot_only() {
    let fixture = fixture("usage-snapshot.json");
    assert_eq!(fixture["mode"].as_str().unwrap(), "usage");
    let before = outbound_log();
    let (order, next) = decide(&fixture);
    assert_eq!(
        outbound_log(),
        before,
        "usage does not record an outbound call"
    );
    assert_eq!(
        order,
        expect(&fixture),
        "usage follows the injected snapshot"
    );
    assert_eq!(next, fixture["next_turn"].as_u64().unwrap());
}

#[test]
fn route_mode_file_default_is_smart() {
    let root = scratch("route-mode-file");
    let _env = EnvRestore::sandbox(&root);
    let path = root.join("data").join("config").join("model_gateway.json");

    let provider = "route-mode-probe";
    let group = "route-mode-group";
    assert_eq!(
        stored_route_mode(RouteOwner::Provider, provider),
        RouteMode::Smart
    );
    assert_eq!(
        stored_route_mode(RouteOwner::Group, group),
        RouteMode::Smart
    );
    assert!(!gateway_file_under(&root), "a missing file is not created");

    write_gateway(
        &path,
        &json!({
            "providers": [{"id": provider}],
            "groups": [{"id": group, "routing": ""}]
        }),
    );
    assert_eq!(
        stored_route_mode(RouteOwner::Provider, provider),
        RouteMode::Smart
    );
    assert_eq!(
        stored_route_mode(RouteOwner::Group, group),
        RouteMode::Smart
    );

    write_gateway(
        &path,
        &json!({
            "providers": [{"id": provider, "routing": "order"}],
            "groups": [{"id": group, "routing": "rotate"}]
        }),
    );
    assert_eq!(
        stored_route_mode(RouteOwner::Provider, provider),
        RouteMode::Order
    );
    assert_eq!(
        stored_route_mode(RouteOwner::Group, group),
        RouteMode::Rotate
    );
    assert_eq!(
        stored_route_mode(RouteOwner::Provider, group),
        RouteMode::Smart
    );
    assert_eq!(
        stored_route_mode(RouteOwner::Provider, "missing"),
        RouteMode::Smart
    );

    write_gateway(
        &path,
        &json!({"providers": [{"id": provider, "routing": "usage"}]}),
    );
    assert_eq!(
        stored_route_mode(RouteOwner::Provider, provider),
        RouteMode::Usage
    );

    write_gateway(
        &path,
        &json!({"providers": [{"id": provider, "routing": "nope"}]}),
    );
    assert_eq!(
        stored_route_mode(RouteOwner::Provider, provider),
        RouteMode::Smart
    );

    write_gateway(
        &path,
        &json!({
            "providers": [
                {"id": provider, "routing": "order"},
                {"id": provider, "routing": "usage"}
            ]
        }),
    );
    assert_eq!(
        stored_route_mode(RouteOwner::Provider, provider),
        RouteMode::Order
    );

    let broken = b"{";
    fs::write(&path, broken).unwrap();
    assert_eq!(
        stored_route_mode(RouteOwner::Provider, provider),
        RouteMode::Smart
    );
    assert_eq!(fs::read(&path).unwrap(), broken);
}

fn ordered(fixture: &Value) -> Vec<String> {
    route_smart(&candidates_of(fixture))
}

fn decide(fixture: &Value) -> (Vec<String>, u64) {
    let mode = RouteMode::parse(fixture["mode"].as_str().unwrap());
    let turn = fixture["turn"].as_u64().unwrap();
    route_mode(mode, &candidates_of(fixture), turn)
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

fn expect(fixture: &Value) -> Vec<String> {
    fixture["expect"]
        .as_array()
        .unwrap()
        .iter()
        .map(|id| id.as_str().unwrap().to_string())
        .collect()
}

fn fixture(name: &str) -> Value {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/magpie/route")
        .join(name);
    let bytes = fs::read(&path).unwrap_or_else(|error| panic!("read {}: {error}", path.display()));
    serde_json::from_slice(&bytes).unwrap()
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

fn write_gateway(path: &Path, value: &Value) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, serde_json::to_vec_pretty(value).unwrap()).unwrap();
}

fn gateway_file_under(root: &Path) -> bool {
    let Ok(entries) = fs::read_dir(root) else {
        return false;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path
            .file_name()
            .is_some_and(|name| name == "model_gateway.json")
        {
            return true;
        }
        if path.is_dir() && gateway_file_under(&path) {
            return true;
        }
    }
    false
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
