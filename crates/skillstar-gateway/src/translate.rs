//! Turn an agent body into the body a Chat Completions upstream receives, and
//! turn that upstream's JSON reply back into the agent's protocol.
//!
//! Chat is copied through. Anthropic Messages is rebuilt. A translated Chat
//! request is always streamed: that is the body the reference gateway sends
//! after it has parsed the agent request, including `stream_options`.

use serde::Deserialize;
use serde_json::{Map, Value, value::RawValue};

/// Wire protocol of the agent that sent the body.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Protocol {
    Chat,
    Anthropic,
}

/// The inbound body could not be translated.
#[derive(Debug)]
pub struct TranslateError {
    message: String,
}

impl TranslateError {
    fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl std::fmt::Display for TranslateError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for TranslateError {}

/// Body to send to a Chat Completions upstream for this agent request.
///
/// Chat is returned unchanged. Anthropic is rendered as Chat Completions.
pub fn upstream_body(protocol: Protocol, inbound: &[u8]) -> Result<Vec<u8>, TranslateError> {
    match protocol {
        Protocol::Chat => Ok(inbound.to_vec()),
        Protocol::Anthropic => chat_request_from_anthropic(inbound),
    }
}

/// Body to return to the agent, given the upstream's Chat Completions JSON.
///
/// Chat is returned unchanged. Anthropic is rendered as a Messages reply.
pub fn outbound_body(
    protocol: Protocol,
    upstream_response: &[u8],
) -> Result<Vec<u8>, TranslateError> {
    match protocol {
        Protocol::Chat => Ok(upstream_response.to_vec()),
        Protocol::Anthropic => anthropic_message_from_chat(upstream_response),
    }
}

#[derive(Deserialize)]
struct AnthropicRequest {
    model: Option<String>,
    system: Option<String>,
    max_tokens: Option<i64>,
    messages: Option<Vec<InMessage>>,
    tools: Option<Vec<InTool>>,
    thinking: Option<Thinking>,
}

#[derive(Deserialize)]
struct InMessage {
    role: Option<String>,
    /// Kept raw so a block's `input` object can be copied with its original
    /// key order. An untagged enum would re-parse through `Value` and sort keys.
    content: Box<RawValue>,
}

enum MessageContent {
    Text(String),
    Blocks(Vec<Block>),
}

#[derive(Deserialize)]
struct Block {
    #[serde(rename = "type")]
    kind: Option<String>,
    text: Option<String>,
    id: Option<String>,
    name: Option<String>,
    input: Option<Box<RawValue>>,
    tool_use_id: Option<String>,
    content: Option<String>,
}

#[derive(Deserialize)]
struct InTool {
    name: Option<String>,
    description: Option<String>,
    input_schema: Option<Box<RawValue>>,
}

#[derive(Deserialize)]
struct Thinking {
    #[serde(rename = "type")]
    kind: Option<String>,
    budget_tokens: Option<i64>,
}

fn chat_request_from_anthropic(body: &[u8]) -> Result<Vec<u8>, TranslateError> {
    let request: AnthropicRequest = serde_json::from_slice(body)
        .map_err(|error| TranslateError::new(format!("invalid request: {error}")))?;
    let mut messages = Vec::new();
    if let Some(system) = request.system.filter(|system| !system.is_empty()) {
        messages.push(sorted_object([
            ("content", json_string(&system)),
            ("role", json_string("system")),
        ]));
    }
    for message in request.messages.unwrap_or_default() {
        let content = message_content(&message.content)?;
        if message.role.as_deref() == Some("assistant") {
            messages.push(assistant_message(&content));
        } else {
            push_user_content(&mut messages, &content);
        }
    }

    let mut pairs = vec![
        ("messages", array(&messages)),
        ("model", json_string(request.model.as_deref().unwrap_or(""))),
        ("stream", "true".to_string()),
        (
            "stream_options",
            sorted_object([("include_usage", "true".to_string())]),
        ),
    ];
    if let Some(max_tokens) = request.max_tokens.filter(|max_tokens| *max_tokens > 0) {
        pairs.push(("max_tokens", max_tokens.to_string()));
    }
    if let Some(effort) = thinking_effort(request.thinking.as_ref()) {
        pairs.push(("reasoning_effort", json_string(effort)));
    }
    if let Some(tools) = request.tools.filter(|tools| !tools.is_empty()) {
        let rendered: Vec<String> = tools.iter().map(chat_tool).collect();
        pairs.push(("tools", array(&rendered)));
    }
    Ok(sorted_object(pairs).into_bytes())
}

fn message_content(raw: &RawValue) -> Result<MessageContent, TranslateError> {
    let text = raw.get();
    if text.starts_with('"') {
        let text: String = serde_json::from_str(text)
            .map_err(|error| TranslateError::new(format!("invalid message content: {error}")))?;
        return Ok(MessageContent::Text(text));
    }
    let blocks: Vec<Block> = serde_json::from_str(text)
        .map_err(|error| TranslateError::new(format!("invalid message content: {error}")))?;
    Ok(MessageContent::Blocks(blocks))
}

fn assistant_message(content: &MessageContent) -> String {
    let mut text = String::new();
    let mut calls = Vec::new();
    match content {
        MessageContent::Text(value) => text.push_str(value),
        MessageContent::Blocks(blocks) => {
            for block in blocks {
                match block.kind.as_deref() {
                    Some("text") => text.push_str(block.text.as_deref().unwrap_or("")),
                    Some("tool_use") => calls.push(chat_tool_call(block)),
                    _ => {}
                }
            }
        }
    }
    let mut pairs = vec![("role", json_string("assistant"))];
    if !text.is_empty() || calls.is_empty() {
        pairs.push(("content", json_string(&text)));
    }
    if !calls.is_empty() {
        pairs.push(("tool_calls", array(&calls)));
    }
    sorted_object(pairs)
}

fn chat_tool_call(block: &Block) -> String {
    let arguments = block.input.as_deref().map(RawValue::get).unwrap_or("{}");
    sorted_object([
        (
            "function",
            sorted_object([
                ("arguments", json_string(arguments)),
                ("name", json_string(block.name.as_deref().unwrap_or(""))),
            ]),
        ),
        ("id", json_string(block.id.as_deref().unwrap_or(""))),
        ("type", json_string("function")),
    ])
}

fn push_user_content(out: &mut Vec<String>, content: &MessageContent) {
    match content {
        MessageContent::Text(text) => {
            if !text.is_empty() {
                out.push(user_text(text));
            }
        }
        MessageContent::Blocks(blocks) => {
            let mut text = String::new();
            for block in blocks {
                match block.kind.as_deref() {
                    Some("tool_result") => {
                        flush_user_text(out, &mut text);
                        out.push(tool_message(block));
                    }
                    Some("text") => text.push_str(block.text.as_deref().unwrap_or("")),
                    _ => {}
                }
            }
            flush_user_text(out, &mut text);
        }
    }
}

fn flush_user_text(out: &mut Vec<String>, text: &mut String) {
    if text.is_empty() {
        return;
    }
    out.push(user_text(text));
    text.clear();
}

fn user_text(text: &str) -> String {
    sorted_object([
        ("content", json_string(text)),
        ("role", json_string("user")),
    ])
}

fn tool_message(block: &Block) -> String {
    sorted_object([
        (
            "content",
            json_string(block.content.as_deref().unwrap_or("")),
        ),
        ("role", json_string("tool")),
        (
            "tool_call_id",
            json_string(block.tool_use_id.as_deref().unwrap_or("")),
        ),
    ])
}

fn chat_tool(tool: &InTool) -> String {
    let mut function = vec![
        (
            "description",
            json_string(tool.description.as_deref().unwrap_or("")),
        ),
        ("name", json_string(tool.name.as_deref().unwrap_or(""))),
    ];
    if let Some(schema) = &tool.input_schema {
        function.push(("parameters", schema.get().to_string()));
    }
    sorted_object([
        ("function", sorted_object(function)),
        ("type", json_string("function")),
    ])
}

/// Anthropic `thinking.budget_tokens` as Chat's `reasoning_effort`.
/// A budget the agent did not set stays off, so a title request does not
/// turn reasoning on upstream.
fn thinking_effort(thinking: Option<&Thinking>) -> Option<&'static str> {
    let thinking = thinking?;
    let kind = thinking.kind.as_deref().unwrap_or("");
    if kind != "enabled" && kind != "adaptive" {
        return None;
    }
    match thinking.budget_tokens.unwrap_or(0) {
        ..=0 => None,
        1..=4096 => Some("low"),
        4097..=12000 => Some("medium"),
        12001..=24000 => Some("high"),
        _ => Some("xhigh"),
    }
}

