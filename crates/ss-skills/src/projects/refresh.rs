//! Keep selected, owned project copies in sync with the canonical skills.

use anyhow::{Context, Result};
use ss_core::infra::paths;

use super::index::load_index_strict;
use super::store::load_skills_list_strict;
use super::sync::build_physical_path_plans;
use super::types::ensure_project_root_exists;
use crate::deployment::ownership::{self, Ownership};

/// Refresh selected copies, preserving missing or locally edited deployments.
/// Every failed reconciliation is reported; callers can retry the current Hub
/// content without waiting for another upstream update.
pub fn refresh_stale_copies(project_path: &str) -> Result<u32> {
    let report = refresh_stale_copies_inner(project_path, None)?;
    if !report.failures.is_empty() {
        anyhow::bail!(
            "Project copies could not be synchronized:\n{}",
            report.failures.join("\n")
        );
    }
    Ok(report.refreshed)
}

#[derive(Debug, Default)]
pub(super) struct RefreshStaleCopiesReport {
    pub refreshed: u32,
    pub failures: Vec<String>,
}

pub(super) fn refresh_stale_copies_strict(
    project_path: &str,
    skills: &[String],
) -> Result<RefreshStaleCopiesReport> {
    refresh_stale_copies_inner(project_path, Some(skills))
}

fn refresh_stale_copies_inner(
    project_path: &str,
    only_skills: Option<&[String]>,
) -> Result<RefreshStaleCopiesReport> {
    // Canonical content must stay stable while it is copied. The update lock
    // precedes the project lock, including when a channel re-enters this path.
    let _source = super::upkeep::lock_copy_source()?;
    let _guard = super::write_lock::lock_project_write()?;
    let Some(entry) = load_index_strict()?
        .projects
        .into_iter()
        .find(|entry| entry.path == project_path)
    else {
        return Ok(RefreshStaleCopiesReport::default());
    };
    let project = ensure_project_root_exists(project_path).with_context(|| {
        format!(
            "找不到项目“{}”：{}。请在项目页更改路径，或清理不再需要的复制记录；项目文件会保留",
            entry.name, entry.path
        )
    })?;
    let Some(list) = load_skills_list_strict(&entry.name)? else {
        return Ok(RefreshStaleCopiesReport::default());
    };
    let profiles = crate::agents::list_profile_specs();
    super::store::validate_skills_list_with_profiles(&list, &profiles)?;
    let mut report = RefreshStaleCopiesReport::default();
    for plan in build_physical_path_plans(Some(&project), &list, &profiles)? {
        let rel = &plan.profile.project_skills_rel;
        for skill in plan.skill_names {
            if only_skills.is_some_and(|selected| !selected.contains(&skill)) {
                continue;
            }
            let target = project.join(rel).join(&skill);
            let refreshed = (|| -> Result<bool> {
                crate::content::validate_skill_name(&skill)?;
                super::binding::contained_child(&project, &project.join(rel))?;
                match ownership::owned_project_deployment(&target, &skill) {
                    Ownership::Link { .. } => return Ok(false),
                    Ownership::Missing => {
                        if only_skills.is_some() {
                            anyhow::bail!(
                                "项目副本已不存在；请重新部署或清理复制记录，未自动创建目录"
                            );
                        }
                        return Ok(false);
                    }
                    Ownership::Foreign | Ownership::SkillStarModifiedCopy => {
                        anyhow::bail!(
                            "项目副本已修改或缺少有效部署标记；已保留原文件，可清理复制记录后自行保留或重新部署"
                        );
                    }
                    Ownership::Copy => {}
                }
                let source = paths::hub_skills_dir().join(&skill);
                anyhow::ensure!(
                    source.is_dir(),
                    "hub source is missing; restore the Hub skill or clear its project copy record"
                );
                if ownership::dir_content_hash(&source)? == ownership::dir_content_hash(&target)? {
                    return Ok(false);
                }
                ownership::refresh_project_copy(&source, &target, &skill)?;
                Ok(true)
            })();
            match refreshed {
                Ok(true) => report.refreshed += 1,
                Ok(false) => {}
                Err(error) => report.failures.push(format!("{rel}/{skill}: {error:#}")),
            }
        }
    }
    Ok(report)
}
