//! Validation for custom agents' skill directories.
//!
//! This is the single source of truth for the project-skills-rel format. It is
//! equivalent to the frontend regex literal `^\.[a-zA-Z0-9_-]+/skills$` in
//! `src/features/settings/components/AddCustomAgentDialog.tsx` — keep the two in
//! sync (the frontend regex is a UX pre-check; this function is authoritative).

use anyhow::{Result, anyhow};
use std::path::Path;

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

/// Reject a custom agent's resolved `global_skills_dir` when it names a whole
/// home directory or a filesystem root.
///
/// Deliberately narrow. Several agents legitimately resolve to one shared
/// global skills directory, and that directory need not sit under `$HOME` nor
/// be named `skills`, so neither can be a requirement here. What SkillStar
/// must refuse is a target whose entire contents it would later sweep in
/// `unlink_all_skills_from_agent` / "unlink all".
///
/// Takes the already-resolved path (tilde expanded) so this stays the single
/// rule and `CustomSpec::resolve_global_dir` stays the single expansion.
pub(crate) fn validate_global_skills_dir(resolved: &Path, home: &Path) -> Result<()> {
    if resolved.as_os_str().is_empty() {
        // Empty is the project-only agent: it has no global skills dir at all.
        return Ok(());
    }
    if resolved.parent().is_none() {
        return Err(anyhow!(
            "Global skills path must not be a filesystem root: {}",
            resolved.display()
        ));
    }
    if resolved == home {
        return Err(anyhow!(
            "Global skills path must not be the home directory itself: {}",
            resolved.display()
        ));
    }
    Ok(())
}