fn anthropic_message_from_chat(body: &[u8]) -> Result<Vec<u8>, TranslateError> {
    let response: Map<String, Value> = serde_json::from_slice(body)
        .map_err(|error| TranslateError::new(format!("invalid request: {error}")))?;
    let id = response.get("id").and_then(Value::as_str).unwrap_or("");
    let model = response.get("model").and_then(Value::as_str).unwrap_or("");
    let choice = response
        .get("choices")
        .and_then(Value::as_array)
        .and_then(|choices| choices.first())
        .and_then(Value::as_object);
    let message = choice.and_then(|choice| choice.get("message"));
    let finish = choice
        .and_then(|choice| choice.get("finish_reason"))
        .and_then(Value::as_str)
        .unwrap_or("");

    let mut content = Vec::new();
    let mut has_tool = false;
    if let Some(message) = message {
        if let Some(thought) = message
            .get("reasoning_content")
            .and_then(Value::as_str)
            .filter(|thought| !thought.is_empty())
        {
            content.push(sorted_object([
                ("signature", json_string("")),
                ("thinking", json_string(thought)),
                ("type", json_string("thinking")),
            ]));
        }
        if let Some(text) = message
            .get("content")
            .and_then(Value::as_str)
            .filter(|text| !text.is_empty())
        {
            content.push(sorted_object([
                ("text", json_string(text)),
                ("type", json_string("text")),
            ]));
        }
        if let Some(calls) = message.get("tool_calls").and_then(Value::as_array) {
            for call in calls {
                has_tool = true;
                content.push(anthropic_tool_use(call));
            }
        }
    }

    let rendered = sorted_object([
        ("content", array(&content)),
        ("id", json_string(id)),
        ("model", json_string(model)),
        ("role", json_string("assistant")),
        ("stop_reason", json_string(stop_reason(finish, has_tool))),
        ("stop_sequence", "null".to_string()),
        ("type", json_string("message")),
        ("usage", chat_usage(response.get("usage"))),
    ]);
    Ok(rendered.into_bytes())
}

