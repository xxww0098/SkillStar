//! Oh My Pi (OMP) multi-provider tool sync — YAML config files.
//!
//! OMP (`@oh-my-pi/pi-coding-agent`) keeps its provider config in
//! `~/.omp/agent/models.yml` (YAML `providers.*` blocks) and its active
//! model pointer in `~/.omp/agent/config.yml` `modelRoles.default`. Unlike
//! Codex / OpenCode / Pi — which use the JSON skeleton in
//! `multi_provider.rs` — OMP's files are YAML, so this module mirrors that
//! skeleton with `serde_yaml` (order-preserving mappings).

use super::*;
use crate::tool_sync::types::{is_valid_omp_role_name, omp_role_value};

// ---------------------------------------------------------------------------
// Oh My Pi (YAML multi-provider: ~/.omp/agent/models.yml + config.yml)
// ---------------------------------------------------------------------------

/// Shared YAML write skeleton mirroring [`sync_json_blocks_inner`]: rolling
/// backup → read-or-init root → drop stale `skillstar_*` blocks → one managed
/// block per bound provider → caller finalizes (active pointer) → persist.
/// `serde_yaml::Mapping` is order-preserving, so user key order survives.
pub(crate) fn sync_yaml_blocks_inner(
    entries: &[(&Provider, &BindingEntry)],
    active_id: &str,
    config_path: &Path,
    blocks_key: &str,
    init_root: impl Fn() -> serde_yaml::Value,
    build_block: impl Fn(&Provider, &str) -> serde_yaml::Value,
    finish_root: impl FnOnce(&mut serde_yaml::Mapping, Option<&ActivePointer>),
) -> Result<(Option<PathBuf>, Option<ActivePointer>)> {
    let backup_path = if config_path.exists() {
        Some(create_rolling_backup(config_path)?)
    } else {
        None
    };
    if let Some(parent) = config_path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("Failed to create directory {}", parent.display()))?;
    }

    let mut root: serde_yaml::Value = if config_path.exists() {
        let content = std::fs::read_to_string(config_path)
            .with_context(|| format!("Failed to read {}", config_path.display()))?;
        serde_yaml::from_str(&content).unwrap_or_else(|_| init_root())
    } else {
        init_root()
    };

    let file_label = config_path
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| config_path.display().to_string());
    let root_obj = root
        .as_mapping_mut()
        .with_context(|| format!("{file_label} root must be a mapping"))?;

    let provider_map = root_obj
        .entry(serde_yaml::Value::String(blocks_key.to_string()))
        .or_insert_with(|| serde_yaml::Value::Mapping(serde_yaml::Mapping::new()));
    let provider_map = provider_map
        .as_mapping_mut()
        .with_context(|| format!("{file_label} `{blocks_key}` must be a mapping"))?;

    // Drop stale skillstar* blocks, then write one per current entry.
    provider_map.retain(|k, _| !k.as_str().is_some_and(is_skillstar_managed_key));
    let mut active_pointer: Option<ActivePointer> = None;
    for (provider, entry) in entries {
        if openai_base(provider).trim().is_empty() {
            continue;
        }
        let key = skillstar_managed_key(&provider.id);
        let block = build_block(provider, &entry.model);
        if provider.id == active_id {
            let model_id = if entry.model.trim().is_empty() {
                default_model(provider).to_string()
            } else {
                entry.model.clone()
            };
            if !model_id.trim().is_empty() {
                active_pointer = Some((key.clone(), model_id));
            }
        }
        provider_map.insert(serde_yaml::Value::String(key), block);
    }

    finish_root(root_obj, active_pointer.as_ref());

    let output = serde_yaml::to_string(&root)
        .with_context(|| format!("Failed to serialize {file_label}"))?;
    std::fs::write(config_path, output)
        .with_context(|| format!("Failed to write {}", config_path.display()))?;

    Ok((backup_path, active_pointer))
}

