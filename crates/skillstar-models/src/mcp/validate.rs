//! Input validation for servers the user is creating or editing.
//!
//! ## Why this is separate from `validate_entry`
//!
//! `store::validate_entry` runs on **every** projection, including
//! re-projecting servers a user saved years ago (`sync_all`, `sync_server_by_id`).
//! Tightening it would turn a stricter rule into a retroactive verdict: a name
//! or header that was accepted once, was written verbatim into seven tool
//! configs, and is still sitting in those files would suddenly fail to sync —
//! and because removal is keyed on the stored name, the entry could no longer
//! be cleaned up either. Every existing key would become an orphan.
//!
//! So the split is deliberate (audit R3):
//!
//! - [`super::validate_entry`] — the *invariant* every stored entry must hold
//!   (transport is known, the transport's required field is present). Runs on
//!   read and sync paths. Unchanged.
//! - [`validate_entry_input`] — the *policy* for values entering the store.
//!   Runs on create and update only. Legacy rows never pass through it, so
//!   they keep syncing and stay removable.
//!
//! The practical effect: new servers cannot be given a name that breaks a
//! config file or a header that enables response splitting, while existing
//! servers are left exactly as the user has them.

use anyhow::{Result, bail};

use super::*;

/// Longest server name accepted for a new entry.
///
/// The name becomes a JSON/TOML object key in a dozen files. There is no
/// documented client limit; this is a sanity bound that keeps a pasted blob
/// from becoming a config key.
const MAX_NAME_LEN: usize = 128;

/// Server names Claude Code reserves for itself. Defining one is not an error
/// the user finds out about — Claude Code skips the entry with a warning
/// buried in its logs, so the server simply never appears (research §5.3 #4).
const CLAUDE_CODE_RESERVED_NAMES: &[&str] = &[
    "workspace",
    "claude-in-chrome",
    "computer-use",
    "claude preview",
    "claude browser",
];

/// Validate a server the user is about to create or save.
///
/// Runs [`super::validate_entry`] first (the shared invariant), then the
/// input-only policy: name shape, URL shape, env keys, and header names.
pub fn validate_entry_input(entry: &McpServerEntry) -> Result<()> {
    validate_entry_edit(entry, None)
}

/// Validate an edit, given the name the entry had before it.
///
/// Identical to [`validate_entry_input`] except that the **name** rule is
/// skipped when the name is unchanged. A server saved by an older release may
/// hold a name today's rule rejects; refusing to save any edit to it would
/// strand the entry — the user could not even correct the thing being
/// complained about without first making some other change. So an unchanged
/// legacy name is grandfathered, while renaming to another invalid name is
/// still refused. Every other rule applies either way, since none of them can
/// orphan a key in a tool's config.
pub fn validate_entry_edit(entry: &McpServerEntry, previous_name: Option<&str>) -> Result<()> {
    validate_entry(entry)?;
    if previous_name != Some(entry.name.as_str()) {
        validate_server_name(&entry.name)?;
    }
    if matches!(entry.transport.as_str(), "http" | "sse")
        && let Some(url) = entry.url.as_deref()
    {
        validate_server_url(url)
            .map_err(|e| anyhow::anyhow!("MCP server '{}' has an invalid url: {e}", entry.name))?;
    }
    for key in entry.env.keys() {
        validate_env_key(key).map_err(|e| {
            anyhow::anyhow!("MCP server '{}' has an invalid env key: {e}", entry.name)
        })?;
    }
    for (name, value) in &entry.headers {
        validate_header_name(name).map_err(|e| {
            anyhow::anyhow!("MCP server '{}' has an invalid header: {e}", entry.name)
        })?;
        validate_header_value(name, value).map_err(|e| {
            anyhow::anyhow!("MCP server '{}' has an invalid header: {e}", entry.name)
        })?;
    }
    Ok(())
}

