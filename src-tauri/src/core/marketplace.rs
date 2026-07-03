pub fn initialize_local_snapshot() -> anyhow::Result<()> {
    super::marketplace_snapshot::initialize()
}

pub async fn refresh_local_snapshot_startup_scopes() -> anyhow::Result<()> {
    super::marketplace_snapshot::refresh_startup_scopes_if_needed().await
}

pub async fn resolve_skill_sources_local_first(
    names: &[String],
    existing_sources: &std::collections::HashMap<String, String>,
) -> anyhow::Result<std::collections::HashMap<String, String>> {
    super::marketplace_snapshot::resolve_skill_sources_local_first(names, existing_sources).await
}
