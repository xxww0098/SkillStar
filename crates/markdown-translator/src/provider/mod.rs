use crate::error::Result;
use crate::types::ApiUsage;

/// Abstract LLM provider interface.
///
/// Uses `async_trait` because the trait needs to be object-safe for `Box<dyn LlmProvider>`.
#[async_trait::async_trait]
pub trait LlmProvider: Send + Sync {
    /// Send a chat completion request that returns structured JSON.
    async fn chat_json(
        &self,
        system_prompt: &str,
        user_prompt: &str,
        call_label: &str,
    ) -> Result<serde_json::Value>;

    /// Send a chat completion request that returns plain text.
    /// Used when the response format is text with integrity markers, not JSON.
    async fn chat_text(
        &self,
        system_prompt: &str,
        user_prompt: &str,
        call_label: &str,
    ) -> Result<String>;

    /// Stream a chat completion request that yields structured JSON chunks.
    /// Implementations that don't support streaming should return an error.
    /// The yielded values represent partial JSON objects that can be merged.
    async fn chat_json_streaming(
        &self,
        _system_prompt: &str,
        _user_prompt: &str,
        _call_label: &str,
    ) -> Result<Box<dyn futures::Stream<Item = Result<serde_json::Value>> + Send + Sync + '_>> {
        Err(crate::error::Error::Pipeline(
            "Streaming not supported by this provider".into(),
        ))
    }

    /// Get a snapshot of cumulative API usage.
    fn usage(&self) -> &ApiUsage;
}

pub mod openai;
