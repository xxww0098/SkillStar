//! OpenAI-compatible client powered by `async-openai`.
//!
//! Wraps the `async-openai` crate to provide chat completion (non-streaming
//! and streaming) while honouring SkillStar's proxy configuration.
//!
//! `async-openai` 0.27 depends on `reqwest` 0.12, while the rest of the project
//! uses `reqwest` 0.13. We import `reqwest` 0.12 as `reqwest_012` (renamed in
//! Cargo.toml) to build a proxy-aware HTTP client that we inject into
//! `async-openai` via `Client::with_http_client`.

use anyhow::{Context, Result};
use async_openai::{
    Client,
    config::OpenAIConfig,
    types::{
        ChatCompletionRequestSystemMessageArgs, ChatCompletionRequestUserMessageArgs,
        CreateChatCompletionRequestArgs,
    },
};
use futures_util::StreamExt;
use tracing::{error, warn};

use super::config::{AiConfig, ApiFormat};

/// Build an `async-openai` [`Client`] configured with the user's base URL,
/// API key, and proxy-aware HTTP client (reqwest 0.12).
fn build_openai_client(config: &AiConfig) -> Result<Client<OpenAIConfig>> {
    let base_url = normalize_base_url(&config.base_url);
    let api_key = effective_api_key(config);

    let openai_config = OpenAIConfig::new()
        .with_api_base(&base_url)
        .with_api_key(&api_key);

    let http_client = build_proxy_aware_client()?;

    Ok(Client::with_config(openai_config).with_http_client(http_client))
}

/// Build a `reqwest` 0.12 client with SkillStar's proxy settings applied.
fn build_proxy_aware_client() -> Result<reqwest_012::Client> {
    use skillstar_core::config::proxy;

    let mut builder = reqwest_012::Client::builder()
        .timeout(std::time::Duration::from_secs(120))
        .connect_timeout(std::time::Duration::from_secs(10))
        .pool_idle_timeout(std::time::Duration::from_secs(90))
        .pool_max_idle_per_host(4);

    if let Ok(proxy_config) = proxy::load_config() {
        if proxy_config.enabled && !proxy_config.host.trim().is_empty() {
            let scheme = proxy_config.proxy_type.as_scheme();
            let proxy_url = format!("{}://{}:{}", scheme, proxy_config.host, proxy_config.port);

            let mut proxy =
                reqwest_012::Proxy::all(&proxy_url).context("Invalid proxy URL for AI client")?;

            if let Some(ref username) = proxy_config.username {
                if !username.is_empty() {
                    let password = proxy_config.password.as_deref().unwrap_or("");
                    proxy = proxy.basic_auth(username, password);
                }
            }

            builder = builder.proxy(proxy);
        }
    }

    builder.build().context("Failed to build AI HTTP client")
}

/// Normalize the base URL to what `async-openai` expects (the API base without
/// `/chat/completions` — the library appends the endpoint path itself).
fn normalize_base_url(base_url: &str) -> String {
    let mut base = base_url.trim_end_matches('/');
    if base.is_empty() {
        return "https://api.openai.com/v1".to_string();
    }
    // Strip trailing /chat/completions if present — async-openai appends it.
    if base.ends_with("/chat/completions") {
        base = base.trim_end_matches("/chat/completions").trim_end_matches('/');
    }
    // Auto-insert /v1 for bare host:port URLs (e.g. http://host:1234)
    if let Some(after_scheme) = base.split_once("://").map(|(_, rest)| rest) {
        if !after_scheme.contains('/') {
            return format!("{}/v1", base);
        }
    }
    base.to_string()
}

/// For local API format, use a dummy token if api_key is empty.
fn effective_api_key(config: &AiConfig) -> String {
    if config.api_key.trim().is_empty() && matches!(config.api_format, ApiFormat::Local) {
        "ollama".to_string()
    } else {
        config.api_key.clone()
    }
}