fn anthropic_tool_use(call: &Value) -> String {
    let id = call.get("id").and_then(Value::as_str).unwrap_or("");
    let name = call
        .pointer("/function/name")
        .and_then(Value::as_str)
        .unwrap_or("");
    let arguments = call
        .pointer("/function/arguments")
        .and_then(Value::as_str)
        .unwrap_or("");
    sorted_object([
        ("id", json_string(id)),
        ("input", parse_args(arguments)),
        ("name", json_string(name)),
        ("type", json_string("tool_use")),
    ])
}

fn parse_args(arguments: &str) -> String {
    let arguments = arguments.trim();
    if arguments.is_empty() {
        return "{}".to_string();
    }
    if arguments.starts_with('{')
        && serde_json::from_str::<Value>(arguments).is_ok_and(|value| value.is_object())
    {
        return arguments.to_string();
    }
    sorted_object([("input", json_string(arguments))])
}

fn stop_reason(finish: &str, has_tool: bool) -> &'static str {
    match finish {
        "length" => "max_tokens",
        "tool_calls" | "function_call" => "tool_use",
        "content_filter" => "refusal",
        "" if has_tool => "tool_use",
        _ => "end_turn",
    }
}

/// Chat Completions usage, in the names each vendor uses, as Anthropic counts it.
///
/// `prompt_tokens` normally includes cached tokens, so those are subtracted.
/// A relay that already speaks Anthropic's names, and whose prompt count is
/// smaller than the cache it reports, is left as Anthropic counted it.
/// Field order is the Anthropic usage struct, not alphabetical.
fn chat_usage(usage: Option<&Value>) -> String {
    let usage = usage.filter(|usage| usage.is_object());
    let prompt = json_i64(usage, "prompt_tokens");
    let completion = json_i64(usage, "completion_tokens");
    let details =
        usage.and_then(|usage| usage.get("prompt_tokens_details").filter(|d| d.is_object()));
    let mut cache_read = json_i64(details, "cached_tokens");
    let mut cache_write = json_i64(details, "cache_write_tokens");
    for key in ["prompt_cache_hit_tokens", "cached_tokens"] {
        if cache_read == 0 {
            cache_read = json_i64(usage, key);
        }
    }
    let mut whole = true;
    if cache_read == 0 && cache_write == 0 {
        cache_read = json_i64(usage, "cache_read_input_tokens");
        cache_write = json_i64(usage, "cache_creation_input_tokens");
        whole = i128::from(prompt) >= i128::from(cache_read) + i128::from(cache_write);
    }
    let input = if whole {
        i128::from(prompt)
            .saturating_sub(i128::from(cache_read))
            .saturating_sub(i128::from(cache_write))
            .max(0) as i64
    } else {
        prompt
    };
    ordered_object(&[
        ("input_tokens", input.to_string()),
        ("output_tokens", completion.to_string()),
        ("cache_read_input_tokens", cache_read.to_string()),
        ("cache_creation_input_tokens", cache_write.to_string()),
    ])
}

fn json_i64(value: Option<&Value>, key: &str) -> i64 {
    value
        .and_then(|value| value.get(key))
        .and_then(Value::as_i64)
        .unwrap_or(0)
}

fn sorted_object<I, K>(pairs: I) -> String
where
    I: IntoIterator<Item = (K, String)>,
    K: AsRef<str>,
{
    let mut pairs: Vec<(String, String)> = pairs
        .into_iter()
        .map(|(key, value)| (key.as_ref().to_string(), value))
        .collect();
    pairs.sort_by(|left, right| left.0.cmp(&right.0));
    write_object(
        pairs
            .iter()
            .map(|(key, value)| (key.as_str(), value.as_str())),
    )
}

fn ordered_object(pairs: &[(&str, String)]) -> String {
    write_object(pairs.iter().map(|(key, value)| (*key, value.as_str())))
}

fn write_object<'a>(pairs: impl IntoIterator<Item = (&'a str, &'a str)>) -> String {
    let mut out = String::from("{");
    for (index, (key, value)) in pairs.into_iter().enumerate() {
        if index > 0 {
            out.push(',');
        }
        out.push_str(&json_string(key));
        out.push(':');
        out.push_str(value);
    }
    out.push('}');
    out
}

fn array(items: &[String]) -> String {
    let mut out = String::from("[");
    for (index, item) in items.iter().enumerate() {
        if index > 0 {
            out.push(',');
        }
        out.push_str(item);
    }
    out.push(']');
    out
}

fn json_string(value: &str) -> String {
    serde_json::to_string(value).unwrap_or_else(|_| "\"\"".to_string())
}
