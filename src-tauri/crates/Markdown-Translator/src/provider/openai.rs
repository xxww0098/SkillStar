use std::env;

use reqwest::Client;
use serde::Deserialize;
use tracing::{debug, info};

use crate::config::ProviderConfig;
use crate::error::{Error, Result};
use crate::provider::LlmProvider;
use crate::types::ApiUsage;

/// OpenAI-compatible chat completions provider.
///
/// Works with any API that implements the OpenAI Chat Completions spec
/// (OpenAI, Azure, Ollama, vLLM, LiteLLM, etc.).
pub struct OpenAiProvider {
    client: Client,
    base_url: String,
    api_key: String,
    model: String,
    temperature: f64,
    max_tokens: u32,
    usage: ApiUsage,
}

impl OpenAiProvider {
    pub fn new(config: &ProviderConfig) -> Self {
        let api_key = resolve_api_key(config.api_key.as_deref(), &config.api_key_env);
        Self {
            client: Client::new(),
            base_url: config.base_url.trim_end_matches('/').to_owned(),
            api_key,
            model: config.model.clone(),
            temperature: config.temperature,
            max_tokens: config.max_tokens,
            usage: ApiUsage::new(),
        }
    }
}

#[async_trait::async_trait]
impl LlmProvider for OpenAiProvider {
    async fn chat_json(
        &self,
        system_prompt: &str,
        user_prompt: &str,
        call_label: &str,
    ) -> Result<serde_json::Value> {
        let url = format!("{}/chat/completions", self.base_url);
        let start = std::time::Instant::now();

        info!(
            call_label,
            model = %self.model,
            system_chars = system_prompt.len(),
            user_chars = user_prompt.len(),
            "LLM request start"
        );
        debug!(call_label, system = system_prompt, "system prompt");
        debug!(call_label, user = user_prompt, "user prompt");

        let request_body = serde_json::json!({
            "model": self.model,
            "temperature": self.temperature,
            "max_tokens": self.max_tokens,
            "response_format": { "type": "json_object" },
            "messages": [
                { "role": "system", "content": system_prompt },
                { "role": "user", "content": user_prompt },
            ],
        });

        let response = self
            .client
            .post(&url)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header("Content-Type", "application/json")
            .json(&request_body)
            .send()
            .await?;

        let status = response.status();
        if !status.is_success() {
            let body = response.text().await.unwrap_or_default();
            return Err(Error::Api {
                status: status.as_u16(),
                body,
            });
        }

        let raw: ChatCompletionResponse = response.json().await?;
        let content = raw
            .choices
            .first()
            .and_then(|c| c.message.content.as_deref())
            .unwrap_or("{}");

        // Record usage.
        let (prompt_tok, completion_tok, total_tok) = extract_usage(&raw.usage);
        self.usage.record(prompt_tok, completion_tok, total_tok);

        let elapsed_ms = start.elapsed().as_millis();
        info!(
            call_label,
            elapsed_ms,
            content_chars = content.len(),
            prompt_tokens = prompt_tok,
            completion_tokens = completion_tok,
            total_tokens = total_tok,
            "LLM request done"
        );
        debug!(call_label, raw_output = content, "LLM raw output");

        let parsed: serde_json::Value =
            serde_json::from_str(content).map_err(|e| Error::LlmOutputParse {
                bundle_id: call_label.to_owned(),
                source: e,
            })?;

        Ok(parsed)
    }

    async fn chat_text(
        &self,
        system_prompt: &str,
        user_prompt: &str,
        call_label: &str,
    ) -> Result<String> {
        let url = format!("{}/chat/completions", self.base_url);
        let start = std::time::Instant::now();

        info!(
            call_label,
            model = %self.model,
            system_chars = system_prompt.len(),
            user_chars = user_prompt.len(),
            "LLM request start (text mode)"
        );
        debug!(call_label, system = system_prompt, "system prompt");
        debug!(call_label, user = user_prompt, "user prompt");

        let request_body = serde_json::json!({
            "model": self.model,
            "temperature": self.temperature,
            "max_tokens": self.max_tokens,
            "messages": [
                { "role": "system", "content": system_prompt },
                { "role": "user", "content": user_prompt },
            ],
        });

        let response = self
            .client
            .post(&url)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header("Content-Type", "application/json")
            .json(&request_body)
            .send()
            .await?;

        let status = response.status();
        if !status.is_success() {
            let body = response.text().await.unwrap_or_default();
            return Err(Error::Api {
                status: status.as_u16(),
                body,
            });
        }

        let raw: ChatCompletionResponse = response.json().await?;
        let content = raw
            .choices
            .first()
            .and_then(|c| c.message.content.as_deref())
            .unwrap_or("");

        // Record usage.
        let (prompt_tok, completion_tok, total_tok) = extract_usage(&raw.usage);
        self.usage.record(prompt_tok, completion_tok, total_tok);

        let elapsed_ms = start.elapsed().as_millis();
        info!(
            call_label,
            elapsed_ms,
            content_chars = content.len(),
            prompt_tokens = prompt_tok,
            completion_tokens = completion_tok,
            total_tokens = total_tok,
            "LLM request done (text mode)"
        );
        debug!(call_label, raw_output = content, "LLM raw output");

        Ok(content.to_string())
    }

    fn usage(&self) -> &ApiUsage {
        &self.usage
    }
}

// ─── Helpers ────────────────────────────────────────────────────────────────

fn resolve_api_key(api_key: Option<&str>, api_key_env: &str) -> String {
    // 1. Direct api_key value.
    if let Some(key) = api_key {
        if !key.is_empty() {
            return key.to_owned();
        }
    }
    // 2. If api_key_env looks like a raw key (starts with sk-, or >24 chars), use it directly.
    if api_key_env.starts_with("sk-")
        || api_key_env.starts_with("sess-")
        || api_key_env.starts_with("Bearer ")
        || api_key_env.len() > 24
    {
        return api_key_env.to_owned();
    }
    // 3. Otherwise treat it as an environment variable name.
    if !api_key_env.is_empty() {
        if let Ok(val) = env::var(api_key_env) {
            return val;
        }
    }
    String::new()
}

fn extract_usage(usage: &Option<ChatUsage>) -> (u64, u64, u64) {
    match usage {
        Some(u) => {
            let total = if u.total_tokens > 0 {
                u.total_tokens
            } else {
                u.prompt_tokens + u.completion_tokens
            };
            (u.prompt_tokens, u.completion_tokens, total)
        }
        None => (0, 0, 0),
    }
}

// ─── Response types ─────────────────────────────────────────────────────────

#[derive(Deserialize)]
struct ChatCompletionResponse {
    choices: Vec<ChatChoice>,
    #[serde(default)]
    usage: Option<ChatUsage>,
}

#[derive(Deserialize)]
struct ChatChoice {
    message: ChatMessage,
}

#[derive(Deserialize)]
struct ChatMessage {
    content: Option<String>,
}

#[derive(Deserialize)]
struct ChatUsage {
    #[serde(default)]
    prompt_tokens: u64,
    #[serde(default)]
    completion_tokens: u64,
    #[serde(default)]
    total_tokens: u64,
}
