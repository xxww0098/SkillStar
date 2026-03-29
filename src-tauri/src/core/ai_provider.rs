use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

const AI_MAX_TOKENS: u32 = 196_608;

// ── Configuration ───────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AiConfig {
    pub enabled: bool,
    #[serde(default = "default_api_format")]
    pub api_format: String,
    pub base_url: String,
    pub api_key: String,
    pub model: String,
    pub target_language: String,
}

fn default_api_format() -> String {
    "openai".to_string()
}

impl Default for AiConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            api_format: default_api_format(),
            base_url: "https://api.openai.com/v1".to_string(),
            api_key: String::new(),
            model: "gpt-5.4".to_string(),
            target_language: "zh-CN".to_string(),
        }
    }
}

fn config_path() -> PathBuf {
    dirs::data_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("skillstar")
        .join("ai_config.json")
}

fn get_encryption_key() -> aes_gcm::Key<aes_gcm::Aes256Gcm> {
    use sha2::{Digest, Sha256};
    let machine_id = machine_uid::get().unwrap_or_else(|_| "skillstar-fallback-id-123".to_string());
    let mut hasher = Sha256::new();
    hasher.update(b"skillstar-ai-api-key");
    hasher.update(machine_id.as_bytes());
    let result = hasher.finalize();
    *aes_gcm::Key::<aes_gcm::Aes256Gcm>::from_slice(result.as_slice())
}

fn encrypt_api_key(plain: &str) -> String {
    use aes_gcm::{
        aead::{Aead, AeadCore, OsRng},
        Aes256Gcm, KeyInit,
    };
    use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};

    if plain.is_empty() {
        return String::new();
    }
    let key = get_encryption_key();
    let cipher = Aes256Gcm::new(&key);
    let nonce = Aes256Gcm::generate_nonce(&mut OsRng); // 96-bits; unique per message
    match cipher.encrypt(&nonce, plain.as_bytes()) {
        Ok(ciphertext) => {
            let mut combined = nonce.to_vec();
            combined.extend_from_slice(&ciphertext);
            BASE64.encode(combined)
        }
        Err(_) => plain.to_string(), // fallback
    }
}

fn decrypt_api_key(encoded: &str) -> String {
    use aes_gcm::{aead::Aead, Aes256Gcm, KeyInit, Nonce};
    use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};

    if encoded.is_empty() {
        return String::new();
    }
    let Ok(decoded) = BASE64.decode(encoded) else {
        return encoded.to_string();
    };
    if decoded.len() < 12 {
        return encoded.to_string();
    }
    let (nonce_bytes, ciphertext) = decoded.split_at(12);
    let nonce = Nonce::from_slice(nonce_bytes);
    let key = get_encryption_key();
    let cipher = Aes256Gcm::new(&key);
    match cipher.decrypt(nonce, ciphertext) {
        Ok(plaintext) => String::from_utf8(plaintext).unwrap_or_else(|_| encoded.to_string()),
        Err(_) => encoded.to_string(),
    }
}

pub fn load_config() -> AiConfig {
    let path = config_path();
    if !path.exists() {
        return AiConfig::default();
    }
    match std::fs::read_to_string(&path) {
        Ok(content) => {
            let mut config: AiConfig = serde_json::from_str(&content).unwrap_or_default();
            config.api_key = decrypt_api_key(&config.api_key);
            config
        }
        Err(_) => AiConfig::default(),
    }
}

pub fn save_config(config: &AiConfig) -> Result<()> {
    let path = config_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).context("Failed to create config directory")?;
    }
    let mut config_to_save = config.clone();
    config_to_save.api_key = encrypt_api_key(&config_to_save.api_key);

    let content =
        serde_json::to_string_pretty(&config_to_save).context("Failed to serialize AI config")?;
    std::fs::write(&path, content).context("Failed to write AI config")?;
    Ok(())
}

// ── Language Mapping ────────────────────────────────────────────────

fn language_display_name(code: &str) -> &str {
    match code {
        "zh-CN" => "Simplified Chinese",
        "zh-TW" => "Traditional Chinese",
        "en" => "English",
        "ja" => "Japanese",
        "ko" => "Korean",
        "es" => "Spanish",
        "fr" => "French",
        "de" => "German",
        "ru" => "Russian",
        "pt-BR" => "Brazilian Portuguese",
        "ar" => "Arabic",
        "hi" => "Hindi",
        _ => code,
    }
}

const TRANSLATION_CHUNK_SOFT_LIMIT_CHARS: usize = 10_000;
const TRANSLATION_CHUNK_RETRY_MIN_CHARS: usize = 4_000;

