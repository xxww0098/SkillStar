use std::fs;
use std::path::PathBuf;

use serde_json::Value;
use skillstar_gateway::{AllowanceSnapshot, RouteCandidate, route_smart};

#[test]
fn route_smart_puts_used_share_behind() {
    let fixture = fixture();
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
    let fixture = fixture();
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

fn ordered(fixture: &Value) -> Vec<String> {
    let candidates: Vec<RouteCandidate<'_>> = fixture["candidates"]
        .as_array()
        .unwrap()
        .iter()
        .map(|candidate| RouteCandidate {
            id: candidate["id"].as_str().unwrap(),
            allowance: candidate
                .get("used")
                .and_then(Value::as_f64)
                .map(|used| AllowanceSnapshot { used }),
        })
        .collect();
    route_smart(&candidates)
}

fn expect(fixture: &Value) -> Vec<String> {
    fixture["expect"]
        .as_array()
        .unwrap()
        .iter()
        .map(|id| id.as_str().unwrap().to_string())
        .collect()
}

fn fixture() -> Value {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/magpie/route/smart-one.json");
    let bytes = fs::read(&path).unwrap_or_else(|error| panic!("read {}: {error}", path.display()));
    serde_json::from_slice(&bytes).unwrap()
}
