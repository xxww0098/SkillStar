//! Paths under the SkillStar data directory for project metadata.

use std::path::PathBuf;

use crate::core::infra::paths as data_paths;

pub(crate) fn index_path() -> PathBuf {
    data_paths::projects_manifest_path()
}

/// Directory for a specific project's config files.
pub(crate) fn project_dir(name: &str) -> PathBuf {
    data_paths::project_detail_dir(name)
}

/// Path to a project's skill list file.
pub(crate) fn skills_list_path(name: &str) -> PathBuf {
    project_dir(name).join("skills-list.json")
}