fn is_empty_ai_response_error(err: &anyhow::Error) -> bool {
    err.to_string().contains("AI returned empty response")
}

fn build_translation_system_prompt(lang: &str) -> String {
    format!(
        "You are a professional technical translator. Translate the ENTIRE Markdown document to {}. \
         Rules:\n\
         1. Translate all human-readable prose across the whole file (frontmatter values, headings, paragraphs, list text, table text, blockquotes).\n\
         2. Even when a line contains inline code (text wrapped by backticks), translate the surrounding prose and keep only the inline code span unchanged.\n\
         3. Keep YAML keys unchanged. Keep the `name` field value exactly as original.\n\
         4. Do NOT translate code blocks, inline code spans, variable names, file paths, command names, identifiers, URLs, or markdown syntax tokens.\n\
         5. Preserve document structure exactly: same sections, ordering, markdown constructs, frontmatter delimiters, and overall layout.\n\
         6. Do not add, delete, or reorder content blocks.\n\
         7. Output ONLY the translated document content (no commentary, no code fences around the whole output).",
        lang
    )
}

fn build_short_translation_system_prompt(lang: &str) -> String {
    format!(
        "Translate the following text to {}. \
         Output ONLY the translated text, nothing else. \
         Do not add any explanation, commentary, or surrounding quotes. \
         Keep technical terms, product names, command names, and code identifiers unchanged.",
        lang
    )
}

fn build_summary_system_prompt(lang: &str) -> String {
    format!(
        "You are an AI coding-skill analyst. Analyze the following SKILL.md and produce a concise \
         structured summary in {}. Output format:\n\n\
         📌 **Core Capability**: [1-2 sentences describing what this skill does]\n\n\
         🎯 **Triggers**: [list triggers/commands, or \"No explicit triggers\"]\n\n\
         📦 **Use Cases**: [2-3 bullet points of when to use this skill]\n\n\
         🔧 **Tools Used**: [list allowed tools, or \"Not specified\"]\n\n\
         ⚡ **Key Rules**: [2-3 most important rules or constraints]\n\n\
         Output ONLY the summary, no extra explanation.",
        lang
    )
}

fn build_translation_chunk_prompt(
    base_system_prompt: &str,
    chunk_number: usize,
    total: usize,
) -> String {
    format!(
        "{base_system_prompt}\n\
         Additional chunk mode rules:\n\
         - You are translating chunk {chunk_number}/{total} of one Markdown document.\n\
         - Translate ONLY this chunk.\n\
         - Keep Markdown syntax, fenced-code boundaries, and line structure intact.\n\
         - Output only translated chunk content."
    )
}

fn split_translation_chunks(text: &str, soft_limit_chars: usize) -> Vec<String> {
    if text.len() <= soft_limit_chars || soft_limit_chars == 0 {
        return vec![text.to_string()];
    }

    let mut chunks = Vec::new();
    let mut current = String::new();
    let mut in_fenced_code_block = false;

    for line in text.split_inclusive('\n') {
        let trimmed_start = line.trim_start();
        let starts_fence = trimmed_start.starts_with("```") || trimmed_start.starts_with("~~~");

        let should_split_before_line = !current.is_empty()
            && current.len() + line.len() > soft_limit_chars
            && !in_fenced_code_block;

        if should_split_before_line {
            chunks.push(current);
            current = String::new();
        }

        current.push_str(line);

        if starts_fence {
            in_fenced_code_block = !in_fenced_code_block;
        }
    }

    if !current.is_empty() {
        chunks.push(current);
    }

    if chunks.is_empty() {
        vec![text.to_string()]
    } else {
        chunks
    }
}

async fn translate_text_in_chunks(
    config: &AiConfig,
    base_system_prompt: &str,
    text: &str,
) -> Result<String> {
    let chunks = split_translation_chunks(text, TRANSLATION_CHUNK_SOFT_LIMIT_CHARS);
    if chunks.len() <= 1 {
        return chat_completion(config, base_system_prompt, text).await;
    }

    let total = chunks.len();
    let mut translated = String::new();

    for (index, chunk) in chunks.iter().enumerate() {
        let chunk_number = index + 1;
        let chunk_prompt = build_translation_chunk_prompt(base_system_prompt, chunk_number, total);

        let chunk_result = chat_completion(config, &chunk_prompt, chunk)
            .await
            .with_context(|| format!("Failed to translate chunk {chunk_number}/{total}"))?;

        if chunk_result.trim().is_empty() {
            anyhow::bail!("AI returned empty response for chunk {chunk_number}/{total}");
        }

        translated.push_str(&chunk_result);
        if chunk.ends_with('\n') && !chunk_result.ends_with('\n') {
            translated.push('\n');
        }
    }

    Ok(translated)
}

