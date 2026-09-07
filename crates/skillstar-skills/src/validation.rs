//! SkillStar install adapter over [`skill_spec::frontmatter`].
//!
//! Parse and diagnostics live in the product-agnostic `skill-spec` leaf.
//! This module re-exports that API so existing callers keep
//! `skillstar_skills::validation::…` paths, and owns the install-path gate.

use std::path::Path;

pub use skill_spec::frontmatter::{
    inspect_skill_frontmatter, inspect_skill_frontmatter_content, FrontmatterIssue,
    FrontmatterReport, MAX_DESCRIPTION_CHARS, MAX_MANIFEST_BYTES, MAX_NAME_CHARS,
};

/// Blocking check used by the repo-install path and bundle export/import.
///
/// Returns a single actionable reason naming the skill directory.
pub fn ensure_installable(skill_dir: &Path) -> Result<(), String> {
    let report = inspect_skill_frontmatter(skill_dir);
    match report.first_blocking() {
        Some(issue) => Err(format!(
            "SKILL.md is not a valid skill: {}",
            issue.describe()
        )),
        None => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn missing_description_blocks_install() {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir_all(dir.path()).unwrap();
        fs::write(
            dir.path().join("SKILL.md"),
            "---\nname: my-skill\n---\n\n# Body\n",
        )
        .unwrap();
        let error = ensure_installable(dir.path()).unwrap_err();
        assert!(error.contains("description"), "{error}");
    }
}
