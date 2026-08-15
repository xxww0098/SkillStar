//! Repairing what is already on disk, after the store has been migrated.
//!
//! ## Why migrating the store is not enough
//!
//! The v3 → v4 migration fixes SkillStar's own file. It does not touch
//! `~/.codex/config.toml`, and that file is where the damage is: for every
//! third-party provider a user bound to Codex, SkillStar wrote
//! `wire_api = "chat"`. Codex ≥0.95 deleted that variant from its `WireApi`
//! enum, so the value no longer deserializes — and because it appears inside
//! `config.toml`, the failure is not scoped to one provider table. The whole
//! file fails to parse and **Codex does not start at all**.
//!
//! A user in that state cannot fix it from SkillStar's UI, because the UI's
//! only lever was the binding that produced it. So the migration has to reach
//! out and clean up: drop the entries that can no longer be written, delete
//! their tables from the file, and re-project everything that survives.
//!
//! ## Why the dropped entries are reported rather than deleted quietly
//!
//! Losing a Codex binding is a real loss, and the user did not ask for it. The
//! entries land in [`MigrationReport::codex_dropped`] with the provider name
//! and model, so the UI can say which ones went and why — and so that probing
//! the host later can offer them back rather than requiring the user to
//! remember what they had.

use super::{agent_spec, codex_can_serve, sync_tool_binding, unsync_codex_entry, unsync_tool};
use crate::providers::ProvidersStoreV4;
use crate::providers::migrate::{DropReason, DroppedBinding, MigrationReport};
use tracing::warn;

/// The agent whose writer v4 retired. Its marker file is deleted, not migrated.
const PLANNED_AGENT_CLAUDE_DESKTOP: &str = "claude-desktop";

/// What [`repair_agent_configs`] did, for the caller to log or surface.
#[derive(Debug, Default, PartialEq)]
pub struct ConfigRepair {
    /// Agents whose config was rewritten from the migrated store.
    pub resynced: Vec<String>,
    /// Agents whose config was cleared because nothing bindable was left.
    pub unsynced: Vec<String>,
    /// `(agent_id, provider_id)` pairs whose managed table was removed.
    pub dropped_entries: Vec<(String, String)>,
}

/// Re-project every binding onto disk, dropping what v4 cannot write.
///
/// Mutates `store` — the dropped Codex entries are removed from it as well, so
/// the file and the store agree afterwards. The caller persists the result.
///
/// Failures are collected as report warnings rather than propagated. The store
/// migration has already committed by the time this runs; aborting here would
/// leave the store at v4 and the config files half-repaired, which is strictly
/// worse than a repaired store plus a named warning.
pub fn repair_agent_configs(
    store: &mut ProvidersStoreV4,
    report: &mut MigrationReport,
) -> ConfigRepair {
    let mut repair = ConfigRepair::default();

    // Claude Desktop never had a real writer — only a marker file. It is a
    // planned agent in v4, so the marker goes and the binding is already
    // reported as dropped by the store migration.
    if store.bindings.contains_key(PLANNED_AGENT_CLAUDE_DESKTOP)
        && let Err(e) = unsync_tool(PLANNED_AGENT_CLAUDE_DESKTOP)
    {
        report
            .warnings
            .push(format!("could not remove the Claude Desktop marker: {e}"));
    }
    store.bindings.remove(PLANNED_AGENT_CLAUDE_DESKTOP);

    drop_unwritable_codex_entries(store, report, &mut repair);

    // Sorted so a migration run is reproducible and its log diffable.
    let mut agent_ids: Vec<String> = store.bindings.keys().cloned().collect();
    agent_ids.sort();

    for agent_id in agent_ids {
        if agent_spec(&agent_id).is_none() {
            // A tool id left behind by an older SkillStar (`gemini`). Nothing
            // was ever written for it, so there is nothing to repair.
            continue;
        }
        let empty = store
            .bindings
            .get(&agent_id)
            .is_none_or(|binding| binding.is_empty());
        if empty {
            match unsync_tool(&agent_id) {
                Ok(()) => repair.unsynced.push(agent_id.clone()),
                Err(e) => report
                    .warnings
                    .push(format!("{agent_id}: could not clear its config ({e})")),
            }
            continue;
        }
        let result = sync_tool_binding(store, &agent_id);
        if result.success {
            repair.resynced.push(agent_id.clone());
        } else {
            report.warnings.push(format!(
                "{agent_id}: could not rewrite its config ({})",
                result.error.unwrap_or_else(|| "unknown error".to_string())
            ));
        }
    }

    repair
}

/// Remove Codex entries whose provider cannot serve `/v1/responses`, from both
/// the store and `config.toml`.
fn drop_unwritable_codex_entries(
    store: &mut ProvidersStoreV4,
    report: &mut MigrationReport,
    repair: &mut ConfigRepair,
) {
    let Some(binding) = store.bindings.get("codex") else {
        return;
    };

    let mut doomed: Vec<DroppedBinding> = Vec::new();
    for entry in &binding.entries {
        match store.provider(&entry.provider_id) {
            Some(provider) => {
                // Native-login rows have no endpoints on purpose; they are not
                // broken, they are the "hand control back to Codex" state.
                if provider.is_external_cli() || codex_can_serve(provider) {
                    continue;
                }
                doomed.push(DroppedBinding {
                    agent_id: "codex".to_string(),
                    provider_id: entry.provider_id.clone(),
                    provider_name: provider.name.clone(),
                    model: entry.model.clone(),
                    reason: DropReason::CodexRequiresResponsesApi,
                });
            }
            None => doomed.push(DroppedBinding {
                agent_id: "codex".to_string(),
                provider_id: entry.provider_id.clone(),
                provider_name: String::new(),
                model: entry.model.clone(),
                reason: DropReason::ProviderMissing,
            }),
        }
    }

    if doomed.is_empty() {
        return;
    }

    for dropped in &doomed {
        // Clear the table first. If this fails the entry stays in the store,
        // so the next run tries again rather than leaving an orphaned table
        // nobody remembers writing.
        if let Err(e) = unsync_codex_entry(&dropped.provider_id) {
            warn!(
                "failed to remove Codex table for provider {}: {e}",
                dropped.provider_id
            );
            report.warnings.push(format!(
                "codex: the stale table for '{}' could not be removed ({e}); Codex may still fail to start",
                dropped.provider_name
            ));
            continue;
        }
        repair
            .dropped_entries
            .push(("codex".to_string(), dropped.provider_id.clone()));
    }

    let removed: std::collections::HashSet<&str> = repair
        .dropped_entries
        .iter()
        .map(|(_, provider_id)| provider_id.as_str())
        .collect();

    if let Some(binding) = store.bindings.get_mut("codex") {
        let active_provider = binding.active().map(|e| e.provider_id.clone());
        binding
            .entries
            .retain(|entry| !removed.contains(entry.provider_id.as_str()));
        binding
            .roles
            .retain(|_, target| !removed.contains(target.provider_id.as_str()));
        // Re-point rather than re-index: the surviving entries kept their
        // relative order, so the old index may now name a different provider.
        binding.active_index = active_provider
            .and_then(|id| binding.entries.iter().position(|e| e.provider_id == id))
            .unwrap_or(0);
    }

    report
        .codex_dropped
        .extend(doomed.into_iter().filter(|d| removed.contains(d.provider_id.as_str())));
}