// ── HTTP Client Builder ─────────────────────────────────────────────

/// Build a reqwest client, optionally honouring the user's proxy config.
fn build_http_client() -> Result<reqwest::Client> {
    // Try to load the proxy configuration
    let proxy_path = dirs::data_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("skillstar")
        .join("proxy.json");

    let mut builder = reqwest::Client::builder().timeout(std::time::Duration::from_secs(120));

    if proxy_path.exists() {
        if let Ok(raw) = std::fs::read_to_string(&proxy_path) {
            if let Ok(proxy_cfg) = serde_json::from_str::<serde_json::Value>(&raw) {
                if proxy_cfg
                    .get("enabled")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false)
                {
                    let ptype = proxy_cfg
                        .get("proxy_type")
                        .and_then(|v| v.as_str())
                        .unwrap_or("http");
                    let host = proxy_cfg.get("host").and_then(|v| v.as_str()).unwrap_or("");
                    let port = proxy_cfg
                        .get("port")
                        .and_then(|v| v.as_u64())
                        .unwrap_or(7897);

                    if !host.is_empty() {
                        let proxy_url = format!("{}://{}:{}", ptype, host, port);
                        let proxy = reqwest::Proxy::all(&proxy_url).context("Invalid proxy URL")?;
                        builder = builder.proxy(proxy);
                    }
                }
            }
        }
    }

    builder.build().context("Failed to build HTTP client")
}

// ── OpenAI-Compatible Chat Completion ────────────────────────────────

#[derive(Serialize)]
struct ChatMessage {
    role: String,
    content: String,
}

#[derive(Serialize)]
struct ChatRequest {
    model: String,
    messages: Vec<ChatMessage>,
    temperature: f32,
    max_tokens: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    seed: Option<u64>,
}

#[derive(Serialize)]
struct ChatStreamRequest {
    model: String,
    messages: Vec<ChatMessage>,
    temperature: f32,
    max_tokens: u32,
    stream: bool,
}

#[derive(Deserialize)]
struct ChatChoice {
    message: ChatMessageResponse,
}

#[derive(Deserialize)]
struct ChatMessageResponse {
    content: String,
}

#[derive(Deserialize)]
struct ChatResponse {
    choices: Vec<ChatChoice>,
}

#[derive(Serialize)]
struct AnthropicMessage {
    role: String,
    content: String,
}

#[derive(Serialize)]
struct AnthropicRequest {
    model: String,
    max_tokens: u32,
    system: String,
    messages: Vec<AnthropicMessage>,
}

#[derive(Serialize)]
struct AnthropicStreamRequest {
    model: String,
    max_tokens: u32,
    system: String,
    messages: Vec<AnthropicMessage>,
    stream: bool,
}

#[derive(Deserialize)]
struct AnthropicTextBlock {
    #[serde(rename = "type")]
    kind: String,
    text: Option<String>,
}

#[derive(Deserialize)]
struct AnthropicResponse {
    content: Vec<AnthropicTextBlock>,
}

fn is_anthropic_format(config: &AiConfig) -> bool {
    config.api_format.eq_ignore_ascii_case("anthropic")
}

fn build_openai_chat_url(base_url: &str) -> String {
    let base = base_url.trim_end_matches('/');
    if base.ends_with("/chat/completions") {
        base.to_string()
    } else {
        format!("{}/chat/completions", base)
    }
}

fn build_anthropic_messages_url(base_url: &str) -> String {
    let base = base_url.trim_end_matches('/');
    if base.ends_with("/v1/messages") || base.ends_with("/messages") {
        base.to_string()
    } else if base.ends_with("/v1") {
        format!("{}/messages", base)
    } else {
        format!("{}/v1/messages", base)
    }
}

async fn openai_chat_completion(
    config: &AiConfig,
    system_prompt: &str,
    user_content: &str,
) -> Result<String> {
    openai_chat_completion_with_opts(config, system_prompt, user_content, 0.3, None).await
}

