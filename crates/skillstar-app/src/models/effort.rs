//! Catalog effort levels. The list is read from the gateway cache.

/// Levels for one upstream id. An unknown id is an empty list.
pub fn model_efforts(id: &str) -> Vec<String> {
    skillstar_gateway::model_efforts(id)
}
