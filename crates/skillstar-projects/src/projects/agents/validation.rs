//! Validation for custom agents' `project_skills_rel`.
//!
//! This is the single source of truth for the project-skills-rel format. It is
//! equivalent to the frontend regex literal `^\.[a-zA-Z0-9_-]+/skills$` in
//! `src/features/settings/components/AddCustomAgentDialog.tsx` — keep the two in
//! sync (the frontend regex is a UX pre-check; this function is authoritative).

use anyhow::{anyhow, Result};

/// Validate and normalize a custom agent's `project_skills_rel`.
///
/// Trims, normalizes backslashes to `/`, and accepts either an empty string
/// (global-only agent) or `.<name>/skills` where `<name>` is one segment of
/// `[A-Za-z0-9_-]`. Returns the normalized value.
pub(crate) fn validate_project_skills_rel(raw: &str) -> Result<String> {
    let normalized = raw.trim().replace('\\', "/");
    if normalized.is_empty() {
        return Ok(normalized);
    }
    if !normalized.starts_with('.') || !normalized.ends_with("/skills") {
        return Err(anyhow!(
            "Project skills path must strictly follow the format '.agentname/skills'"
        ));
    }
    let middle = &normalized[1..normalized.len() - "/skills".len()];
    if middle.is_empty()
        || !middle
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    {
        return Err(anyhow!(
            "Project skills path must strictly follow the format '.agentname/skills'"
        ));
    }
    Ok(normalized)
}