async fn openai_chat_completion_with_opts(
    config: &AiConfig,
    system_prompt: &str,
    user_content: &str,
    temperature: f32,
    seed: Option<u64>,
) -> Result<String> {
    let client = build_http_client()?;
    let url = build_openai_chat_url(&config.base_url);

    let body = ChatRequest {
        model: config.model.clone(),
        messages: vec![
            ChatMessage {
                role: "system".to_string(),
                content: system_prompt.to_string(),
            },
            ChatMessage {
                role: "user".to_string(),
                content: user_content.to_string(),
            },
        ],
        temperature,
        max_tokens: AI_MAX_TOKENS,
        seed,
    };

    let resp = client
        .post(&url)
        .header("Content-Type", "application/json")
        .header("Authorization", format!("Bearer {}", config.api_key))
        .json(&body)
        .send()
        .await
        .context("Failed to send request to AI provider")?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body_text = resp.text().await.unwrap_or_default();
        anyhow::bail!("AI API returned {} — {}", status, body_text);
    }

    let chat_resp: ChatResponse = resp.json().await.context("Failed to parse AI response")?;

    chat_resp
        .choices
        .into_iter()
        .next()
        .map(|c| c.message.content)
        .context("AI returned empty response")
}

fn process_openai_stream_data_event<F>(
    data_lines: &[String],
    translated: &mut String,
    on_delta: &mut F,
) -> Result<()>
where
    F: FnMut(&str) -> Result<()>,
{
    if data_lines.is_empty() {
        return Ok(());
    }

    let data = data_lines.join("\n");
    let trimmed = data.trim();
    if trimmed.is_empty() || trimmed == "[DONE]" {
        return Ok(());
    }

    let value: serde_json::Value = serde_json::from_str(trimmed)
        .with_context(|| "Failed to parse AI streaming payload".to_string())?;

    if let Some(message) = value
        .get("error")
        .and_then(|error| error.get("message"))
        .and_then(|msg| msg.as_str())
    {
        anyhow::bail!("AI API stream error — {}", message);
    }

    let delta = value
        .pointer("/choices/0/delta/content")
        .and_then(|v| v.as_str())
        .or_else(|| {
            value
                .pointer("/choices/0/message/content")
                .and_then(|v| v.as_str())
        });

    if let Some(delta_text) = delta.filter(|s| !s.is_empty()) {
        translated.push_str(delta_text);
        on_delta(delta_text)?;
    }

    Ok(())
}

async fn openai_chat_completion_stream<F>(
    config: &AiConfig,
    system_prompt: &str,
    user_content: &str,
    on_delta: &mut F,
) -> Result<String>
where
    F: FnMut(&str) -> Result<()>,
{
    let client = build_http_client()?;
    let url = build_openai_chat_url(&config.base_url);

    let body = ChatStreamRequest {
        model: config.model.clone(),
        messages: vec![
            ChatMessage {
                role: "system".to_string(),
                content: system_prompt.to_string(),
            },
            ChatMessage {
                role: "user".to_string(),
                content: user_content.to_string(),
            },
        ],
        temperature: 0.3,
        max_tokens: AI_MAX_TOKENS,
        stream: true,
    };

    let mut resp = client
        .post(&url)
        .header("Content-Type", "application/json")
        .header("Accept", "text/event-stream")
        .header("Authorization", format!("Bearer {}", config.api_key))
        .json(&body)
        .send()
        .await
        .context("Failed to send streaming request to AI provider")?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body_text = resp.text().await.unwrap_or_default();
        anyhow::bail!("AI API returned {} — {}", status, body_text);
    }

    let mut translated = String::new();
    let mut buffer = String::new();
    let mut event_data_lines: Vec<String> = Vec::new();

    while let Some(chunk) = resp
        .chunk()
        .await
        .context("Failed to read streaming response from AI provider")?
    {
        let chunk_text = String::from_utf8_lossy(&chunk);
        buffer.push_str(&chunk_text);

        while let Some(newline_idx) = buffer.find('\n') {
            let mut line = buffer[..newline_idx].to_string();
            buffer.drain(..=newline_idx);

            if line.ends_with('\r') {
                line.pop();
            }

            if line.is_empty() {
                process_openai_stream_data_event(&event_data_lines, &mut translated, on_delta)?;
                event_data_lines.clear();
                continue;
            }

            if let Some(data_part) = line.strip_prefix("data:") {
                event_data_lines.push(data_part.trim_start().to_string());
            }
        }
    }

    if !buffer.trim().is_empty() {
        let mut tail = buffer;
        if tail.ends_with('\r') {
            tail.pop();
        }
        if let Some(data_part) = tail.strip_prefix("data:") {
            event_data_lines.push(data_part.trim_start().to_string());
        }
    }

    if !event_data_lines.is_empty() {
        process_openai_stream_data_event(&event_data_lines, &mut translated, on_delta)?;
    }

    if translated.trim().is_empty() {
        anyhow::bail!("AI returned empty response");
    }

    Ok(translated)
}