/// Non-streaming chat completion via `async-openai`.
pub async fn chat_completion(
    config: &AiConfig,
    system_prompt: &str,
    user_content: &str,
    temperature: f32,
    seed: Option<u64>,
    max_tokens: Option<u32>,
) -> Result<String> {
    let client = build_openai_client(config)?;

    let mut request_builder = CreateChatCompletionRequestArgs::default();
    request_builder
        .model(&config.model)
        .messages(vec![
            ChatCompletionRequestSystemMessageArgs::default()
                .content(system_prompt)
                .build()?
                .into(),
            ChatCompletionRequestUserMessageArgs::default()
                .content(user_content)
                .build()?
                .into(),
        ])
        .temperature(temperature);

    if let Some(max) = max_tokens {
        request_builder.max_tokens(max);
    }

    if let Some(s) = seed {
        request_builder.seed(s as i64);
    }

    let request = request_builder
        .build()
        .context("Failed to build chat completion request")?;

    let response = client
        .chat()
        .create(request)
        .await
        .context("Failed to send request to AI provider")?;

    let content = response
        .choices
        .into_iter()
        .next()
        .and_then(|choice| choice.message.content);

    match content {
        Some(c) if !c.trim().is_empty() => Ok(c),
        _ => {
            warn!(target: "ai_provider", "AI returned empty/null content");
            anyhow::bail!("AI returned empty response");
        }
    }
}

/// Streaming chat completion via `async-openai`.
///
/// Calls `on_delta` for each incremental text chunk received from the API.
/// Returns the fully assembled response text.
pub async fn chat_completion_stream<F>(
    config: &AiConfig,
    system_prompt: &str,
    user_content: &str,
    max_tokens: u32,
    on_delta: &mut F,
) -> Result<String>
where
    F: FnMut(&str) -> Result<()>,
{
    let client = build_openai_client(config)?;

    let request = CreateChatCompletionRequestArgs::default()
        .model(&config.model)
        .messages(vec![
            ChatCompletionRequestSystemMessageArgs::default()
                .content(system_prompt)
                .build()?
                .into(),
            ChatCompletionRequestUserMessageArgs::default()
                .content(user_content)
                .build()?
                .into(),
        ])
        .temperature(0.3f32)
        .max_tokens(max_tokens)
        .build()
        .context("Failed to build streaming chat completion request")?;

    let mut stream = client
        .chat()
        .create_stream(request)
        .await
        .context("Failed to send streaming request to AI provider")?;

    let mut translated = String::new();

    while let Some(result) = stream.next().await {
        match result {
            Ok(response) => {
                for choice in &response.choices {
                    if let Some(ref content) = choice.delta.content {
                        if !content.is_empty() {
                            translated.push_str(content);
                            on_delta(content)?;
                        }
                    }
                }
            }
            Err(e) => {
                error!(target: "ai_provider", error = %e, "stream chunk error");
                anyhow::bail!("AI stream error: {}", e);
            }
        }
    }

    if translated.trim().is_empty() {
        anyhow::bail!("AI returned empty response");
    }

    Ok(translated)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_base_url_strips_chat_completions() {
        assert_eq!(
            normalize_base_url("https://api.openai.com/v1/chat/completions"),
            "https://api.openai.com/v1"
        );
    }

    #[test]
    fn normalize_base_url_adds_v1_for_bare_host() {
        assert_eq!(
            normalize_base_url("http://localhost:11434"),
            "http://localhost:11434/v1"
        );
    }

    #[test]
    fn normalize_base_url_preserves_existing_path() {
        assert_eq!(
            normalize_base_url("https://api.openai.com/v1"),
            "https://api.openai.com/v1"
        );
    }

    #[test]
    fn normalize_base_url_defaults_when_empty() {
        assert_eq!(normalize_base_url(""), "https://api.openai.com/v1");
    }

    #[test]
    fn effective_api_key_ollama_for_local() {
        let config = AiConfig {
            api_format: ApiFormat::Local,
            api_key: String::new(),
            ..AiConfig::default()
        };
        assert_eq!(effective_api_key(&config), "ollama");
    }

    #[test]
    fn effective_api_key_uses_real_key() {
        let config = AiConfig {
            api_format: ApiFormat::Openai,
            api_key: "sk-test".to_string(),
            ..AiConfig::default()
        };
        assert_eq!(effective_api_key(&config), "sk-test");
    }
}
