//! Cindy takes a provider only through an import link. Its database is read, never written.

use std::path::Path;

use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use rusqlite::{Connection, OpenFlags};
use serde::Serialize;

const SCHEME: &str = "cindy://provider/import?v=1&data=";

#[derive(Serialize)]
struct LinkBody<'a> {
    kind: &'static str,
    name: &'static str,
    id: &'static str,
    auth: Auth<'a>,
    endpoints: [Endpoint<'a>; 3],
}

#[derive(Serialize)]
struct Auth<'a> {
    method: &'static str,
    #[serde(rename = "apiKey")]
    api_key: &'a str,
}

#[derive(Serialize)]
struct Endpoint<'a> {
    protocol: &'static str,
    #[serde(rename = "baseUrl")]
    base_url: &'a str,
    targets: [&'static str; 1],
    #[serde(rename = "modelsUrl")]
    models_url: &'a str,
}

/// Import link for Cindy. `origin` is the gateway root, without `/v1`.
pub fn cindy_link(origin: &str) -> String {
    let v1 = format!("{origin}/v1");
    let models = format!("{v1}/models");
    let key = super::token_for("cindy");
    let body = LinkBody {
        kind: "custom",
        name: "skillstar",
        id: "skillstar",
        auth: Auth {
            method: "apiKey",
            api_key: &key,
        },
        endpoints: [
            Endpoint {
                protocol: "anthropic-messages",
                base_url: origin,
                targets: ["claude-code"],
                models_url: &models,
            },
            Endpoint {
                protocol: "openai-responses",
                base_url: &v1,
                targets: ["codex"],
                models_url: &models,
            },
            Endpoint {
                protocol: "openai-chat",
                base_url: &v1,
                targets: ["pi"],
                models_url: &models,
            },
        ],
    };
    let bytes = serde_json::to_vec(&body).unwrap_or_default();
    format!("{SCHEME}{}", URL_SAFE_NO_PAD.encode(bytes))
}

/// Whether Cindy's database already has the skillstar provider. Missing files
/// and unreadable databases count as not imported. The file is opened read-only.
pub fn cindy_imported(path: &Path, origin: &str) -> bool {
    if origin.is_empty() || !path.is_file() {
        return false;
    }
    let Ok(conn) = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY) else {
        return false;
    };
    let count: i64 = conn
        .query_row(
            "SELECT count(*) FROM custom_providers
             WHERE id = ?1 OR lower(name) = ?1 OR instr(runtimes, ?2) > 0",
            rusqlite::params!["skillstar", origin],
            |row| row.get(0),
        )
        .unwrap_or(0);
    count > 0
}
