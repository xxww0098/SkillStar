//! Routes on the one loopback handler, other than the Claude MCP callback.
//!
//! Chat and Anthropic still go through `translate`. Responses, images, and
//! Gemini posts are forwarded as the agent sent them. A model id that contains
//! `/` is a local error: this crate has no catalog that can resolve it.

use serde_json::{Value, json};

use crate::translate::Protocol;

const JSON: &str = "application/json";
const TEXT: &str = "text/plain; charset=utf-8";
const VERSION: &str = "dev";
const SERVES: &str = "skillstar serves /v1/chat/completions, /v1/responses, /v1/messages, /v1/images/generations, /v1/images/edits and /v1beta/models/*";
const CHAT_UPSTREAM: &str = "/v1/chat/completions";

const CHAT_POSTS: &[&str] = &["/v1/chat/completions", "/chat/completions"];
const ANTHROPIC_POSTS: &[&str] = &["/v1/messages", "/messages"];
const RAW_POSTS: &[&str] = &[
    "/v1/responses",
    "/responses",
    "/v1/images/generations",
    "/images/generations",
    "/v1/images/edits",
    "/images/edits",
];

pub(crate) struct Local {
    pub status: u16,
    pub content_type: &'static str,
    pub body: Vec<u8>,
}

pub(crate) enum Kind {
    Translated(Protocol),
    Raw,
    CountTokens,
    Gemini,
    Codex,
}

pub(crate) enum Plan {
    Local(Local),
    WithBody(Kind),
}

pub(crate) enum Outcome {
    Local(Local),
    Forward {
        protocol: Option<Protocol>,
        /// Path beginning with `/`, appended to the configured origin.
        url_path: String,
    },
}

/// `HEAD` follows the GET table and the caller clears the body.
pub(crate) fn plan(method: &str, path: &str, upgrade: Option<&str>) -> Plan {
    let method = if method.eq_ignore_ascii_case("HEAD") {
        "GET"
    } else {
        method
    };
    if let Some(rest) = codex_rest(path) {
        return plan_codex(method, rest, upgrade);
    }
    if method == "GET"
        && let Some(local) = get_route(path)
    {
        return Plan::Local(local);
    }
    if method == "POST" {
        if path == "/v1/messages/count_tokens" {
            return Plan::WithBody(Kind::CountTokens);
        }
        if CHAT_POSTS.contains(&path) {
            return Plan::WithBody(Kind::Translated(Protocol::Chat));
        }
        if ANTHROPIC_POSTS.contains(&path) {
            return Plan::WithBody(Kind::Translated(Protocol::Anthropic));
        }
        if RAW_POSTS.contains(&path) {
            return Plan::WithBody(Kind::Raw);
        }
        if path.starts_with("/v1beta/models/") {
            return Plan::WithBody(Kind::Gemini);
        }
    }
    Plan::Local(serves())
}

pub(crate) fn finish(kind: Kind, path: &str, body: &[u8]) -> Outcome {
    match kind {
        Kind::Translated(protocol) => finish_translated(protocol, body),
        Kind::Raw => finish_named(path, body),
        Kind::CountTokens => finish_count(body),
        Kind::Gemini => finish_gemini(path, body),
        Kind::Codex => finish_codex(path, body),
    }
}

fn get_route(path: &str) -> Option<Local> {
    match path {
        "/" => Some(info()),
        "/api/hello" => Some(hello()),
        "/v1/models" | "/models" => Some(model_list()),
        "/v1beta/models" => Some(empty_models()),
        _ => {
            let id = path.strip_prefix("/v1/models/")?;
            Some(error_local(
                Shape::Chat,
                404,
                &format!("unknown model {id}"),
            ))
        }
    }
}

fn plan_codex(method: &str, rest: &str, upgrade: Option<&str>) -> Plan {
    if upgrade.is_some_and(|value| value.trim().eq_ignore_ascii_case("websocket")) {
        return Plan::Local(local_text(426, "skillstar speaks HTTP\n"));
    }
    if method == "GET" && rest == "/models" {
        return Plan::Local(empty_models());
    }
    if method == "POST" && (rest == "/responses" || rest == "/responses/compact") {
        return Plan::WithBody(Kind::Codex);
    }
    Plan::Local(serves())
}

/// `/backend-api/codex/responses` → `/responses`. The exact path without the
/// trailing slash is not this route.
fn codex_rest(path: &str) -> Option<&str> {
    let rest = path.strip_prefix("/backend-api/codex")?;
    if rest.is_empty() {
        return None;
    }
    rest.starts_with('/').then_some(rest)
}

fn finish_translated(protocol: Protocol, body: &[u8]) -> Outcome {
    if let Some(model) = slash_model(body) {
        let shape = match protocol {
            Protocol::Chat => Shape::Chat,
            Protocol::Anthropic => Shape::Anthropic,
        };
        return Outcome::Local(error_local(shape, 404, &unknown_model(&model)));
    }
    Outcome::Forward {
        protocol: Some(protocol),
        url_path: CHAT_UPSTREAM.to_string(),
    }
}

