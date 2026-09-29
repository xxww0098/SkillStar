use std::fs;
use std::path::PathBuf;

use skillstar_gateway::{Protocol, outbound_body, upstream_body};

#[test]
fn translate_chat_passthrough_matches_fixture() {
    assert_fixture("chat-passthrough", Protocol::Chat);
}

#[test]
fn translate_anthropic_tool_call_matches_fixture() {
    assert_fixture("anthropic-tool-call", Protocol::Anthropic);
}

/// Vendor spellings of "this many tokens were cached", copied from magpie's
/// `TestChatUsageCache`. A reply with no cache fields still reports the whole
/// prompt; these cases are what a hardcoded zero would get wrong.
#[test]
fn translate_anthropic_usage_counts_cache() {
    let cases = [
        (
            "openai",
            r#"{"prompt_tokens":1000,"completion_tokens":5,"prompt_tokens_details":{"cached_tokens":800}}"#,
            r#"{"input_tokens":200,"output_tokens":5,"cache_read_input_tokens":800,"cache_creation_input_tokens":0}"#,
        ),
        (
            "deepseek",
            r#"{"prompt_tokens":1000,"completion_tokens":5,"prompt_cache_hit_tokens":900,"prompt_cache_miss_tokens":100}"#,
            r#"{"input_tokens":100,"output_tokens":5,"cache_read_input_tokens":900,"cache_creation_input_tokens":0}"#,
        ),
        (
            "deepseek both",
            r#"{"prompt_tokens":1000,"completion_tokens":5,"prompt_cache_hit_tokens":900,"prompt_tokens_details":{"cached_tokens":900}}"#,
            r#"{"input_tokens":100,"output_tokens":5,"cache_read_input_tokens":900,"cache_creation_input_tokens":0}"#,
        ),
        (
            "moonshot",
            r#"{"prompt_tokens":1000,"completion_tokens":5,"cached_tokens":600}"#,
            r#"{"input_tokens":400,"output_tokens":5,"cache_read_input_tokens":600,"cache_creation_input_tokens":0}"#,
        ),
        (
            "openrouter",
            r#"{"prompt_tokens":1000,"completion_tokens":5,"prompt_tokens_details":{"cached_tokens":0,"cache_write_tokens":700}}"#,
            r#"{"input_tokens":300,"output_tokens":5,"cache_read_input_tokens":0,"cache_creation_input_tokens":700}"#,
        ),
        (
            "claude relay, whole",
            r#"{"prompt_tokens":1000,"completion_tokens":5,"cache_read_input_tokens":800,"cache_creation_input_tokens":100}"#,
            r#"{"input_tokens":100,"output_tokens":5,"cache_read_input_tokens":800,"cache_creation_input_tokens":100}"#,
        ),
        (
            "claude relay, anthropic count",
            r#"{"prompt_tokens":10,"completion_tokens":5,"cache_read_input_tokens":800,"cache_creation_input_tokens":100}"#,
            r#"{"input_tokens":10,"output_tokens":5,"cache_read_input_tokens":800,"cache_creation_input_tokens":100}"#,
        ),
        (
            "none",
            r#"{"prompt_tokens":1000,"completion_tokens":5}"#,
            r#"{"input_tokens":1000,"output_tokens":5,"cache_read_input_tokens":0,"cache_creation_input_tokens":0}"#,
        ),
    ];
    for (name, usage, want) in cases {
        let body = format!(
            r#"{{"id":"c1","model":"m1","choices":[{{"message":{{"role":"assistant","content":"ok"}},"finish_reason":"stop"}}],"usage":{usage}}}"#
        );
        let outbound = outbound_body(Protocol::Anthropic, body.as_bytes())
            .unwrap_or_else(|error| panic!("{name}: {error}"));
        let parsed: serde_json::Value = serde_json::from_slice(&outbound)
            .unwrap_or_else(|error| panic!("{name}: outbound is not json: {error}"));
        let got = parsed
            .get("usage")
            .cloned()
            .unwrap_or_else(|| panic!("{name}: outbound has no usage"));
        let want: serde_json::Value = serde_json::from_str(want).unwrap();
        assert_eq!(got, want, "{name}");
    }
}

#[test]
fn translate_allow_list_normalizes_brand() {
    let left = normalize(b"hello magpie and Magpie", &["brand".to_string()]);
    let right = normalize(b"hello skillstar and Skillstar", &["brand".to_string()]);
    assert_eq!(left, right);
}