async fn translate_text_in_chunks_streaming<F>(
    config: &AiConfig,
    base_system_prompt: &str,
    text: &str,
    on_delta: &mut F,
) -> Result<String>
where
    F: FnMut(&str) -> Result<()>,
{
    let chunks = split_translation_chunks(text, TRANSLATION_CHUNK_SOFT_LIMIT_CHARS);
    if chunks.len() <= 1 {
        return chat_completion_stream(config, base_system_prompt, text, on_delta).await;
    }

    let total = chunks.len();
    let mut translated = String::new();

    for (index, chunk) in chunks.iter().enumerate() {
        let chunk_number = index + 1;
        let chunk_prompt = build_translation_chunk_prompt(base_system_prompt, chunk_number, total);
        let chunk_result = chat_completion_stream(config, &chunk_prompt, chunk, on_delta)
            .await
            .with_context(|| format!("Failed to stream-translate chunk {chunk_number}/{total}"))?;

        if chunk_result.trim().is_empty() {
            anyhow::bail!("AI returned empty response for chunk {chunk_number}/{total}");
        }

        translated.push_str(&chunk_result);
        if chunk.ends_with('\n') && !chunk_result.ends_with('\n') {
            translated.push('\n');
            on_delta("\n")?;
        }
    }

    Ok(translated)
}

async fn anthropic_messages_completion(
    config: &AiConfig,
    system_prompt: &str,
    user_content: &str,
) -> Result<String> {
    let client = build_http_client()?;
    let url = build_anthropic_messages_url(&config.base_url);

    let body = AnthropicRequest {
        model: config.model.clone(),
        max_tokens: AI_MAX_TOKENS,
        system: system_prompt.to_string(),
        messages: vec![AnthropicMessage {
            role: "user".to_string(),
            content: user_content.to_string(),
        }],
    };

    let resp = client
        .post(&url)
        .header("Content-Type", "application/json")
        .header("x-api-key", &config.api_key)
        .header("anthropic-version", "2023-06-01")
        .json(&body)
        .send()
        .await
        .context("Failed to send request to AI provider")?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body_text = resp.text().await.unwrap_or_default();
        anyhow::bail!("AI API returned {} — {}", status, body_text);
    }

    let anthropic_resp: AnthropicResponse =
        resp.json().await.context("Failed to parse AI response")?;

    let text = anthropic_resp
        .content
        .iter()
        .filter(|b| b.kind == "text")
        .filter_map(|b| b.text.as_ref().map(|t| t.trim()))
        .filter(|t| !t.is_empty())
        .collect::<Vec<_>>()
        .join("\n\n");

    if text.is_empty() {
        anyhow::bail!("AI returned empty response");
    }

    Ok(text)
}

/// Anthropic Messages API with real SSE streaming.
/// Anthropic's SSE format uses event types like `content_block_delta` with
/// `delta.type = "text_delta"` and `delta.text` for incremental content.
async fn anthropic_messages_completion_stream<F>(
    config: &AiConfig,
    system_prompt: &str,
    user_content: &str,
    on_delta: &mut F,
) -> Result<String>
where
    F: FnMut(&str) -> Result<()>,
{
    let client = build_http_client()?;
    let url = build_anthropic_messages_url(&config.base_url);

    let body = AnthropicStreamRequest {
        model: config.model.clone(),
        max_tokens: AI_MAX_TOKENS,
        system: system_prompt.to_string(),
        messages: vec![AnthropicMessage {
            role: "user".to_string(),
            content: user_content.to_string(),
        }],
        stream: true,
    };

    let mut resp = client
        .post(&url)
        .header("Content-Type", "application/json")
        .header("Accept", "text/event-stream")
        .header("x-api-key", &config.api_key)
        .header("anthropic-version", "2023-06-01")
        .json(&body)
        .send()
        .await
        .context("Failed to send streaming request to Anthropic API")?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body_text = resp.text().await.unwrap_or_default();
        anyhow::bail!("Anthropic API returned {} — {}", status, body_text);
    }

    let mut translated = String::new();
    let mut buffer = String::new();
    let mut current_event_type = String::new();
    let mut event_data_lines: Vec<String> = Vec::new();

    while let Some(chunk) = resp
        .chunk()
        .await
        .context("Failed to read streaming response from Anthropic API")?
    {
        let chunk_text = String::from_utf8_lossy(&chunk);
        buffer.push_str(&chunk_text);

        while let Some(newline_idx) = buffer.find('\n') {
            let mut line = buffer[..newline_idx].to_string();
            buffer.drain(..=newline_idx);

            if line.ends_with('\r') {
                line.pop();
            }

            // Empty line = end of SSE event block
            if line.is_empty() {
                if !event_data_lines.is_empty() {
                    let data = event_data_lines.join("\n");
                    process_anthropic_sse_event(
                        &current_event_type,
                        &data,
                        &mut translated,
                        on_delta,
                    )?;
                    event_data_lines.clear();
                }
                current_event_type.clear();
                continue;
            }

            if let Some(event_type) = line.strip_prefix("event:") {
                current_event_type = event_type.trim().to_string();
            } else if let Some(data_part) = line.strip_prefix("data:") {
                event_data_lines.push(data_part.trim_start().to_string());
            }
        }
    }

    // Process any remaining buffered event
    if !event_data_lines.is_empty() {
        let data = event_data_lines.join("\n");
        process_anthropic_sse_event(&current_event_type, &data, &mut translated, on_delta)?;
    }

    if translated.trim().is_empty() {
        anyhow::bail!("AI returned empty response");
    }

    Ok(translated)
}

