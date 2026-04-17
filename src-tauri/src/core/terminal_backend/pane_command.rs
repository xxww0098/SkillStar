use std::collections::HashMap;

use crate::core::terminal::config::LayoutNode;

use super::provider_env::extract_env_for_pane;
use super::registry::binary_name_for_agent;

#[derive(Debug, Clone)]
pub(crate) struct PaneCommandSpec {
    pub(crate) binary: String,
    pub(crate) args: Vec<String>,
    pub(crate) env_vars: HashMap<String, String>,
    pub(crate) unset_env_vars: Vec<String>,
}

fn has_explicit_provider(provider_id: Option<&str>) -> bool {
    provider_id.map(str::trim).is_some_and(|id| !id.is_empty())
}

fn claude_forced_model(
    env_vars: &HashMap<String, String>,
    pane_model_id: Option<&str>,
) -> Option<String> {
    env_vars
        .get("CLAUDE_CODE_MODEL")
        .or_else(|| env_vars.get("ANTHROPIC_MODEL"))
        .map(String::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .or_else(|| {
            pane_model_id
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_string)
        })
}

fn claude_conflicting_env_vars_to_unset(env_vars: &HashMap<String, String>) -> Vec<String> {
    let has_auth_token = env_vars
        .get("ANTHROPIC_AUTH_TOKEN")
        .is_some_and(|value| !value.trim().is_empty());
    let has_api_key = env_vars
        .get("ANTHROPIC_API_KEY")
        .is_some_and(|value| !value.trim().is_empty());

    match (has_auth_token, has_api_key) {
        (true, false) => vec!["ANTHROPIC_API_KEY".to_string()],
        (false, true) => vec!["ANTHROPIC_AUTH_TOKEN".to_string()],
        _ => vec![],
    }
}

pub(crate) fn pane_command_spec(pane: &LayoutNode) -> Option<PaneCommandSpec> {
    let (agent_id, safe_mode, extra_args, model_id, provider_id) = match pane {
        LayoutNode::Pane {
            agent_id,
            safe_mode,
            extra_args,
            model_id,
            provider_id,
            ..
        } => (
            agent_id.as_str(),
            *safe_mode,
            extra_args.as_slice(),
            model_id.as_deref(),
            provider_id.as_deref(),
        ),
        _ => return None,
    };

    let binary = binary_name_for_agent(agent_id)
        .unwrap_or(agent_id)
        .to_string();
    let mut args = vec![];

    if safe_mode && agent_id == "claude" {
        args.push("--dangerously-skip-permissions".to_string());
    }

    if let Some(model) = (agent_id == "opencode")
        .then(|| model_id.filter(|model| !model.is_empty()))
        .flatten()
    {
        args.push("--model".to_string());
        args.push(model.to_string());
    }

    args.extend(extra_args.iter().cloned());

    let env_vars = extract_env_for_pane(pane);
    if agent_id == "claude" && has_explicit_provider(provider_id) {
        // Provider-pinned launch should ignore user-level Claude settings
        // to avoid being silently overridden by prior global CLI config.
        args.push("--setting-sources".to_string());
        args.push("project,local".to_string());

        if let Some(model) = claude_forced_model(&env_vars, model_id) {
            args.push("--model".to_string());
            args.push(model);
        }
    }

    let unset_env_vars = if agent_id == "claude" {
        claude_conflicting_env_vars_to_unset(&env_vars)
    } else {
        vec![]
    };

    Some(PaneCommandSpec {
        binary,
        args,
        env_vars,
        unset_env_vars,
    })
}

/// Build the CLI command string for POSIX shells.
pub(crate) fn build_posix_pane_command(pane: &LayoutNode) -> String {
    let Some(spec) = pane_command_spec(pane) else {
        return String::new();
    };

    let mut env_entries: Vec<_> = spec.env_vars.iter().collect();
    env_entries.sort_by(|a, b| a.0.cmp(b.0));
    let env_prefix = env_entries
        .iter()
        .map(|(k, v)| format!("{}={}", k, shell_escape(v)))
        .collect::<Vec<_>>()
        .join(" ");

    let unset_prefix = if spec.unset_env_vars.is_empty() {
        String::new()
    } else {
        format!("unset {};", spec.unset_env_vars.join(" "))
    };

    let mut parts = vec![];
    if !unset_prefix.is_empty() {
        parts.push(unset_prefix);
    }
    if !env_prefix.is_empty() {
        parts.push(env_prefix);
    }
    parts.push(spec.binary);
    parts.extend(spec.args);
    parts.join(" ")
}

pub(crate) fn shell_escape(value: &str) -> String {
    if value.contains(' ') || value.contains('"') || value.contains('\'') || value.contains('$') {
        format!("\"{}\"", value.replace('\\', "\\\\").replace('"', "\\\""))
    } else {
        value.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn has_explicit_provider_treats_empty_as_false() {
        assert!(!has_explicit_provider(None));
        assert!(!has_explicit_provider(Some("")));
        assert!(!has_explicit_provider(Some("   ")));
        assert!(has_explicit_provider(Some("minimax")));
    }

    #[test]
    fn claude_forced_model_prefers_claude_code_model() {
        let mut env = HashMap::new();
        env.insert(
            "CLAUDE_CODE_MODEL".to_string(),
            "MiniMax-M2.7-highspeed".to_string(),
        );
        env.insert("ANTHROPIC_MODEL".to_string(), "ignored".to_string());
        let model = claude_forced_model(&env, Some("fallback-model"));
        assert_eq!(model.as_deref(), Some("MiniMax-M2.7-highspeed"));
    }

    #[test]
    fn claude_forced_model_falls_back_to_pane_model() {
        let env = HashMap::new();
        let model = claude_forced_model(&env, Some("pane-model"));
        assert_eq!(model.as_deref(), Some("pane-model"));
    }

    #[test]
    fn pane_command_spec_adds_setting_sources_for_explicit_claude_provider() {
        let pane = LayoutNode::Pane {
            id: "pane-1".to_string(),
            agent_id: "claude".to_string(),
            provider_id: Some("minimax".to_string()),
            provider_name: Some("MiniMax".to_string()),
            model_id: None,
            safe_mode: false,
            extra_args: vec![],
        };

        let spec = pane_command_spec(&pane).expect("pane command spec");
        let has_setting_sources = spec
            .args
            .windows(2)
            .any(|pair| pair == ["--setting-sources", "project,local"]);

        assert!(has_setting_sources);
    }

    #[test]
    fn pane_command_spec_uses_pane_model_when_provider_is_explicit() {
        let pane = LayoutNode::Pane {
            id: "pane-1".to_string(),
            agent_id: "claude".to_string(),
            provider_id: Some("minimax".to_string()),
            provider_name: Some("MiniMax".to_string()),
            model_id: Some("MiniMax-M2.7-highspeed".to_string()),
            safe_mode: false,
            extra_args: vec![],
        };

        let spec = pane_command_spec(&pane).expect("pane command spec");
        let has_model = spec
            .args
            .windows(2)
            .any(|pair| pair == ["--model", "MiniMax-M2.7-highspeed"]);

        assert!(has_model);
    }
}