#[test]
fn gateway_manifest_depends_only_on_skillstar_core() {
    let manifest = include_str!("../Cargo.toml");
    let mut skillstar = Vec::new();
    let mut in_deps = false;
    for line in manifest.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('[') {
            in_deps = trimmed.starts_with("[dependencies]")
                || trimmed.starts_with("[dev-dependencies]")
                || trimmed.starts_with("[build-dependencies]")
                || (trimmed.starts_with("[target.") && trimmed.contains("dependencies"));
            continue;
        }
        if !in_deps || trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let Some((key, _)) = trimmed.split_once('=') else {
            continue;
        };
        let key = key.trim().trim_matches('"');
        if key.starts_with("skillstar") {
            skillstar.push(key.to_string());
        }
    }
    assert_eq!(
        skillstar,
        vec!["skillstar-core".to_string()],
        "gateway skillstar deps must be exactly skillstar-core"
    );
    assert!(
        manifest.contains("../skillstar-core"),
        "skillstar-core must be a path dependency"
    );
}

fn assert_fixture(group: &str, protocol: Protocol) {
    let inbound = read_body(group, "inbound.json");
    let expected_upstream = read_body(group, "upstream_request.json");
    let upstream_response = read_body(group, "upstream_response.json");
    let expected_outbound = read_body(group, "outbound.json");
    let allow = allow_list(group);

    let upstream = upstream_body(protocol, &inbound).unwrap_or_else(|error| {
        panic!("{group} upstream translation failed: {error}");
    });
    let outbound = outbound_body(protocol, &upstream_response).unwrap_or_else(|error| {
        panic!("{group} outbound translation failed: {error}");
    });

    assert_normalized(
        group,
        "upstream request",
        &expected_upstream,
        &upstream,
        &allow,
    );
    assert_normalized(group, "outbound", &expected_outbound, &outbound, &allow);
}

fn assert_normalized(group: &str, label: &str, expected: &[u8], actual: &[u8], allow: &[String]) {
    let expected = normalize(expected, allow);
    let actual = normalize(actual, allow);
    if expected == actual {
        return;
    }
    panic!(
        "{group} {label} differs after allow-list normalization\n{diff}",
        diff = explain(&expected, &actual)
    );
}

fn normalize(bytes: &[u8], allow: &[String]) -> Vec<u8> {
    let mut text = String::from_utf8(bytes.to_vec()).unwrap_or_else(|error| {
        panic!("fixture body is not utf-8: {error}");
    });
    for rule in allow {
        match rule.as_str() {
            "brand" => {
                text = text
                    .replace("Magpie", "Skillstar")
                    .replace("magpie", "skillstar");
            }
            other => panic!("unknown allow rule {other:?}"),
        }
    }
    text.into_bytes()
}

fn explain(expected: &[u8], actual: &[u8]) -> String {
    let expected_pretty = pretty(expected);
    let actual_pretty = pretty(actual);
    let mut out = String::from("--- expected\n");
    out.push_str(&expected_pretty);
    out.push_str("\n--- actual\n");
    out.push_str(&actual_pretty);
    if expected_pretty == actual_pretty {
        out.push_str("\n--- raw expected\n");
        out.push_str(&String::from_utf8_lossy(expected));
        out.push_str("\n--- raw actual\n");
        out.push_str(&String::from_utf8_lossy(actual));
    }
    out.push_str("\n--- line diff\n");
    let expected_lines: Vec<&str> = expected_pretty.lines().collect();
    let actual_lines: Vec<&str> = actual_pretty.lines().collect();
    let lines = expected_lines.len().max(actual_lines.len());
    for index in 0..lines {
        let left = expected_lines.get(index).copied().unwrap_or("<missing>");
        let right = actual_lines.get(index).copied().unwrap_or("<missing>");
        if left != right {
            out.push_str(&format!(
                "line {}: expected {left}\n         actual   {right}\n",
                index + 1
            ));
        }
    }
    out
}

fn pretty(bytes: &[u8]) -> String {
    match serde_json::from_slice::<serde_json::Value>(bytes) {
        Ok(value) => {
            serde_json::to_string_pretty(&value).unwrap_or_else(|_| bytes_to_string(bytes))
        }
        Err(_) => bytes_to_string(bytes),
    }
}

fn bytes_to_string(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

fn allow_list(group: &str) -> Vec<String> {
    let bytes = read_body(group, "allow.json");
    serde_json::from_slice(&bytes).unwrap_or_else(|error| {
        panic!("{group} allow.json: {error}");
    })
}

fn read_body(group: &str, name: &str) -> Vec<u8> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/magpie/translate")
        .join(group)
        .join(name);
    let mut bytes =
        fs::read(&path).unwrap_or_else(|error| panic!("read {}: {error}", path.display()));
    if bytes.ends_with(b"\n") {
        bytes.pop();
        if bytes.ends_with(b"\r") {
            bytes.pop();
        }
    }
    bytes
}