/// Parse a single Anthropic SSE event and extract delta text.
fn process_anthropic_sse_event<F>(
    event_type: &str,
    data: &str,
    translated: &mut String,
    on_delta: &mut F,
) -> Result<()>
where
    F: FnMut(&str) -> Result<()>,
{
    let trimmed = data.trim();
    if trimmed.is_empty() {
        return Ok(());
    }

    let value: serde_json::Value = match serde_json::from_str(trimmed) {
        Ok(v) => v,
        Err(_) => return Ok(()), // skip unparseable events
    };

    // Check for error events
    if event_type == "error" {
        let message = value
            .get("error")
            .and_then(|e| e.get("message"))
            .and_then(|m| m.as_str())
            .or_else(|| value.get("message").and_then(|m| m.as_str()))
            .unwrap_or("Unknown Anthropic API error");
        anyhow::bail!("Anthropic API stream error — {}", message);
    }

    // content_block_delta: extract delta.text
    if event_type == "content_block_delta" {
        if let Some(delta_text) = value
            .get("delta")
            .and_then(|d| d.get("text"))
            .and_then(|t| t.as_str())
        {
            if !delta_text.is_empty() {
                translated.push_str(delta_text);
                on_delta(delta_text)?;
            }
        }
    }

    Ok(())
}

async fn chat_completion(
    config: &AiConfig,
    system_prompt: &str,
    user_content: &str,
) -> Result<String> {
    if is_anthropic_format(config) {
        anthropic_messages_completion(config, system_prompt, user_content).await
    } else {
        openai_chat_completion(config, system_prompt, user_content).await
    }
}

/// Chat completion with temperature and seed overrides for deterministic output.
async fn chat_completion_deterministic(
    config: &AiConfig,
    system_prompt: &str,
    user_content: &str,
    seed: Option<u64>,
) -> Result<String> {
    if is_anthropic_format(config) {
        // Anthropic API does not support temperature/seed overrides in this wrapper,
        // but temperature 0 is roughly achieved by using the same prompt.
        anthropic_messages_completion(config, system_prompt, user_content).await
    } else {
        openai_chat_completion_with_opts(config, system_prompt, user_content, 0.0, seed).await
    }
}

/// Generic streaming chat completion dispatcher — routes to OpenAI or Anthropic.
async fn chat_completion_stream<F>(
    config: &AiConfig,
    system_prompt: &str,
    user_content: &str,
    on_delta: &mut F,
) -> Result<String>
where
    F: FnMut(&str) -> Result<()>,
{
    if is_anthropic_format(config) {
        anthropic_messages_completion_stream(config, system_prompt, user_content, on_delta).await
    } else {
        openai_chat_completion_stream(config, system_prompt, user_content, on_delta).await
    }
}

// ── Public API ───────────────────────────────────────────────────────

/// Translate a SKILL.md content to the target language.
/// Preserves markdown formatting; only translates natural language text.
pub async fn translate_text(config: &AiConfig, text: &str) -> Result<String> {
    let lang = language_display_name(&config.target_language);
    let system_prompt = build_translation_system_prompt(lang);

    match chat_completion(config, &system_prompt, text).await {
        Ok(result) if !result.trim().is_empty() => Ok(result),
        Ok(_) => {
            if text.len() < TRANSLATION_CHUNK_RETRY_MIN_CHARS {
                anyhow::bail!("AI returned empty response");
            }
            translate_text_in_chunks(config, &system_prompt, text)
                .await
                .context("Single-pass translation returned empty response; chunked fallback failed")
        }
        Err(err) => {
            if text.len() < TRANSLATION_CHUNK_RETRY_MIN_CHARS || !is_empty_ai_response_error(&err) {
                return Err(err);
            }

            translate_text_in_chunks(config, &system_prompt, text).await.with_context(|| {
                format!(
                    "Single-pass translation failed with empty response for {} chars; chunked fallback failed",
                    text.len()
                )
            })
        }
    }
}