/// Write a whole Oh My Pi binding to `~/.omp/agent/models.yml`
/// (+ the `config.yml` `modelRoles` pointers).
///
/// Each bound provider becomes a `providers.skillstar_<id>` block
/// (`api: "openai-completions"`, plaintext `apiKey`, minimal `{ id }` model
/// entries so OMP's own defaults apply).
///
/// Roles come from [`AgentBinding::roles`]: each assigned role is written
/// as `modelRoles.<role> = "skillstar_<id>/<model>[:thinking]"`. When no
/// `default` role is assigned, the active entry supplies it, which is the
/// pre-roles behaviour. Roles pointing at a provider that is not bound (or has
/// no OpenAI base URL, so it never reached `models.yml`) are skipped rather than
/// written as dangling pointers.
pub fn sync_omp_binding(
    binding: &AgentBinding,
    providers: &[Provider],
) -> Result<ToolSyncResultFlat> {
    let models_path = resolve_omp_models_path()?;
    let config_path = resolve_omp_config_path()?;
    Ok(ToolSyncResultFlat::from_write_outcome(
        "omp",
        &models_path,
        sync_omp_binding_inner(binding, providers, &models_path, &config_path),
    ))
}

/// Build one OMP provider block. Model entries carry only `id` — OMP supplies
/// its own `contextWindow` / `maxTokens` defaults, and we have no reliable
/// per-model metadata to override them with.
///
/// `role_models` are models this provider is referenced by through a role
/// assignment. They must be listed even when absent from `provider.models`
/// (a relay provider may have an empty catalogue and a hand-typed model),
/// otherwise the role would point at a model OMP cannot resolve.
pub(crate) fn build_omp_provider_block(
    provider: &Provider,
    model: &str,
    role_models: &[String],
) -> serde_yaml::Value {
    let base_url = openai_base(provider).trim().trim_end_matches('/');

    let mut seen = std::collections::HashSet::new();
    let mut model_ids: Vec<String> = Vec::new();
    for candidate in std::iter::once(model)
        .chain(std::iter::once(default_model(provider)))
        .chain(provider.models.iter().map(String::as_str))
        .chain(role_models.iter().map(String::as_str))
    {
        let id = candidate.trim();
        if !id.is_empty() && seen.insert(id.to_string()) {
            model_ids.push(id.to_string());
        }
    }

    let models: Vec<serde_json::Value> = model_ids
        .into_iter()
        .map(|id| serde_json::json!({ "id": id }))
        .collect();

    serde_yaml::to_value(serde_json::json!({
        "baseUrl": base_url,
        "api": "openai-completions",
        "apiKey": api_key(provider),
        "models": models
    }))
    .expect("static OMP provider block always serializes")
}

/// Path-taking core of [`sync_omp_binding`] — exposed `pub(crate)` so unit
/// tests can drive it against isolated temp paths instead of the shared
/// sandbox HOME (avoids cross-test races on `~/.omp/agent/models.yml`).
pub(crate) fn sync_omp_binding_inner(
    binding: &AgentBinding,
    providers: &[Provider],
    models_path: &Path,
    config_path: &Path,
) -> Result<Option<PathBuf>> {
    let (entries, active_id) = resolve_entries(binding, providers)
        .context("OMP binding has no resolvable provider entries")?;

    // A role may name any model of any bound provider, including one absent from
    // the provider's catalogue. Collect them first so every role target gets a
    // matching entry in models.yml.
    let role_models = role_models_by_provider(binding);

    let (backup_path, active_pointer) = sync_yaml_blocks_inner(
        &entries,
        &active_id,
        models_path,
        "providers",
        || serde_yaml::to_value(serde_json::json!({ "providers": {} })).unwrap(),
        |provider, model| {
            let extra = role_models
                .get(&provider.id)
                .map(Vec::as_slice)
                .unwrap_or(&[]);
            build_omp_provider_block(provider, model, extra)
        },
        // OMP's active pointer lives in config.yml modelRoles, not models.yml.
        |_root, _active| {},
    )?;

    // config.yml: rewrite the managed modelRoles, preserving every other role
    // and setting the user keeps there.
    let roles = resolve_omp_roles(binding, &entries, active_pointer.as_ref());
    set_omp_model_roles(config_path, &roles)?;

    Ok(backup_path)
}