fn finish_named(path: &str, body: &[u8]) -> Outcome {
    if let Some(model) = slash_model(body) {
        return Outcome::Local(error_local(Shape::Chat, 404, &unknown_model(&model)));
    }
    Outcome::Forward {
        protocol: None,
        url_path: path.to_string(),
    }
}

fn finish_codex(path: &str, body: &[u8]) -> Outcome {
    let Some(rest) = codex_rest(path) else {
        return Outcome::Local(serves());
    };
    if let Some(model) = slash_model(body) {
        if rest == "/responses/compact" {
            return Outcome::Local(error_local(
                Shape::Chat,
                400,
                "/responses/compact is not supported for skillstar models; use a compaction_trigger on /responses",
            ));
        }
        return Outcome::Local(error_local(Shape::Chat, 404, &unknown_model(&model)));
    }
    Outcome::Forward {
        protocol: None,
        url_path: format!("/v1{rest}"),
    }
}

fn finish_count(body: &[u8]) -> Outcome {
    if serde_json::from_slice::<Value>(body).is_err() {
        return Outcome::Local(error_local(
            Shape::Anthropic,
            400,
            "invalid request: body must be JSON",
        ));
    }
    Outcome::Local(local_json(200, json!({"input_tokens": body.len() / 4})))
}

fn finish_gemini(path: &str, body: &[u8]) -> Outcome {
    let Some(call) = path.strip_prefix("/v1beta/models/") else {
        return Outcome::Local(serves());
    };
    let Some((model, method)) = call.rsplit_once(':') else {
        return Outcome::Local(error_local(
            Shape::Gemini,
            404,
            "expected /v1beta/models/{model}:generateContent",
        ));
    };
    if model.is_empty() {
        return Outcome::Local(error_local(
            Shape::Gemini,
            400,
            "invalid request: model must be a nonempty string",
        ));
    }
    if method == "countTokens" {
        if serde_json::from_slice::<Value>(body).is_err() {
            return Outcome::Local(error_local(
                Shape::Gemini,
                400,
                "invalid request: body must be JSON",
            ));
        }
        return Outcome::Local(local_json(200, json!({"totalTokens": body.len() / 4})));
    }
    if model.contains('/') {
        return Outcome::Local(error_local(Shape::Gemini, 404, &unknown_model(model)));
    }
    match method {
        "generateContent" | "streamGenerateContent" => Outcome::Forward {
            protocol: None,
            url_path: path.to_string(),
        },
        _ => Outcome::Local(error_local(
            Shape::Gemini,
            404,
            &format!("unknown method {method}"),
        )),
    }
}

fn slash_model(body: &[u8]) -> Option<String> {
    #[derive(serde::Deserialize)]
    struct Envelope {
        model: Option<String>,
    }
    let model = serde_json::from_slice::<Envelope>(body).ok()?.model?;
    model.contains('/').then_some(model)
}

fn unknown_model(model: &str) -> String {
    format!("skillstar knows no model {model:?}; add a provider in skillstar first")
}

fn info() -> Local {
    local_json(
        200,
        json!({
            "name": "skillstar",
            "version": VERSION,
            "models": 0,
            "apis": [
                "/v1/chat/completions",
                "/v1/responses",
                "/v1/messages",
                "/v1beta/models/{model}:generateContent",
                "/v1/images/generations",
                "/v1/images/edits",
            ],
        }),
    )
}

fn hello() -> Local {
    local_json(200, json!({"name": "skillstar", "version": VERSION}))
}

fn model_list() -> Local {
    local_json(
        200,
        json!({"object": "list", "data": [], "has_more": false}),
    )
}

fn empty_models() -> Local {
    local_json(200, json!({"models": []}))
}

fn serves() -> Local {
    error_local(Shape::Chat, 404, SERVES)
}

enum Shape {
    Chat,
    Anthropic,
    Gemini,
}

fn error_local(shape: Shape, status: u16, message: &str) -> Local {
    let typ = match status {
        400 => "invalid_request_error",
        401 => "authentication_error",
        403 => "permission_error",
        404 => "not_found_error",
        429 => "rate_limit_error",
        529 => "overloaded_error",
        _ => "api_error",
    };
    let value = match shape {
        Shape::Anthropic => json!({"type": "error", "error": {"type": typ, "message": message}}),
        Shape::Gemini => {
            let label = match status {
                400 => "INVALID_ARGUMENT",
                401 => "UNAUTHENTICATED",
                403 => "PERMISSION_DENIED",
                404 => "NOT_FOUND",
                429 => "RESOURCE_EXHAUSTED",
                500 => "INTERNAL",
                502 | 503 | 529 => "UNAVAILABLE",
                _ => "UNKNOWN",
            };
            json!({"error": {"code": status, "message": message, "status": label}})
        }
        Shape::Chat => {
            json!({"error": {"message": message, "type": typ, "code": null, "param": null}})
        }
    };
    local_json(status, value)
}

fn local_json(status: u16, value: Value) -> Local {
    Local {
        status,
        content_type: JSON,
        body: serde_json::to_vec(&value).expect("json value"),
    }
}

fn local_text(status: u16, body: &str) -> Local {
    Local {
        status,
        content_type: TEXT,
        body: body.as_bytes().to_vec(),
    }
}