/// Translate SKILL.md content with streaming delta callbacks.
/// Supports both OpenAI and Anthropic formats with real SSE streaming.
pub async fn translate_text_streaming<F>(
    config: &AiConfig,
    text: &str,
    on_delta: &mut F,
) -> Result<String>
where
    F: FnMut(&str) -> Result<()>,
{
    let lang = language_display_name(&config.target_language);
    let system_prompt = build_translation_system_prompt(lang);

    match chat_completion_stream(config, &system_prompt, text, on_delta).await {
        Ok(result) if !result.trim().is_empty() => Ok(result),
        Ok(_) => {
            if text.len() < TRANSLATION_CHUNK_RETRY_MIN_CHARS {
                anyhow::bail!("AI returned empty response");
            }
            translate_text_in_chunks_streaming(config, &system_prompt, text, on_delta)
                .await
                .context("Streaming translation returned empty response; chunked fallback failed")
        }
        Err(err) => {
            if text.len() >= TRANSLATION_CHUNK_RETRY_MIN_CHARS && is_empty_ai_response_error(&err) {
                return translate_text_in_chunks_streaming(config, &system_prompt, text, on_delta)
                    .await
                    .with_context(|| {
                        format!(
                            "Single-pass streaming translation failed with empty response for {} chars; chunked fallback failed",
                            text.len()
                        )
                    });
            }

            translate_text(config, text).await.with_context(|| {
                format!(
                    "Streaming translation failed ({}); non-stream fallback failed",
                    err
                )
            })
        }
    }
}

/// Translate a short description / text snippet (not a full Markdown document).
/// Uses a simpler, more direct prompt to avoid the AI treating short text as conversation.
pub async fn translate_short_text(config: &AiConfig, text: &str) -> Result<String> {
    let lang = language_display_name(&config.target_language);
    let system_prompt = build_short_translation_system_prompt(lang);
    chat_completion(config, &system_prompt, text).await
}

/// Translate a short description with streaming delta callbacks.
pub async fn translate_short_text_streaming<F>(
    config: &AiConfig,
    text: &str,
    on_delta: &mut F,
) -> Result<String>
where
    F: FnMut(&str) -> Result<()>,
{
    let lang = language_display_name(&config.target_language);
    let system_prompt = build_short_translation_system_prompt(lang);

    match chat_completion_stream(config, &system_prompt, text, on_delta).await {
        Ok(result) if !result.trim().is_empty() => Ok(result),
        Ok(_) => {
            // Streaming returned empty — fall back to non-streaming
            translate_short_text(config, text).await
        }
        Err(_) => {
            // Streaming failed — fall back to non-streaming
            translate_short_text(config, text).await
        }
    }
}

/// Generate a structured summary of a SKILL.md content.
pub async fn summarize_text(config: &AiConfig, text: &str) -> Result<String> {
    let lang = language_display_name(&config.target_language);
    let system_prompt = build_summary_system_prompt(lang);

    chat_completion(config, &system_prompt, text).await
}

/// Generate a structured summary with streaming delta callbacks.
/// Supports both OpenAI and Anthropic formats with real SSE streaming.
pub async fn summarize_text_streaming<F>(
    config: &AiConfig,
    text: &str,
    on_delta: &mut F,
) -> Result<String>
where
    F: FnMut(&str) -> Result<()>,
{
    let lang = language_display_name(&config.target_language);
    let system_prompt = build_summary_system_prompt(lang);

    match chat_completion_stream(config, &system_prompt, text, on_delta).await {
        Ok(result) if !result.trim().is_empty() => Ok(result),
        Ok(_) => summarize_text(config, text)
            .await
            .context("Streaming summary returned empty response; non-stream fallback failed"),
        Err(err) => summarize_text(config, text).await.with_context(|| {
            format!(
                "Streaming summary failed ({}); non-stream fallback failed",
                err
            )
        }),
    }
}

