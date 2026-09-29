//! Claude Code sync is a no-op. Unsync and path resolution still live here.
//!
//! Codex, OpenCode, and Pi unsync live in `multi_provider.rs`. None of these
//! modules write a vendor base URL or API key into an Agent config.

use super::*;

/// Resolve the path to Codex's auth.json file.
pub fn resolve_codex_auth_path() -> Result<PathBuf> {
    Ok(codex_home()?.join("auth.json"))
}

/// Resolve the path to Codex's config.toml file.
pub fn resolve_codex_config_path() -> Result<PathBuf> {
    Ok(codex_home()?.join("config.toml"))
}

/// Codex's own home, honouring the upstream `CODEX_HOME` override.
///
/// The CLI namespaces both `auth.json` **and** its keychain entry by this
/// directory (`cli|<sha256(canonical CODEX_HOME)[..16]>`), so resolving the
/// hardcoded `~/.codex` for a user who moved it would write credentials the
/// CLI never reads. The sandbox still wins: tests must never escape into a
/// developer's real Codex home even when `CODEX_HOME` is exported.
fn codex_home() -> Result<PathBuf> {
    if let Some(home) = sandbox_home() {
        return Ok(home.join(".codex"));
    }
    if let Some(dir) = upstream_home_override("CODEX_HOME") {
        return Ok(dir);
    }
    Ok(sync_home_dir()?.join(".codex"))
}

/// Claude Code settings are no longer written from the provider store.
pub fn sync_to_claude_code(
    provider: &Provider,
    model: &str,
    roles: &std::collections::BTreeMap<String, ModelRef>,
) -> Result<ToolSyncResultFlat> {
    let config_path = resolve_tool_config_path("claude-code")?;
    Ok(ToolSyncResultFlat::from_write_outcome_with_drops(
        "claude-code",
        &config_path,
        sync_to_claude_code_inner(provider, model, roles, &config_path)
            .map(|backup| (backup, claude_dropped_roles(provider, roles))),
    ))
}

/// Roles the Claude writer will not put on disk, and why.
///
/// Claude Code is a single-provider agent: its env block names exactly one
/// `ANTHROPIC_BASE_URL`, so a role pointing at a *different* provider cannot be
/// honoured — the model id would be sent to the bound provider's endpoint and
/// fail. v3 wrote the model id anyway (it only ever read the string), producing
/// a config that looks configured and 404s. The role is skipped, and the user is
/// told which one and why instead of being left to discover it at runtime.
fn claude_dropped_roles(
    provider: &Provider,
    roles: &std::collections::BTreeMap<String, ModelRef>,
) -> Vec<DroppedRole> {
    let defs = agent_spec("claude-code").map(|s| s.roles).unwrap_or(&[]);
    let mut dropped = Vec::new();
    for (role, target) in roles {
        if !defs.iter().any(|def| def.id == role.as_str()) {
            dropped.push(DroppedRole::new(role, RoleDropReason::RoleNotSupported));
        } else if target.model.trim().is_empty() {
            // Nothing to write is not a failure — it is how a role is cleared —
            // so an empty *and* unset role is silent. A role with a provider but
            // no model is a half-filled row worth flagging.
            if !target.provider_id.trim().is_empty() {
                dropped.push(DroppedRole::new(role, RoleDropReason::NoModel));
            }
        } else if !target.provider_id.trim().is_empty() && target.provider_id != provider.id {
            dropped.push(DroppedRole::for_provider(
                role,
                RoleDropReason::ProviderNotBound,
                &target.provider_id,
            ));
        }
    }
    dropped
}

/// Claude Code settings are no longer written from the provider store.
pub(crate) fn sync_to_claude_code_inner(
    provider: &Provider,
    model: &str,
    roles: &std::collections::BTreeMap<String, ModelRef>,
    config_path: &Path,
) -> Result<Option<PathBuf>> {
    let _ = (provider, model, roles, config_path);
    Ok(None)
}