/// Models each provider is referenced by through a role assignment, keyed by
/// SkillStar provider id. Feeds the `models.yml` block so no role can name a
/// model the provider block does not declare.
fn role_models_by_provider(
    binding: &AgentBinding,
) -> std::collections::HashMap<String, Vec<String>> {
    let mut map: std::collections::HashMap<String, Vec<String>> = std::collections::HashMap::new();
    for (role, target) in &binding.roles {
        if !is_valid_omp_role_name(role) {
            continue;
        }
        let model = target.model.trim();
        if model.is_empty() {
            continue;
        }
        let slot = map.entry(target.provider_id.clone()).or_default();
        if !slot.iter().any(|m| m == model) {
            slot.push(model.to_string());
        }
    }
    map
}

/// Turn the binding's role assignments into the concrete `(role, value)` pairs
/// to write, dropping anything that would land as a dangling pointer.
///
/// A role survives only if its name is writable, its provider is actually bound
/// *and* reached `models.yml` (non-empty OpenAI base URL), and it names a model.
/// `default` falls back to the active entry so a binding with no role config at
/// all keeps behaving exactly as it did before roles existed.
fn resolve_omp_roles(
    binding: &AgentBinding,
    entries: &[(&Provider, &BindingEntry)],
    active_pointer: Option<&ActivePointer>,
) -> Vec<(String, String)> {
    let mut resolved: Vec<(String, String)> = Vec::new();

    for (role, target) in &binding.roles {
        // Back into OMP's vocabulary. The store holds canonical ids; OMP knows
        // `smol` and `task`, not `fast` and `subagent`, so writing the store's
        // spelling would drop the user's routing and add roles OMP ignores.
        let role = crate::providers::migrate::omp_role_key(role);
        if !is_valid_omp_role_name(&role) {
            continue;
        }
        // The provider must have produced a `skillstar_*` block in models.yml,
        // otherwise the role would point at a provider OMP cannot resolve.
        let written = entries.iter().any(|(provider, _)| {
            provider.id == target.provider_id && !openai_base(provider).trim().is_empty()
        });
        if !written {
            continue;
        }
        let key = skillstar_managed_key(&target.provider_id);
        if let Some(value) = omp_role_value(target, &key) {
            resolved.push((role, value));
        }
    }

    if !resolved.iter().any(|(role, _)| role == "default")
        && let Some((provider_key, model_id)) = active_pointer
    {
        resolved.push(("default".to_string(), format!("{provider_key}/{model_id}")));
    }

    // Sorted by OMP's own role name. `modelRoles` is an order-preserving
    // mapping, so the iteration order of the store's map would otherwise leak
    // into the file: renaming `smol` to the canonical `fast` in the store moved
    // it two places in the YAML even though nothing about the routing changed.
    // A config file that reorders itself for internal reasons is a diff nobody
    // can read.
    resolved.sort_by(|(a, _), (b, _)| a.cmp(b));
    resolved
}

