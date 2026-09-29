//! Oh My Pi sync is a no-op. Unsync still strips SkillStar-managed YAML blocks.

use super::*;

/// Oh My Pi config is no longer written from the provider store.
pub fn sync_omp_binding(
    binding: &AgentBinding,
    providers: &[Provider],
) -> Result<ToolSyncResultFlat> {
    let models_path = resolve_omp_models_path()?;
    let config_path = resolve_omp_config_path()?;
    Ok(ToolSyncResultFlat::from_write_outcome_with_drops(
        "omp",
        &models_path,
        sync_omp_binding_with_drops(binding, providers, &models_path, &config_path),
    ))
}

/// Path-taking core of [`sync_omp_binding`] — exposed `pub(crate)` so unit
/// tests can drive it against isolated temp paths instead of the shared
/// sandbox HOME (avoids cross-test races on `~/.omp/agent/models.yml`).
#[cfg(test)]
pub(crate) fn sync_omp_binding_inner(
    binding: &AgentBinding,
    providers: &[Provider],
    models_path: &Path,
    config_path: &Path,
) -> Result<Option<PathBuf>> {
    sync_omp_binding_with_drops(binding, providers, models_path, config_path)
        .map(|(backup, _dropped)| backup)
}

/// As [`sync_omp_binding_inner`], but also reporting the roles that did not make
/// it onto disk.
pub(crate) fn sync_omp_binding_with_drops(
    binding: &AgentBinding,
    providers: &[Provider],
    models_path: &Path,
    config_path: &Path,
) -> Result<(Option<PathBuf>, Vec<DroppedRole>)> {
    let _ = (binding, providers, models_path, config_path);
    Ok((None, Vec::new()))
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
        skillstar_core::infra::fs_ops::atomic_write(
            models_path,
            serde_yaml::to_string(&root)?.as_bytes(),
        )?;
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
            skillstar_core::infra::fs_ops::atomic_write(
                config_path,
                serde_yaml::to_string(&root)?.as_bytes(),
            )?;
        }
    }
    Ok(())
}
