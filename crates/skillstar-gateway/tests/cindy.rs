//! Cindy's import link. The database is only read.

use std::fs;
use std::path::Path;
use std::time::SystemTime;

use base64::Engine;
use rusqlite::Connection;
use serde_json::Value;
use skillstar_gateway::{cindy_imported, cindy_link, token_for};

const ORIGIN: &str = "http://127.0.0.1:21847";

fn decode(link: &str) -> Value {
    let data = link
        .strip_prefix("cindy://provider/import?v=1&data=")
        .expect("scheme");
    let bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(data)
        .expect("base64");
    serde_json::from_slice(&bytes).expect("json")
}

fn stamp(path: &Path) -> (Vec<u8>, SystemTime) {
    let meta = fs::metadata(path).unwrap();
    (fs::read(path).unwrap(), meta.modified().unwrap())
}

#[test]
fn cindy_link_roundtrip() {
    let link = cindy_link(ORIGIN);
    let body = decode(&link);
    assert_eq!(body["kind"], "custom");
    assert_eq!(body["id"], "skillstar");
    assert_eq!(body["name"], "skillstar");
    assert_eq!(body["endpoints"][0]["protocol"], "anthropic-messages");
    assert_eq!(body["endpoints"][0]["baseUrl"], ORIGIN);
    assert_eq!(body["endpoints"][0]["targets"][0], "claude-code");
    assert_eq!(body["endpoints"][1]["protocol"], "openai-responses");
    assert_eq!(body["endpoints"][1]["baseUrl"], format!("{ORIGIN}/v1"));
    assert_eq!(body["endpoints"][1]["targets"][0], "codex");
    assert_eq!(body["endpoints"][2]["protocol"], "openai-chat");
    assert_eq!(body["endpoints"][2]["baseUrl"], format!("{ORIGIN}/v1"));
    assert_eq!(body["endpoints"][2]["targets"][0], "pi");
    assert_eq!(body["endpoints"][0]["modelsUrl"], format!("{ORIGIN}/v1/models"));
    let text = link.to_lowercase();
    assert!(!text.contains("magpie"), "{link}");
    assert!(!link.contains("api.anthropic.com"), "{link}");
    assert!(!link.contains("api.openai.com"), "{link}");
    assert!(!link.contains(":3425"), "{link}");
}

#[test]
fn cindy_bearer_is_token_for() {
    let body = decode(&cindy_link(ORIGIN));
    assert_eq!(body["auth"]["method"], "apiKey");
    assert_eq!(body["auth"]["apiKey"], token_for("cindy"));
    assert_eq!(token_for("cindy"), "skillstar-cindy");
}

#[test]
fn cindy_database_is_not_written() {
    let root = std::env::temp_dir().join(format!(
        "skillstar-cindy-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos()
    ));
    fs::create_dir_all(&root).unwrap();
    let path = root.join("cindy-local-v1.db");
    assert!(!cindy_imported(&path, ORIGIN));
    assert!(!path.exists());

    let conn = Connection::open(&path).unwrap();
    conn.execute_batch(
        "CREATE TABLE custom_providers (
            id text PRIMARY KEY,
            name text,
            runtimes text DEFAULT '{}'
        );",
    )
    .unwrap();
    drop(conn);
    let (empty, empty_time) = stamp(&path);
    assert!(!cindy_imported(&path, ORIGIN));
    assert_eq!(stamp(&path), (empty, empty_time));

    let conn = Connection::open(&path).unwrap();
    conn.execute(
        "INSERT INTO custom_providers VALUES (?1, ?2, ?3)",
        ("p1", "Other", r#"{"codex":{"baseUrl":"https://api.openai.com/v1"}}"#),
    )
    .unwrap();
    drop(conn);
    let (other, other_time) = stamp(&path);
    assert!(!cindy_imported(&path, ORIGIN));
    assert_eq!(stamp(&path), (other, other_time));

    let conn = Connection::open(&path).unwrap();
    conn.execute(
        "INSERT INTO custom_providers VALUES (?1, ?2, ?3)",
        (
            "p2",
            "Mine",
            format!(r#"{{"codex":{{"baseUrl":"{ORIGIN}/v1"}}}}"#),
        ),
    )
    .unwrap();
    drop(conn);
    let (hit, hit_time) = stamp(&path);
    assert!(cindy_imported(&path, ORIGIN));
    assert_eq!(stamp(&path), (hit, hit_time));
    let _ = fs::remove_dir_all(&root);
}
