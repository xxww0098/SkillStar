use crate::error::{McpHubError, McpHubResult};
use crate::model::{McpServer, Transport};
use regex::Regex;
use std::sync::LazyLock;

static ENV_DOLLAR_BRACE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\$\{(?:env:)?([A-Za-z_][A-Za-z0-9_]*)(?::-([^}]*))?\}").unwrap());
static ENV_BRACE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\{env:([A-Za-z_][A-Za-z0-9_]*)\}").unwrap());
static SAFE_JS_IDENT: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^[A-Za-z_][A-Za-z0-9_]*$").unwrap());

/// Env reference syntax targeted by an agent config format.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EnvRefStyle {
    /// `${VAR}` (Claude, Codex `env_vars`, etc.)
    DollarBrace,
    /// `${env:VAR}` (Cursor)
    DollarEnvColon,
    /// `{env:VAR}` (OpenCode / DevEco / MiMo Code)
    BraceEnvColon,
    /// Plain env values (`no-env-ref`: Antigravity, Kimi, Alma, Devin, Cline, Zed, LiveAgent)
    Plain,
}

/// Detect whether `input` is strictly a single env var reference, returning its variable name.
pub fn single_env_var_ref(input: &str) -> Option<String> {
    let trimmed = input.trim();
    if let Some(cap) = ENV_DOLLAR_BRACE.captures(trimmed)
        && cap.get(0).map(|m| m.as_str()) == Some(trimmed)
    {
        return Some(cap[1].to_string());
    }
    if let Some(cap) = ENV_BRACE.captures(trimmed)
        && cap.get(0).map(|m| m.as_str()) == Some(trimmed)
    {
        return Some(cap[1].to_string());
    }
    None
}

/// Normalize env references when converting back into a canonical server or rendering a style.
pub fn render_env_syntax(input: &str, style: EnvRefStyle) -> String {
    match style {
        EnvRefStyle::Plain => {
            // Convert Cursor/OpenCode syntax to `${VAR}` if not expanding, or resolve env if set.
            let step1 = ENV_BRACE.replace_all(input, "$${$1}");
            let step2 = ENV_DOLLAR_BRACE.replace_all(&step1, |caps: &regex::Captures<'_>| {
                let name = &caps[1];
                if let Ok(val) = std::env::var(name)
                    && !val.is_empty()
                {
                    return val;
                }
                if let Some(default) = caps.get(2) {
                    return default.as_str().to_string();
                }
                format!("${{{name}}}")
            });
            step2.into_owned()
        }
        EnvRefStyle::DollarBrace => {
            let step1 = ENV_BRACE.replace_all(input, "$${$1}");
            let step2 = ENV_DOLLAR_BRACE.replace_all(&step1, |caps: &regex::Captures<'_>| {
                if let Some(default) = caps.get(2) {
                    format!("${{{}:-{}}}", &caps[1], default.as_str())
                } else {
                    format!("${{{}}}", &caps[1])
                }
            });
            step2.into_owned()
        }
        EnvRefStyle::DollarEnvColon => {
            let step1 = ENV_BRACE.replace_all(input, "$${env:$1}");
            let step2 = ENV_DOLLAR_BRACE.replace_all(&step1, "$${env:$1}");
            step2.into_owned()
        }
        EnvRefStyle::BraceEnvColon => {
            let step1 = ENV_DOLLAR_BRACE.replace_all(input, "{env:$1}");
            let step2 = ENV_BRACE.replace_all(&step1, "{env:$1}");
            step2.into_owned()
        }
    }
}

/// Normalize an imported string back to canonical `${VAR}` style.
pub fn normalize_imported_env_syntax(input: &str) -> String {
    let step1 = ENV_BRACE.replace_all(input, "$${$1}");
    let step2 = ENV_DOLLAR_BRACE.replace_all(&step1, |caps: &regex::Captures<'_>| {
        if let Some(default) = caps.get(2) {
            format!("${{{}:-{}}}", &caps[1], default.as_str())
        } else {
            format!("${{{}}}", &caps[1])
        }
    });
    step2.into_owned()
}

/// Expand env var references using `custom_env` first, falling back to OS process env.
pub fn expand_value_for_probe(
    input: &str,
    custom_env: &std::collections::BTreeMap<String, String>,
) -> String {
    let normalized = normalize_imported_env_syntax(input);
    ENV_DOLLAR_BRACE
        .replace_all(&normalized, |caps: &regex::Captures<'_>| {
            let name = &caps[1];
            if let Some(val) = custom_env.get(name) {
                return val.clone();
            }
            if let Ok(val) = std::env::var(name) {
                return val;
            }
            caps.get(2)
                .map(|m| m.as_str().to_string())
                .unwrap_or_default()
        })
        .into_owned()
}

/// Format a DSH `!!js` expression string referencing `process.env.<NAME>` when applicable.
pub fn to_dsh_env_expr(value: &str) -> Option<String> {
    let var_name = single_env_var_ref(value)?;
    if SAFE_JS_IDENT.is_match(&var_name) {
        Some(format!("process.env.{var_name}"))
    } else {
        None
    }
}

/// Validate server name and transport-required fields before saving to library or syncing.
pub fn validate_server(server: &McpServer) -> McpHubResult<()> {
    let name = server.name.trim();
    if name.is_empty() {
        return Err(McpHubError::InvalidServer(
            "server name cannot be empty".to_string(),
        ));
    }
    if name.contains('/') || name.contains('\\') || name == "." || name == ".." {
        return Err(McpHubError::InvalidServer(format!(
            "server name contains disallowed path characters: '{name}'"
        )));
    }
    match server.transport {
        Transport::Stdio => {
            if server.command.trim().is_empty() {
                return Err(McpHubError::InvalidServer(format!(
                    "stdio server '{name}' requires a command"
                )));
            }
        }
        Transport::Http | Transport::Sse => {
            let url = server.url.trim();
            if url.is_empty() {
                return Err(McpHubError::InvalidServer(format!(
                    "remote server '{name}' requires a url"
                )));
            }
            let parsed = url::Url::parse(url).map_err(|err| {
                McpHubError::InvalidServer(format!("invalid url for '{name}': {err}"))
            })?;
            if !matches!(parsed.scheme(), "http" | "https") {
                return Err(McpHubError::InvalidServer(format!(
                    "url for '{name}' must use http or https scheme"
                )));
            }
        }
    }
    if let Some(timeout) = server.startup_timeout_sec
        && (!timeout.is_finite() || timeout <= 0.0)
    {
        return Err(McpHubError::InvalidServer(format!(
            "startup_timeout_sec for '{name}' must be a positive finite number"
        )));
    }
    Ok(())
}

/// Strip leading/trailing whitespace on server fields without erasing intentional args/env values.
pub fn normalize_server(mut server: McpServer) -> McpServer {
    server.name = server.name.trim().to_string();
    server.command = server.command.trim().to_string();
    server.url = server.url.trim().to_string();
    server.agents.sort();
    server.agents.dedup();
    server.disabled_tools.retain(|t| !t.trim().is_empty());
    server.disabled_tools.sort();
    server.disabled_tools.dedup();
    server
}