/// Remove SkillStar-managed Claude env keys (unsync).
fn clear_claude_managed_env_at(config_path: &Path) -> Result<Option<PathBuf>> {
    if !config_path.exists() {
        return Ok(None);
    }

    let backup_path = Some(create_rolling_backup(config_path)?);

    let content = std::fs::read_to_string(config_path)
        .with_context(|| format!("Failed to read {}", config_path.display()))?;
    let mut json: Value = serde_json::from_str(&content)
        .with_context(|| format!("Failed to parse JSON in {}", config_path.display()))?;

    if let Some(env_obj) = json.get_mut("env").and_then(|v| v.as_object_mut()) {
        for key in claude_managed_env_keys() {
            env_obj.remove(key);
        }
        if env_obj.is_empty()
            && let Some(root_obj) = json.as_object_mut()
        {
            root_obj.remove("env");
        }
    }

    let output =
        serde_json::to_string_pretty(&json).context("Failed to serialize Claude Code config")?;
    skillstar_core::infra::fs_ops::atomic_write(config_path, output.as_bytes())
        .with_context(|| format!("Failed to write {}", config_path.display()))?;

    Ok(backup_path)
}

/// Registry adapter: Claude Code sync resolves the active entry and writes nothing.
pub(crate) fn sync_claude_code_binding(
    binding: &AgentBinding,
    providers: &[Provider],
) -> Result<ToolSyncResultFlat> {
    let (provider, model) = resolve_single_active(binding, providers)?;
    sync_to_claude_code(provider, model, &binding.roles)
}

/// Claude Desktop's marker file is no longer written from the provider store.
pub(crate) fn sync_claude_desktop_binding(
    binding: &AgentBinding,
    providers: &[Provider],
) -> Result<ToolSyncResultFlat> {
    let path = resolve_claude_desktop_binding_path()?;
    Ok(ToolSyncResultFlat::from_write_outcome(
        "claude-desktop",
        &path,
        sync_claude_desktop_binding_inner(binding, providers, &path),
    ))
}

pub(crate) fn sync_claude_desktop_binding_inner(
    binding: &AgentBinding,
    providers: &[Provider],
    path: &Path,
) -> Result<Option<PathBuf>> {
    let _ = (binding, providers, path);
    Ok(None)
}

/// Remove the Claude Desktop SkillStar binding marker (deactivation).
pub fn unsync_claude_desktop() -> Result<()> {
    let path = resolve_claude_desktop_binding_path()?;
    if path.exists() {
        let _ = create_rolling_backup(&path)?;
        std::fs::remove_file(&path)
            .with_context(|| format!("Failed to remove {}", path.display()))?;
    }
    Ok(())
}

/// Marker-file detect: report bound provider name when the marker exists.
pub(crate) fn detect_claude_desktop_provider(path: &Path) -> Result<Option<String>> {
    if !path.exists() {
        return Ok(None);
    }
    let content = std::fs::read_to_string(path)
        .with_context(|| format!("Failed to read {}", path.display()))?;
    let value: serde_json::Value = serde_json::from_str(&content).unwrap_or_default();
    Ok(value
        .get("provider_name")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string()))
}

/// Resolve the active entry of a single-provider binding to `(provider, model)`.
fn resolve_single_active<'a>(
    binding: &'a AgentBinding,
    providers: &'a [Provider],
) -> Result<(&'a Provider, &'a str)> {
    let active = binding.active().context("no active entry")?;
    let provider = providers
        .iter()
        .find(|p| p.id == active.provider_id)
        .with_context(|| format!("Provider '{}' not found", active.provider_id))?;
    Ok((provider, active.model.as_str()))
}

/// Remove every SkillStar-managed field/entry from a tool's config files.
///
/// Registry-driven deactivation dispatch: known agents route to their
/// [`AgentSpec::unsync`] column; ids missing from the registry are a no-op
/// (nothing was ever written for them).
pub fn unsync_tool(tool_id: &str) -> Result<()> {
    match agent_spec(tool_id) {
        Some(spec) => (spec.unsync)(),
        None => Ok(()),
    }
}

/// Remove managed fields from Claude Code's config (deactivation).
///
/// Removes `ANTHROPIC_BASE_URL`, `ANTHROPIC_AUTH_TOKEN`, and `ANTHROPIC_MODEL`
/// from the `env` block in `~/.claude/settings.json`.
/// Preserves all other user-added fields in the env block and top-level.
pub fn unsync_claude_code() -> Result<()> {
    let config_path = resolve_tool_config_path("claude-code")?;
    let _ = clear_claude_managed_env_at(&config_path)?;
    Ok(())
}