/// Rewrite the SkillStar-managed entries of `modelRoles` in
/// `~/.omp/agent/config.yml`, preserving every other key and role.
///
/// Mirrors how `models.yml` provider blocks are synced: drop every role that
/// currently points at a `skillstar_*` provider (so roles the user unassigned in
/// SkillStar disappear instead of lingering as dangling pointers), then write
/// the current set. Roles pointing at the user's own providers are never
/// touched. Creates the file when absent.
fn set_omp_model_roles(config_path: &Path, roles: &[(String, String)]) -> Result<()> {
    if config_path.exists() {
        create_rolling_backup(config_path)?;
    }
    if let Some(parent) = config_path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("Failed to create directory {}", parent.display()))?;
    }

    let mut root: serde_yaml::Value = if config_path.exists() {
        let content = std::fs::read_to_string(config_path)
            .with_context(|| format!("Failed to read {}", config_path.display()))?;
        serde_yaml::from_str(&content)
            .unwrap_or_else(|_| serde_yaml::Value::Mapping(Default::default()))
    } else {
        serde_yaml::Value::Mapping(Default::default())
    };
    let root_obj = root
        .as_mapping_mut()
        .context("config.yml root must be a mapping")?;
    let roles_map = root_obj
        .entry(serde_yaml::Value::String("modelRoles".to_string()))
        .or_insert_with(|| serde_yaml::Value::Mapping(serde_yaml::Mapping::new()));
    let roles_map = roles_map
        .as_mapping_mut()
        .context("config.yml `modelRoles` must be a mapping")?;

    roles_map.retain(|_, v| !role_value_points_at_managed(v));
    for (role, value) in roles {
        roles_map.insert(
            serde_yaml::Value::String(role.clone()),
            serde_yaml::Value::String(value.clone()),
        );
    }

    std::fs::write(config_path, serde_yaml::to_string(&root)?)
        .with_context(|| format!("Failed to write {}", config_path.display()))?;
    Ok(())
}

/// Whether a `modelRoles` value is one SkillStar owns, i.e. its `provider/model`
/// prefix is a `skillstar_*` managed key.
fn role_value_points_at_managed(value: &serde_yaml::Value) -> bool {
    value
        .as_str()
        .and_then(|s| s.split('/').next())
        .is_some_and(is_skillstar_managed_key)
}

/// Remove every SkillStar-managed OMP provider block (`skillstar` +
/// `skillstar_*`) from `models.yml`, plus every `modelRoles` entry in
/// `config.yml` that targets one. Roles pointing at the user's own providers,
/// and all other user settings, survive untouched.
pub fn unsync_omp_all() -> Result<()> {
    let models_path = resolve_omp_models_path()?;
    let config_path = resolve_omp_config_path()?;
    unsync_omp_all_at(&models_path, &config_path)
}

/// Path-taking core of [`unsync_omp_all`] — exposed `pub(crate)` so unit tests
/// can drive it against isolated temp paths instead of the shared sandbox HOME.
pub(crate) fn unsync_omp_all_at(models_path: &Path, config_path: &Path) -> Result<()> {
    if models_path.exists() {
        create_rolling_backup(models_path)?;
        let content = std::fs::read_to_string(models_path)?;
        let mut root: serde_yaml::Value = serde_yaml::from_str(&content)
            .with_context(|| format!("Failed to parse {}", models_path.display()))?;
        if let Some(root_obj) = root.as_mapping_mut()
            && let Some(providers) = root_obj
                .get_mut(serde_yaml::Value::String("providers".to_string()))
                .and_then(|v| v.as_mapping_mut())
        {
            providers.retain(|k, _| !k.as_str().is_some_and(is_skillstar_managed_key));
        }
        std::fs::write(models_path, serde_yaml::to_string(&root)?)?;
    }

    if config_path.exists() {
        let content = std::fs::read_to_string(config_path)?;
        let mut root: serde_yaml::Value = serde_yaml::from_str(&content)
            .with_context(|| format!("Failed to parse {}", config_path.display()))?;
        let has_managed_role = root
            .get(serde_yaml::Value::String("modelRoles".to_string()))
            .and_then(|v| v.as_mapping())
            .is_some_and(|roles| roles.values().any(role_value_points_at_managed));
        if has_managed_role
            && let Some(root_obj) = root.as_mapping_mut()
            && let Some(roles) = root_obj
                .get_mut(serde_yaml::Value::String("modelRoles".to_string()))
                .and_then(|v| v.as_mapping_mut())
        {
            create_rolling_backup(config_path)?;
            // Every managed role goes, not just `default` — a `smol`/`slow`
            // pointer left behind would dangle once its provider block is gone.
            roles.retain(|_, v| !role_value_points_at_managed(v));
            std::fs::write(config_path, serde_yaml::to_string(&root)?)?;
        }
    }
    Ok(())
}
