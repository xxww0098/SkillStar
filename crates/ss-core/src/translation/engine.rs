//! Machine translation (Google) and OpenAI-compatible LLM translation.
//!
//! Neither path is called from a unit test. Parsers are.

use std::time::Duration;

use anyhow::{Context, Result, anyhow};

use super::config::{Engine, LlmAuth, TranslationConfig, translation_language};

const CHUNK: usize = 1400;

pub fn parse_gtx(body: &str) -> Option<String> {
    let value: serde_json::Value = serde_json::from_str(body).ok()?;
    let segments = value.get(0)?.as_array()?;
    let mut out = String::new();
    for segment in segments {
        let piece = segment.get(0)?.as_str()?;
        out.push_str(piece);
    }
    let trimmed = out.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

pub fn parse_chat(body: &str) -> Option<String> {
    let value: serde_json::Value = serde_json::from_str(body).ok()?;
    let text = value
        .pointer("/choices/0/message/content")?
        .as_str()?
        .trim();
    if text.is_empty() {
        None
    } else {
        Some(text.to_string())
    }
}

pub async fn translate_text(
    source: &str,
    target: &str,
    config: &TranslationConfig,
    auth: &LlmAuth,
) -> Result<String> {
    match config.engine {
        Engine::Machine => translate_machine(source, target).await,
        Engine::Llm => translate_llm(source, target, config, auth).await,
    }
}

async fn translate_machine(source: &str, target: &str) -> Result<String> {
    let mut out = String::new();
    for chunk in chunks(source) {
        let piece = gtx_once(&chunk, target).await?;
        out.push_str(&piece);
    }
    Ok(out)
}

async fn gtx_once(source: &str, target: &str) -> Result<String> {
    let client = crate::infra::http_client::probe_http_client(Duration::from_secs(20))?;
    let response = client
        .get("https://translate.googleapis.com/translate_a/single")
        .header("User-Agent", "SkillStar")
        .query(&[
            ("client", "gtx"),
            ("sl", "auto"),
            ("tl", target),
            ("dt", "t"),
            ("q", source),
        ])
        .send()
        .await
        .context("machine translation request failed")?;
    let status = response.status();
    let body = response.text().await.context("machine translation body")?;
    if !status.is_success() {
        anyhow::bail!("machine translation returned {status}");
    }
    parse_gtx(&body).ok_or_else(|| anyhow!("machine translation response was empty"))
}

async fn translate_llm(
    source: &str,
    target: &str,
    config: &TranslationConfig,
    auth: &LlmAuth,
) -> Result<String> {
    let key = auth.api_key.trim().to_string();
    if key.is_empty() {
        anyhow::bail!("LLM API key is empty");
    }
    let model = config.llm_model.trim();
    if model.is_empty() {
        anyhow::bail!("LLM model is empty");
    }
    let mut out = String::new();
    for chunk in chunks(source) {
        let piece = llm_once(&chunk, target, auth, config, model, &key).await?;
        if !out.is_empty() && !out.ends_with('\n') && !piece.starts_with('\n') {
            out.push('\n');
        }
        out.push_str(&piece);
    }
    Ok(out)
}

async fn llm_once(
    source: &str,
    target: &str,
    auth: &LlmAuth,
    config: &TranslationConfig,
    model: &str,
    key: &str,
) -> Result<String> {
    let base = if auth.base_url.trim().is_empty() {
        config.llm_base()
    } else {
        auth.base_url.trim().trim_end_matches('/').to_string()
    };
    let url = format!("{base}/chat/completions");
    let into = translation_language(target)
        .map(|language| language.prompt)
        .unwrap_or(target);
    let body = serde_json::json!({
        "model": model,
        "temperature": 0,
        "messages": [
            {
                "role": "system",
                "content": format!(
                    "You are a professional translator. Translate the user text into {into}. Return only the translation."
                )
            },
            { "role": "user", "content": source }
        ]
    });
    let client = crate::infra::http_client::probe_http_client(Duration::from_secs(60))?;
    let mut request = client.post(url).bearer_auth(key);
    if let Some((name, value)) = opencode_session(&base) {
        request = request.header(name, value);
    }
    let response = request
        .json(&body)
        .send()
        .await
        .context("LLM translation request failed")?;
    let status = response.status();
    let text = response.text().await.context("LLM translation body")?;
    if !status.is_success() {
        anyhow::bail!("LLM translation returned {status}");
    }
    parse_chat(&text).ok_or_else(|| anyhow!("LLM translation response was empty"))
}

/// OpenCode Go rejects chat completions that omit this header.
fn opencode_session<'a>(base: &str) -> Option<(&'a str, &'a str)> {
    base.contains("opencode.ai")
        .then_some(("x-opencode-session", "skillstar-translate"))
}

fn chunks(text: &str) -> Vec<String> {
    if text.chars().count() <= CHUNK {
        return vec![text.to_string()];
    }
    let mut out = Vec::new();
    let mut buf = String::new();
    let mut count = 0usize;
    for ch in text.chars() {
        buf.push(ch);
        count += 1;
        if count >= CHUNK && ch.is_whitespace() {
            out.push(std::mem::take(&mut buf));
            count = 0;
        }
    }
    if !buf.is_empty() {
        out.push(buf);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::{opencode_session, parse_chat, parse_gtx};

    #[test]
    fn gtx_joins_segments() {
        let body = r#"[[["你好","Hello",null,null,1],["世界"," world",null,null,1]],null,"en"]"#;
        assert_eq!(parse_gtx(body).as_deref(), Some("你好世界"));
    }

    #[test]
    fn opencode_go_sends_a_session_header() {
        assert_eq!(
            opencode_session("https://opencode.ai/zen/go/v1"),
            Some(("x-opencode-session", "skillstar-translate"))
        );
        assert!(opencode_session("https://ollama.com/v1").is_none());
        assert!(opencode_session("https://api.commandcode.ai/provider/v1").is_none());
    }

    #[test]
    fn chat_reads_the_message() {
        let body = r#"{"choices":[{"message":{"content":" 你好 "}}]}"#;
        assert_eq!(parse_chat(body).as_deref(), Some("你好"));
        assert!(parse_chat(r#"{"choices":[]}"#).is_none());
    }
}
