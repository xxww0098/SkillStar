//! Model ids for an OpenAI-compatible account.
//!
//! The shell asks for this list only when the user fetches it. The parser
//! accepts the OpenAI `{data:[{id}]}` catalog and Ollama's `{models:[{name}]}`.

use std::time::Duration;

use anyhow::Result;
use thiserror::Error;

const MODEL_LIST_CAP: usize = 400;

#[derive(Debug, Error)]
pub enum ModelListError {
    #[error("translation model list has no api key")]
    MissingKey,
    #[error("translation model list request failed")]
    Request,
    #[error("translation model list returned {0}")]
    Status(u16),
    #[error("translation model list was empty")]
    Empty,
}

/// `GET {base}/models` with the account bearer token.
pub async fn list_models(base_url: &str, api_key: &str) -> Result<Vec<String>> {
    let key = api_key.trim();
    if key.is_empty() {
        return Err(ModelListError::MissingKey.into());
    }
    let base = base_url.trim().trim_end_matches('/');
    if base.is_empty() {
        return Err(ModelListError::Request.into());
    }
    let client = crate::infra::http_client::probe_http_client(Duration::from_secs(20))?;
    let response = client
        .get(format!("{base}/models"))
        .bearer_auth(key)
        .header("User-Agent", "SkillStar")
        .send()
        .await
        .map_err(|error| {
            tracing::warn!("translation model list failed: {error}");
            ModelListError::Request
        })?;
    let status = response.status();
    let body = response.text().await.map_err(|error| {
        tracing::warn!("translation model list body failed: {error}");
        ModelListError::Request
    })?;
    if !status.is_success() {
        return Err(ModelListError::Status(status.as_u16()).into());
    }
    let models = parse_model_ids(&body);
    if models.is_empty() {
        return Err(ModelListError::Empty.into());
    }
    Ok(models)
}

fn parse_model_ids(body: &str) -> Vec<String> {
    let Ok(value) = serde_json::from_str::<serde_json::Value>(body) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    if let Some(items) = value.get("data").and_then(|item| item.as_array()) {
        push_ids(&mut out, items);
    }
    if out.is_empty()
        && let Some(items) = value.get("models").and_then(|item| item.as_array())
    {
        push_ids(&mut out, items);
    }
    out
}

fn push_ids(out: &mut Vec<String>, items: &[serde_json::Value]) {
    for item in items {
        if out.len() >= MODEL_LIST_CAP {
            break;
        }
        let Some(id) = item_id(item) else {
            continue;
        };
        let id = id.trim();
        if id.is_empty() || out.iter().any(|seen| seen == id) {
            continue;
        }
        out.push(id.to_string());
    }
}

fn item_id(item: &serde_json::Value) -> Option<&str> {
    item.get("id")
        .and_then(|value| value.as_str())
        .or_else(|| item.get("name").and_then(|value| value.as_str()))
        .or_else(|| item.get("model").and_then(|value| value.as_str()))
}

#[cfg(test)]
mod tests {
    use super::parse_model_ids;

    #[test]
    fn openai_catalog_trims_and_drops_duplicates() {
        let body = r#"{"data":[
            {"id":"deepseek-v4.1-flash"},
            {"id":" kimi-k2.6 "},
            {"id":"deepseek-v4.1-flash"},
            {"id":""}
        ]}"#;
        assert_eq!(
            parse_model_ids(body),
            vec!["deepseek-v4.1-flash", "kimi-k2.6"]
        );
    }

    #[test]
    fn ollama_names_are_used_when_data_is_missing() {
        let body = r#"{"models":[{"name":"gemma4:31b"},{"model":"qwen3"}]}"#;
        assert_eq!(parse_model_ids(body), vec!["gemma4:31b", "qwen3"]);
    }

    #[test]
    fn empty_data_falls_through_to_models() {
        let body = r#"{"data":[],"models":[{"id":"deepseek/deepseek-v4-flash"}]}"#;
        assert_eq!(
            parse_model_ids(body),
            vec!["deepseek/deepseek-v4-flash".to_string()]
        );
    }

    #[test]
    fn garbage_is_an_empty_list() {
        assert!(parse_model_ids("not-json").is_empty());
        assert!(parse_model_ids(r#"{"data":[{"object":"model"}]}"#).is_empty());
    }
}
