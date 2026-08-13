//! Binding-level settings tests — the tool-wide bag on `ToolBinding.settings`,
//! as opposed to the per-entry bag on `ToolActivation.settings`.
//!
//! OMP model roles are its only consumer today: a role may target any bound
//! provider, so it cannot live on a single entry. These cover the store-layer
//! contract (write, clear, prune on unbind/delete); the on-disk YAML side is
//! covered in `tool_sync::tests::part4`.

use super::*;
use crate::tool_sync::{OmpRoleTarget, OmpSettings};

/// A settings bag assigning `roles` as `(role, provider_id, model)`.
fn roles_value(roles: &[(&str, &str, &str)]) -> Value {
    let map: std::collections::BTreeMap<String, OmpRoleTarget> = roles
        .iter()
        .map(|(role, provider_id, model)| {
            (
                (*role).to_string(),
                OmpRoleTarget {
                    provider_id: (*provider_id).to_string(),
                    model: (*model).to_string(),
                    thinking: None,
                },
            )
        })
        .collect();
    serde_json::to_value(OmpSettings { roles: map }).unwrap()
}

/// Role names currently assigned on a tool's binding, sorted.
fn assigned_roles(store: &FlatProvidersStore, tool_id: &str) -> Vec<String> {
    let binding = store.tool_activations.get(tool_id).unwrap();
    let mut names: Vec<String> = OmpSettings::from_binding(binding)
        .roles
        .into_keys()
        .collect();
    names.sort();
    names
}

/// Bind two providers to OMP and return their ids.
fn setup_omp_store() -> (FlatProvidersStore, String, String) {
    let mut store = FlatProvidersStore::default();
    let a = create_provider_flat(&mut store, make_flat_entry("Alpha")).unwrap();
    let b = create_provider_flat(&mut store, make_flat_entry("Beta")).unwrap();
    activate_tool(&mut store, &a.id, "omp", Some("model-a"), None).unwrap();
    activate_tool(&mut store, &b.id, "omp", Some("model-a"), None).unwrap();
    (store, a.id, b.id)
}

#[test]
fn update_tool_binding_settings_stores_roles_without_touching_entries() {
    let (mut store, a, b) = setup_omp_store();
    let before = store.tool_activations.get("omp").unwrap().clone();

    update_tool_binding_settings(
        &mut store,
        "omp",
        roles_value(&[("default", &a, "model-a"), ("smol", &b, "model-a")]),
    )
    .unwrap();

    let after = store.tool_activations.get("omp").unwrap();
    // Roles land in the tool-level bag; provider/model selections are untouched.
    assert_eq!(after.entries, before.entries);
    assert_eq!(after.active_index, before.active_index);
    assert_eq!(assigned_roles(&store, "omp"), vec!["default", "smol"]);
}

#[test]
fn update_tool_binding_settings_clears_on_null() {
    let (mut store, a, _b) = setup_omp_store();
    update_tool_binding_settings(
        &mut store,
        "omp",
        roles_value(&[("default", &a, "model-a")]),
    )
    .unwrap();

    update_tool_binding_settings(&mut store, "omp", Value::Null).unwrap();

    // `null` empties the bag rather than storing a JSON null.
    assert!(
        store
            .tool_activations
            .get("omp")
            .unwrap()
            .settings
            .is_none()
    );
}

#[test]
fn update_tool_binding_settings_requires_an_active_tool() {
    let mut store = FlatProvidersStore::default();
    let err = update_tool_binding_settings(&mut store, "omp", roles_value(&[])).unwrap_err();
    assert!(err.to_string().contains("not currently active"));
}

#[test]
fn entry_settings_and_binding_settings_are_independent() {
    let (mut store, a, _b) = setup_omp_store();
    update_tool_binding_settings(
        &mut store,
        "omp",
        roles_value(&[("default", &a, "model-a")]),
    )
    .unwrap();

    update_tool_settings(&mut store, "omp", serde_json::json!({ "unrelated": true })).unwrap();

    let binding = store.tool_activations.get("omp").unwrap();
    // The per-entry write must not clobber the tool-level roles.
    assert_eq!(assigned_roles(&store, "omp"), vec!["default"]);
    assert_eq!(
        binding.active().unwrap().settings,
        Some(serde_json::json!({ "unrelated": true }))
    );
}

#[test]
fn removing_a_binding_entry_prunes_only_that_providers_roles() {
    let (mut store, a, b) = setup_omp_store();
    update_tool_binding_settings(
        &mut store,
        "omp",
        roles_value(&[
            ("default", &a, "model-a"),
            ("smol", &b, "model-a"),
            ("slow", &b, "model-a"),
        ]),
    )
    .unwrap();

    remove_binding_entry(&mut store, "omp", &b).unwrap();

    // Roles on the unbound provider would dangle, so they go; the other stays.
    assert_eq!(assigned_roles(&store, "omp"), vec!["default"]);
}

#[test]
fn deleting_a_provider_prunes_its_roles() {
    let (mut store, a, b) = setup_omp_store();
    update_tool_binding_settings(
        &mut store,
        "omp",
        roles_value(&[("default", &a, "model-a"), ("smol", &b, "model-a")]),
    )
    .unwrap();

    delete_provider_flat(&mut store, &a).unwrap();

    assert_eq!(assigned_roles(&store, "omp"), vec!["smol"]);
}

#[test]
fn pruning_roles_keeps_other_settings_keys() {
    let (mut store, a, b) = setup_omp_store();
    // A future sibling key in the same bag must survive role pruning.
    let mut settings = roles_value(&[("default", &a, "model-a"), ("smol", &b, "model-a")]);
    settings
        .as_object_mut()
        .unwrap()
        .insert("somethingElse".to_string(), Value::Bool(true));
    update_tool_binding_settings(&mut store, "omp", settings).unwrap();

    remove_binding_entry(&mut store, "omp", &b).unwrap();

    let stored = store
        .tool_activations
        .get("omp")
        .unwrap()
        .settings
        .as_ref()
        .unwrap();
    assert_eq!(stored.get("somethingElse"), Some(&Value::Bool(true)));
    assert_eq!(assigned_roles(&store, "omp"), vec!["default"]);
}

#[test]
fn binding_settings_survive_a_reactivation() {
    let (mut store, a, b) = setup_omp_store();
    update_tool_binding_settings(&mut store, "omp", roles_value(&[("smol", &b, "model-a")]))
        .unwrap();

    // Re-activating a provider (e.g. the user picks another model in the matrix)
    // rewrites an entry, not the tool-level bag.
    activate_tool(&mut store, &a, "omp", Some("model-a"), None).unwrap();

    assert_eq!(assigned_roles(&store, "omp"), vec!["smol"]);
}

#[test]
fn deactivating_a_tool_drops_its_roles() {
    let (mut store, a, _b) = setup_omp_store();
    update_tool_binding_settings(
        &mut store,
        "omp",
        roles_value(&[("default", &a, "model-a")]),
    )
    .unwrap();

    deactivate_tool(&mut store, "omp").unwrap();

    let binding = store.tool_activations.get("omp").unwrap();
    assert!(binding.is_empty());
    // An unbound tool has nothing to route, so the whole bag goes with it.
    assert!(binding.settings.is_none());
}