/// A server name has to survive being written verbatim as an object key into
/// every target's config, and being matched back out again on removal.
///
/// The accepted set is the intersection that is safe everywhere: ASCII
/// alphanumerics plus `_ - .`. That is exactly what the marketplace's
/// `sanitize_key` already produces, so a draft installed from the store always
/// passes; the rule only ever bites hand-typed names.
pub fn validate_server_name(name: &str) -> Result<()> {
    if name.is_empty() {
        bail!("MCP server name must not be empty");
    }
    // Leading/trailing whitespace is the classic paste artifact. Claude Code
    // warns about it and then uses the value as-is, which produces a key the
    // user cannot match by eye (research §5.3 #5).
    if name.trim() != name {
        bail!("MCP server name '{name}' has leading or trailing whitespace");
    }
    if name.chars().count() > MAX_NAME_LEN {
        bail!("MCP server name is too long (max {MAX_NAME_LEN} characters)");
    }
    if let Some(bad) = name
        .chars()
        .find(|c| !(c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.')))
    {
        bail!(
            "MCP server name '{name}' contains '{bad}'. Names are written verbatim as config keys, so use only letters, digits, '_', '-' and '.'"
        );
    }
    if CLAUDE_CODE_RESERVED_NAMES.contains(&name.to_ascii_lowercase().as_str()) {
        bail!("'{name}' is reserved by Claude Code and would be skipped — pick another name");
    }
    Ok(())
}

/// A remote endpoint must be an absolute `http`/`https` URL with a host.
///
/// `ws`/`wss` are rejected on purpose: the store's transport vocabulary is
/// `stdio | http | sse`, so a websocket URL here would be written out under an
/// HTTP `type` and fail at connect time with a far less obvious message.
pub fn validate_server_url(url: &str) -> Result<()> {
    if url.trim() != url {
        bail!("'{url}' has leading or trailing whitespace");
    }
    let parsed = url::Url::parse(url).map_err(|e| anyhow::anyhow!("'{url}' is not a URL ({e})"))?;
    match parsed.scheme() {
        "http" | "https" => {}
        other => bail!("'{url}' uses the '{other}' scheme (expected http or https)"),
    }
    if parsed.host_str().is_none_or(str::is_empty) {
        bail!("'{url}' has no host");
    }
    Ok(())
}

/// Environment variable names follow the portable POSIX shape: a letter or
/// underscore, then letters, digits, or underscores.
///
/// A key containing `=` or whitespace cannot round-trip through a process
/// environment at all, and several targets serialize `env` into TOML where a
/// stray quote or newline would corrupt the file.
pub fn validate_env_key(key: &str) -> Result<()> {
    if key.is_empty() {
        bail!("environment variable names must not be empty");
    }
    let mut chars = key.chars();
    let first = chars.next().unwrap_or_default();
    if !(first.is_ascii_alphabetic() || first == '_') {
        bail!("'{key}' must start with a letter or '_'");
    }
    if let Some(bad) = chars.find(|c| !(c.is_ascii_alphanumeric() || *c == '_')) {
        bail!("'{key}' contains '{bad}' (use letters, digits and '_' only)");
    }
    Ok(())
}

/// Header names must be RFC 7230 tokens.
pub fn validate_header_name(name: &str) -> Result<()> {
    if name.is_empty() {
        bail!("header names must not be empty");
    }
    if let Some(bad) = name.chars().find(|c| !is_header_token_char(*c)) {
        bail!("header name '{name}' contains '{bad}', which is not allowed in a header name");
    }
    Ok(())
}

/// Header values may not carry CR or LF.
///
/// This is the one rule here that is a security rule rather than a
/// well-formedness rule: a newline inside a header value is header injection,
/// and these values are written into config files that other agents then send
/// as real HTTP headers.
pub fn validate_header_value(name: &str, value: &str) -> Result<()> {
    if value.contains(['\r', '\n']) {
        bail!("header '{name}' has a value containing a line break");
    }
    Ok(())
}

/// RFC 7230 `tchar`.
fn is_header_token_char(c: char) -> bool {
    c.is_ascii_alphanumeric()
        || matches!(
            c,
            '!' | '#'
                | '$'
                | '%'
                | '&'
                | '\''
                | '*'
                | '+'
                | '-'
                | '.'
                | '^'
                | '_'
                | '`'
                | '|'
                | '~'
        )
}