/// Pick the most relevant skills from a catalog based on a user-provided project description.
/// Uses a 3-round consensus mechanism for stable, repeatable results.
/// A skill must appear in at least 2 out of 3 rounds to be selected.
pub async fn pick_skills(
    config: &AiConfig,
    prompt: &str,
    skill_catalog: &str,
) -> Result<Vec<String>> {
    let system_prompt = format!(
        "You are a deterministic skill-matching engine. Given a project description \
         and a skill catalog, you must decide which skills are relevant.\n\n\
         ## Evaluation Method\n\
         For EACH skill in the catalog:\n\
         1. Read its name and description.\n\
         2. Decide: Is this skill directly useful or closely related to the described project?\n\
         3. If YES → include it. If NO → omit it.\n\n\
         ## Rules\n\
         - Be INCLUSIVE: when a skill is even somewhat related, include it.\n\
         - Only exclude skills that have ZERO relevance to the project.\n\
         - The skill names in your output MUST exactly match the names in the catalog (case-sensitive).\n\
         - Return a JSON array of selected skill names. Example: [\"skill-a\", \"skill-b\"]\n\
         - Output ONLY the JSON array. No commentary, no markdown fences, no explanation.\n\
         - If nothing matches, return [].\n\n\
         ## Available Skills Catalog\n\n{skill_catalog}",
        skill_catalog = skill_catalog,
    );

    // Run 3 rounds concurrently with different seeds for consensus
    let seeds = [42u64, 123, 7];
    let mut handles = Vec::new();

    for &seed in &seeds {
        let cfg = config.clone();
        let sp = system_prompt.clone();
        let p = prompt.to_string();
        handles.push(tokio::spawn(async move {
            chat_completion_deterministic(&cfg, &sp, &p, Some(seed)).await
        }));
    }

    // Collect results
    let mut vote_counts: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
    let mut success_count = 0usize;

    for handle in handles {
        let result = handle
            .await
            .map_err(|e| anyhow::anyhow!("Skill-pick task panicked: {}", e))?;

        match result {
            Ok(raw) => {
                if let Ok(names) = parse_json_array_from_response(&raw) {
                    success_count += 1;
                    for name in names {
                        *vote_counts.entry(name).or_insert(0) += 1;
                    }
                }
            }
            Err(e) => {
                eprintln!("[ai_pick_skills] Round failed: {}", e);
            }
        }
    }

    if success_count == 0 {
        anyhow::bail!("All 3 AI skill-pick rounds failed. Please check your AI provider settings.");
    }

    // Consensus: select skills that appear in at least 2 rounds (or 1 if only 1 round succeeded)
    let threshold = if success_count >= 2 { 2 } else { 1 };
    let mut selected: Vec<String> = vote_counts
        .into_iter()
        .filter(|(_, count)| *count >= threshold)
        .map(|(name, _)| name)
        .collect();
    selected.sort();

    Ok(selected)
}

/// Parse a JSON array of strings from an AI response, tolerant of markdown fences.
fn parse_json_array_from_response(raw: &str) -> Result<Vec<String>> {
    let trimmed = raw.trim();
    let json_str = if let Some(start) = trimmed.find('[') {
        if let Some(end) = trimmed.rfind(']') {
            &trimmed[start..=end]
        } else {
            trimmed
        }
    } else {
        trimmed
    };

    let names: Vec<String> = serde_json::from_str(json_str).with_context(|| {
        format!(
            "Failed to parse AI skill-pick response as JSON array: {}",
            json_str
        )
    })?;

    Ok(names)
}

/// Test API connectivity with a minimal request.
pub async fn test_connection(config: &AiConfig) -> Result<String> {
    let system_prompt = "Reply with exactly: connection_ok";
    let result = chat_completion(config, system_prompt, "ping").await?;
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::split_translation_chunks;

    #[test]
    fn split_translation_chunks_preserves_full_content() {
        let text = "## Intro\nline 1\nline 2\n\n## Next\nline 3\nline 4\n";
        let chunks = split_translation_chunks(text, 18);
        assert!(chunks.len() > 1);
        assert_eq!(chunks.concat(), text);
    }

    #[test]
    fn split_translation_chunks_no_split_for_short_input() {
        let text = "short markdown";
        let chunks = split_translation_chunks(text, 10_000);
        assert_eq!(chunks, vec![text.to_string()]);
    }

    #[test]
    fn split_translation_chunks_avoids_mid_fence_split() {
        let text = "Header\n```bash\nline-a\nline-b\nline-c\n```\nTail\n";
        let chunks = split_translation_chunks(text, 20);
        assert_eq!(chunks.concat(), text);
        assert!(chunks.iter().any(|chunk| chunk.contains("```bash\nline-a")));
        assert!(chunks.iter().any(|chunk| chunk.contains("line-c\n```")));
    }
}
