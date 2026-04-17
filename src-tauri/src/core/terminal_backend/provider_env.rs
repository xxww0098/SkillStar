use std::collections::HashMap;

use crate::core::terminal::config::LayoutNode;

fn non_empty_claude_auth(env: &HashMap<String, String>, key: &str) -> Option<String> {
    env.get(key)
        .cloned()
        .filter(|value| !value.trim().is_empty())
}

fn has_non_empty_claude_base_url(env: &HashMap<String, String>) -> bool {
    env.get("ANTHROPIC_BASE_URL")
        .is_some_and(|value| !value.trim().is_empty())
}

pub(crate) fn normalize_claude_model_env(env: &mut HashMap<String, String>) {
    let anthropic_model = env
        .get("ANTHROPIC_MODEL")
        .map(String::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty());

    let claude_code_model = env
        .get("CLAUDE_CODE_MODEL")
        .map(String::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty());

    if let Some(model) = claude_code_model
        .is_none()
        .then_some(anthropic_model)
        .flatten()
    {
        env.insert("CLAUDE_CODE_MODEL".to_string(), model.to_string());
    }
}

/// Claude auth env keys must be mutually exclusive.
///
/// For custom Anthropic-compatible gateways (`ANTHROPIC_BASE_URL` set),
/// prefer `ANTHROPIC_AUTH_TOKEN` to avoid interactive API-key approval flows
/// and keep launch behavior stable across sessions.
pub(crate) fn normalize_claude_auth_keys(env: &mut HashMap<String, String>) {
    let auth_token = non_empty_claude_auth(env, "ANTHROPIC_AUTH_TOKEN");
    let api_key = non_empty_claude_auth(env, "ANTHROPIC_API_KEY");
    let has_custom_base_url = has_non_empty_claude_base_url(env);

    if has_custom_base_url {
        match (auth_token, api_key) {
            (Some(token), _) => {
                env.insert("ANTHROPIC_AUTH_TOKEN".to_string(), token);
                env.remove("ANTHROPIC_API_KEY");
            }
            (None, Some(key)) => {
                // Convert gateway API key mode to auth-token mode for Claude CLI.
                env.insert("ANTHROPIC_AUTH_TOKEN".to_string(), key);
                env.remove("ANTHROPIC_API_KEY");
            }
            (None, None) => {
                env.remove("ANTHROPIC_AUTH_TOKEN");
                env.remove("ANTHROPIC_API_KEY");
            }
        }
        return;
    }

    match (auth_token, api_key) {
        (Some(_token), Some(key)) => {
            env.insert("ANTHROPIC_API_KEY".to_string(), key);
            env.remove("ANTHROPIC_AUTH_TOKEN");
        }
        (Some(token), None) => {
            env.insert("ANTHROPIC_AUTH_TOKEN".to_string(), token);
            env.remove("ANTHROPIC_API_KEY");
        }
        (None, Some(key)) => {
            env.insert("ANTHROPIC_API_KEY".to_string(), key);
            env.remove("ANTHROPIC_AUTH_TOKEN");
        }
        (None, None) => {
            env.remove("ANTHROPIC_AUTH_TOKEN");
            env.remove("ANTHROPIC_API_KEY");
        }
    }
}

/// Extract environment variables for a pane's agent from the model provider store.
pub(crate) fn extract_env_for_pane(pane: &LayoutNode) -> HashMap<String, String> {
    let (agent_id, provider_id) = match pane {
        LayoutNode::Pane {
            agent_id,
            provider_id,
            ..
        } => (agent_id.as_str(), provider_id.as_deref()),
        _ => return HashMap::new(),
    };

    if agent_id.is_empty() {
        return HashMap::new();
    }

    let store = crate::core::model_config::providers::read_store().unwrap_or_default();
    let provider_id = provider_id.unwrap_or("");

    let app_providers = match agent_id {
        "claude" => &store.claude,
        "codex" => &store.codex,
        _ => return HashMap::new(),
    };

    let provider = if provider_id.is_empty() {
        app_providers
            .current
            .as_deref()
            .and_then(|id| app_providers.providers.get(id))
    } else {
        app_providers.providers.get(provider_id)
    };

    let Some(provider) = provider else {
        return HashMap::new();
    };

    let mut env = HashMap::new();
    if let Some(env_obj) = provider
        .settings_config
        .get("env")
        .and_then(|v| v.as_object())
    {
        for (key, value) in env_obj {
            if let Some(val) = value.as_str() {
                env.insert(key.clone(), val.to_string());
            }
        }
    }

    if agent_id == "claude" {
        normalize_claude_auth_keys(&mut env);
        normalize_claude_model_env(&mut env);
    }

    env
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_claude_auth_keys_custom_base_prefers_auth_token() {
        let mut env = HashMap::new();
        env.insert(
            "ANTHROPIC_BASE_URL".to_string(),
            "https://api.minimaxi.com/anthropic".to_string(),
        );
        env.insert("ANTHROPIC_API_KEY".to_string(), "key-only".to_string());

        normalize_claude_auth_keys(&mut env);

        assert_eq!(
            env.get("ANTHROPIC_AUTH_TOKEN").map(String::as_str),
            Some("key-only")
        );
        assert!(!env.contains_key("ANTHROPIC_API_KEY"));
    }

    #[test]
    fn normalize_claude_auth_keys_without_custom_base_keeps_api_key_mode() {
        let mut env = HashMap::new();
        env.insert("ANTHROPIC_AUTH_TOKEN".to_string(), "token".to_string());
        env.insert("ANTHROPIC_API_KEY".to_string(), "key".to_string());

        normalize_claude_auth_keys(&mut env);

        assert_eq!(
            env.get("ANTHROPIC_API_KEY").map(String::as_str),
            Some("key")
        );
        assert!(!env.contains_key("ANTHROPIC_AUTH_TOKEN"));
    }
}
