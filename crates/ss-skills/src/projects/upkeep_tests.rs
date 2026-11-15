use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use super::{forget_project_copy, inspect_project_copies, retry_project_copies};
use crate::projects::{self, ProjectCopyState, ProjectDeployMode, SkillsList};
use crate::test_sandbox::Sandbox;
use ss_core::infra::paths;

fn skill(name: &str, body: &str) -> PathBuf {
    let source = paths::hub_skills_dir().join(name);
    fs::create_dir_all(&source).unwrap();
    fs::write(source.join("SKILL.md"), body).unwrap();
    source
}

fn project(
    root: &Path,
    name: &str,
    agents: &[&str],
    mode: ProjectDeployMode,
) -> projects::ProjectEntry {
    let path = root.join(name);
    fs::create_dir_all(&path).unwrap();
    let members = agents
        .iter()
        .map(|id| ((*id).to_string(), vec!["demo".into()]))
        .collect();
    projects::save_and_sync(
        path.to_str().unwrap(),
        members,
        HashMap::from([(".agents/skills".into(), mode)]),
    )
    .unwrap();
    projects::register_project(path.to_str().unwrap()).unwrap()
}

#[test]
fn shared_copy_is_detected_once_and_upgrade_stays_a_copy() {
    let env = Sandbox::new();
    let source = skill("demo", "v1");
    let entry = project(
        env.root(),
        "shared",
        &["codex", "pi"],
        ProjectDeployMode::Copy,
    );
    let records = inspect_project_copies(&entry.name).unwrap();
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].state(), ProjectCopyState::Current);
    fs::write(source.join("SKILL.md"), "v2").unwrap();
    assert_eq!(
        inspect_project_copies(&entry.name).unwrap()[0].state(),
        ProjectCopyState::Stale
    );
    let summary = projects::cascade_skill_update_to_projects(&["demo".into()]);
    assert!(summary.failures.is_empty(), "{:?}", summary.failures);
    assert_eq!(summary.projects_updated, vec![entry.name.clone()]);
    let target = Path::new(&entry.path).join(".agents/skills/demo");
    assert!(!ss_core::infra::fs_ops::is_link(&target));
    assert_eq!(fs::read_to_string(target.join("SKILL.md")).unwrap(), "v2");
    assert_eq!(
        inspect_project_copies(&entry.name).unwrap()[0].state(),
        ProjectCopyState::Current
    );
}

#[test]
fn missing_project_does_not_block_other_copies_and_can_forget_without_its_root() {
    let env = Sandbox::new();
    let source = skill("demo", "v1");
    let gone = project(
        env.root(),
        "gone",
        &["codex", "pi"],
        ProjectDeployMode::Copy,
    );
    let good = project(env.root(), "good", &["codex"], ProjectDeployMode::Copy);
    let moved = env.root().join("moved");
    fs::rename(&gone.path, &moved).unwrap();
    fs::write(source.join("SKILL.md"), "v2").unwrap();
    let summary = projects::cascade_skill_update_to_projects(&["demo".into()]);
    assert_eq!(summary.projects_updated, vec![good.name]);
    assert!(
        summary
            .failures
            .iter()
            .any(|error| error.contains(&gone.name) && error.contains(&gone.path))
    );
    assert!(!Path::new(&gone.path).exists());
    assert_eq!(
        inspect_project_copies(&gone.name).unwrap()[0].state(),
        ProjectCopyState::ProjectMissing
    );
    forget_project_copy(&gone.name, ".agents/skills", "demo").unwrap();
    assert!(
        projects::load_skills_list(&gone.name)
            .unwrap()
            .agents
            .is_empty()
    );
    assert!(inspect_project_copies(&gone.name).unwrap().is_empty());
    assert_eq!(
        fs::read_to_string(moved.join(".agents/skills/demo/SKILL.md")).unwrap(),
        "v1"
    );
    assert!(source.join("SKILL.md").exists());
}

#[test]
fn rebind_moved_project_preserves_copy_mode_and_refreshes_to_current_hub() {
    let env = Sandbox::new();
    let source = skill("demo", "v1");
    let entry = project(env.root(), "before", &["codex"], ProjectDeployMode::Copy);
    let moved = env.root().join("after");
    fs::rename(&entry.path, &moved).unwrap();
    fs::write(source.join("SKILL.md"), "v2").unwrap();
    projects::update_project_path(&entry.name, moved.to_str().unwrap()).unwrap();
    let target = moved.join(".agents/skills/demo");
    assert_eq!(fs::read_to_string(target.join("SKILL.md")).unwrap(), "v2");
    assert!(!ss_core::infra::fs_ops::is_link(&target));
    assert_eq!(
        inspect_project_copies(&entry.name).unwrap()[0].state(),
        ProjectCopyState::Current
    );
}

#[test]
fn retry_retains_edited_and_deleted_copies_and_reports_recovery() {
    let env = Sandbox::new();
    let source = skill("demo", "v1");
    let edited = project(env.root(), "edited", &["codex"], ProjectDeployMode::Copy);
    let target = Path::new(&edited.path).join(".agents/skills/demo");
    fs::write(target.join("SKILL.md"), "mine").unwrap();
    fs::write(source.join("SKILL.md"), "v2").unwrap();
    assert_eq!(
        inspect_project_copies(&edited.name).unwrap()[0].state(),
        ProjectCopyState::Modified
    );
    assert!(retry_project_copies(&edited.name).is_err());
    assert_eq!(fs::read_to_string(target.join("SKILL.md")).unwrap(), "mine");
    fs::remove_dir_all(&target).unwrap();
    assert_eq!(
        inspect_project_copies(&edited.name).unwrap()[0].state(),
        ProjectCopyState::Missing
    );
    retry_project_copies(&edited.name).unwrap();
    assert!(
        !target.exists(),
        "an explicitly removed copy must not be restored by upkeep"
    );
}

#[test]
fn actual_marked_copy_fallback_is_detected_even_with_symlink_configuration() {
    let env = Sandbox::new();
    let source = skill("demo", "v1");
    let root = env.root().join("fallback");
    let target = root.join(".agents/skills/demo");
    fs::create_dir_all(target.parent().unwrap()).unwrap();
    let entry = projects::register_project(root.to_str().unwrap()).unwrap();
    crate::deployment::ownership::deploy_copy(&source, &target, "demo").unwrap();
    projects::save_skills_list(
        &entry.name,
        &SkillsList {
            agents: HashMap::from([("codex".into(), vec!["demo".into()])]),
            ..SkillsList::default()
        },
    )
    .unwrap();
    fs::write(source.join("SKILL.md"), "v2").unwrap();
    assert_eq!(
        inspect_project_copies(&entry.name).unwrap()[0].state(),
        ProjectCopyState::Stale
    );
    retry_project_copies(&entry.name).unwrap();
    assert_eq!(fs::read_to_string(target.join("SKILL.md")).unwrap(), "v2");
    assert!(!ss_core::infra::fs_ops::is_link(&target));
    fs::remove_dir_all(&target).unwrap();
    assert_eq!(
        inspect_project_copies(&entry.name).unwrap()[0].state(),
        ProjectCopyState::Missing
    );
    retry_project_copies(&entry.name).unwrap();
    assert!(!target.exists());
    let summary = projects::cascade_skill_update_to_projects(&["demo".into()]);
    assert!(
        summary
            .failures
            .iter()
            .any(|error| error.contains(&entry.name))
    );
    forget_project_copy(&entry.name, ".agents/skills", "demo").unwrap();
    assert!(inspect_project_copies(&entry.name).unwrap().is_empty());
}
