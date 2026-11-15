use super::super::store::{load_skills_list_strict, save_skills_list};
use super::super::types::{ProjectDeployMode, SkillsList};
use super::*;
use ss_core::infra::fs_ops;
use std::collections::HashMap;
use std::ffi::OsString;
use std::fs;
use std::path::PathBuf;

struct TestEnv {
    root: tempfile::TempDir,
    previous: Vec<(&'static str, Option<OsString>)>,
    _lock: std::sync::MutexGuard<'static, ()>,
}

impl TestEnv {
    fn new() -> Self {
        let lock = crate::lock_test_env();
        let root = tempfile::tempdir().unwrap();
        let mut previous = Vec::new();
        for (key, path) in [
            ("HOME", root.path().join("home")),
            ("SKILLSTAR_DATA_DIR", root.path().join("data")),
            ("SKILLSTAR_HUB_DIR", root.path().join("hub")),
        ] {
            fs::create_dir_all(&path).unwrap();
            previous.push((key, std::env::var_os(key)));
            unsafe {
                std::env::set_var(key, path);
            }
        }
        #[cfg(windows)]
        {
            previous.push(("USERPROFILE", std::env::var_os("USERPROFILE")));
            unsafe {
                std::env::set_var("USERPROFILE", root.path().join("home"));
            }
        }
        Self {
            root,
            previous,
            _lock: lock,
        }
    }

    fn project(&self, name: &str) -> PathBuf {
        let project = self.root.path().join(name);
        fs::create_dir_all(&project).unwrap();
        project
    }

    fn source(&self, name: &str, content: &str) -> PathBuf {
        let source = fs_paths::hub_skills_dir().join(name);
        fs::create_dir_all(&source).unwrap();
        fs::write(source.join("SKILL.md"), content).unwrap();
        source
    }
}

impl Drop for TestEnv {
    fn drop(&mut self) {
        for (key, value) in self.previous.drain(..) {
            unsafe {
                match value {
                    Some(value) => std::env::set_var(key, value),
                    None => std::env::remove_var(key),
                }
            }
        }
    }
}

fn manifest(names: &[&str], mode: ProjectDeployMode) -> SkillsList {
    SkillsList {
        agents: HashMap::from([(
            "claude".into(),
            names.iter().map(|name| (*name).into()).collect(),
        )]),
        deploy_modes: HashMap::from([(".claude/skills".into(), mode)]),
        ..SkillsList::default()
    }
}

#[test]
fn migration_recovers_a_missing_root_in_original_copy_mode() {
    let env = TestEnv::new();
    let old = env.project("old");
    env.source("demo", "hub");
    let entry = register_project(old.to_str().unwrap()).unwrap();
    let list = manifest(&["demo"], ProjectDeployMode::Copy);
    save_skills_list(&entry.name, &list).unwrap();
    let missing = env.root.path().join("moved-away");
    fs::rename(&old, &missing).unwrap();
    let new = env.project("new");
    assert_eq!(
        update_project_path(&entry.name, new.to_str().unwrap()).unwrap(),
        1
    );
    let stored = load_index_strict().unwrap();
    assert_eq!(stored.projects[0].path, new.to_string_lossy());
    let target = new.join(".claude/skills/demo");
    assert!(!fs_ops::is_link(&target));
    assert_eq!(fs::read_to_string(target.join("SKILL.md")).unwrap(), "hub");
    assert_eq!(
        load_skills_list_strict(&entry.name)
            .unwrap()
            .unwrap()
            .deploy_modes,
        list.deploy_modes
    );
    assert!(missing.is_dir());
}

#[test]
fn forgotten_copy_survives_path_rebind() {
    let env = TestEnv::new();
    let old = env.project("old");
    let forgotten_source = env.source("foo", "forgotten original");
    let tracked_source = env.source("bar", "tracked original");
    let list = manifest(&["foo", "bar"], ProjectDeployMode::Copy);
    let (name, _) =
        super::super::sync::save_and_sync(old.to_str().unwrap(), list.agents, list.deploy_modes)
            .unwrap();
    let marker_name = crate::deployment::ownership::DEPLOY_MARKER;
    let forgotten_marker = fs::read(old.join(".claude/skills/foo").join(marker_name)).unwrap();
    super::super::forget_project_copy(&name, ".claude/skills", "foo").unwrap();
    let before = fs::read(fs_paths::projects_manifest_path()).unwrap();
    let moved = env.root.path().join("moved");
    fs::rename(&old, &moved).unwrap();
    assert_eq!(
        fs::read_to_string(moved.join(".claude/skills/foo/SKILL.md")).unwrap(),
        "forgotten original"
    );
    fs::write(
        forgotten_source.join("SKILL.md"),
        "forgotten upstream change",
    )
    .unwrap();
    let mut deep = tracked_source.join("deep");
    for _ in 0..=crate::materialize::COPY_LIMITS.max_depth {
        deep = deep.join("nested");
    }
    fs::create_dir_all(&deep).unwrap();
    fs::write(deep.join("file"), "too deep").unwrap();
    assert!(update_project_path(&name, moved.to_str().unwrap()).is_err());
    assert_eq!(
        fs::read(fs_paths::projects_manifest_path()).unwrap(),
        before
    );
    assert_eq!(
        fs::read_to_string(moved.join(".claude/skills/bar/SKILL.md")).unwrap(),
        "tracked original"
    );
    assert_eq!(
        fs::read(moved.join(".claude/skills/foo").join(marker_name)).unwrap(),
        forgotten_marker
    );
    fs::remove_dir_all(tracked_source.join("deep")).unwrap();
    fs::write(tracked_source.join("SKILL.md"), "tracked new").unwrap();
    assert_eq!(
        update_project_path(&name, moved.to_str().unwrap()).unwrap(),
        1
    );
    assert_eq!(
        load_index_strict().unwrap().projects[0].path,
        moved.to_string_lossy()
    );
    assert_eq!(
        fs::read_to_string(moved.join(".claude/skills/foo/SKILL.md")).unwrap(),
        "forgotten original"
    );
    assert_eq!(
        fs::read(moved.join(".claude/skills/foo").join(marker_name)).unwrap(),
        forgotten_marker
    );
    assert_eq!(
        fs::read_to_string(moved.join(".claude/skills/bar/SKILL.md")).unwrap(),
        "tracked new"
    );
    assert_eq!(
        crate::deployment::ownership::owned_project_deployment(
            &moved.join(".claude/skills/bar"),
            "bar"
        ),
        crate::deployment::Ownership::Copy
    );
    assert_eq!(
        load_skills_list_strict(&name).unwrap().unwrap().agents["claude"],
        ["bar"]
    );
}

#[test]
fn migration_conflict_preflight_preserves_index_old_root_and_all_new_entries() {
    let env = TestEnv::new();
    let old = env.project("old");
    env.source("good", "hub-good");
    env.source("blocked", "hub-blocked");
    let list = manifest(&["good", "blocked"], ProjectDeployMode::Copy);
    let (name, _) = super::super::sync::save_and_sync(
        old.to_str().unwrap(),
        list.agents.clone(),
        list.deploy_modes.clone(),
    )
    .unwrap();
    let before = fs::read(fs_paths::projects_manifest_path()).unwrap();
    let new = env.project("new");
    fs::create_dir_all(new.join(".claude/skills/blocked")).unwrap();
    fs::write(new.join(".claude/skills/blocked/LOCAL.md"), "keep").unwrap();
    let err = update_project_path(&name, new.to_str().unwrap()).unwrap_err();
    assert!(format!("{err:#}").contains("preflight"), "{err:#}");
    assert_eq!(
        fs::read(fs_paths::projects_manifest_path()).unwrap(),
        before
    );
    assert!(old.join(".claude/skills/good/SKILL.md").is_file());
    assert_eq!(
        fs::read_to_string(new.join(".claude/skills/blocked/LOCAL.md")).unwrap(),
        "keep"
    );
    assert!(!new.join(".claude/skills/good").exists());
}

#[test]
fn migration_copy_failure_preserves_registry_and_supports_retry() {
    let env = TestEnv::new();
    let old = env.project("old");
    env.source("good", "old-good");
    let source = env.source("failing", "old-failing");
    let list = manifest(&["good", "failing"], ProjectDeployMode::Copy);
    let (name, _) =
        super::super::sync::save_and_sync(old.to_str().unwrap(), list.agents, list.deploy_modes)
            .unwrap();
    let before = fs::read(fs_paths::projects_manifest_path()).unwrap();
    let new = env.project("new");
    let mut deep = source.join("deep");
    for _ in 0..=crate::materialize::COPY_LIMITS.max_depth {
        deep = deep.join("nested");
    }
    fs::create_dir_all(&deep).unwrap();
    fs::write(deep.join("file"), "too deep").unwrap();
    assert!(update_project_path(&name, new.to_str().unwrap()).is_err());
    assert_eq!(
        fs::read(fs_paths::projects_manifest_path()).unwrap(),
        before
    );
    assert!(new.join(".claude/skills/good/SKILL.md").is_file());
    assert!(!new.join(".claude/skills/failing").exists());
    assert_eq!(
        fs::read_to_string(old.join(".claude/skills/failing/SKILL.md")).unwrap(),
        "old-failing"
    );
    fs::remove_dir_all(source.join("deep")).unwrap();
    assert_eq!(
        update_project_path(&name, new.to_str().unwrap()).unwrap(),
        2
    );
    assert_eq!(
        load_index_strict().unwrap().projects[0].path,
        new.to_string_lossy()
    );
    assert!(!fs_ops::is_link(&new.join(".claude/skills/good")));
    assert!(!fs_ops::is_link(&new.join(".claude/skills/failing")));
    assert!(old.join(".claude/skills/failing/SKILL.md").is_file());
}

#[test]
fn migration_rejects_other_projects_canonical_root_and_invalid_new_root() {
    let env = TestEnv::new();
    let old = env.project("old");
    let other = env.project("other");
    let entry = register_project(old.to_str().unwrap()).unwrap();
    register_project(other.to_str().unwrap()).unwrap();
    let alias = env.root.path().join("other-alias");
    fs_ops::create_symlink(&other, &alias).unwrap();
    let before = fs::read(fs_paths::projects_manifest_path()).unwrap();
    let err = update_project_path(&entry.name, alias.to_str().unwrap()).unwrap_err();
    assert!(format!("{err:#}").contains("already registered"), "{err:#}");
    assert!(
        update_project_path(
            &entry.name,
            env.root.path().join("absent").to_str().unwrap()
        )
        .is_err()
    );
    assert_eq!(
        fs::read(fs_paths::projects_manifest_path()).unwrap(),
        before
    );
    assert!(!other.join(".claude").exists());
}

#[test]
fn damaged_index_is_rejected_by_every_index_writer() {
    let env = TestEnv::new();
    let old = env.project("old");
    let new = env.project("new");
    let path = fs_paths::projects_manifest_path();
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(&path, "{bad-index").unwrap();
    assert!(register_project(old.to_str().unwrap()).is_err());
    assert!(remove_project("old").is_err());
    assert!(update_project_path("old", new.to_str().unwrap()).is_err());
    assert!(save_index(&ProjectIndex::default()).is_err());
    assert_eq!(fs::read_to_string(&path).unwrap(), "{bad-index");
    assert!(!fs_paths::project_detail_dir("old").exists());
}

#[test]
fn damaged_manifest_cannot_be_overwritten_removed_or_migrated() {
    let env = TestEnv::new();
    let old = env.project("old");
    let new = env.project("new");
    let entry = register_project(old.to_str().unwrap()).unwrap();
    let before = fs::read(fs_paths::projects_manifest_path()).unwrap();
    let path = fs_paths::project_detail_dir(&entry.name).join("skills-list.json");
    fs::write(&path, "{bad-skills").unwrap();
    assert!(save_skills_list(&entry.name, &SkillsList::default()).is_err());
    assert!(remove_project(&entry.name).is_err());
    assert!(update_project_path(&entry.name, new.to_str().unwrap()).is_err());
    assert_eq!(fs::read_to_string(path).unwrap(), "{bad-skills");
    assert_eq!(
        fs::read(fs_paths::projects_manifest_path()).unwrap(),
        before
    );
    assert!(!new.join(".claude").exists());
}

#[test]
fn atomic_metadata_replacements_leave_valid_json_and_no_temporaries() {
    let env = TestEnv::new();
    let root = env.project("project");
    let entry = register_project(root.to_str().unwrap()).unwrap();
    let list = manifest(&["demo"], ProjectDeployMode::Copy);
    save_skills_list(&entry.name, &list).unwrap();
    save_skills_list(&entry.name, &SkillsList::default()).unwrap();
    let index = load_index_strict().unwrap();
    save_index(&index).unwrap();
    assert!(
        load_skills_list_strict(&entry.name)
            .unwrap()
            .unwrap()
            .agents
            .is_empty()
    );
    for directory in [
        fs_paths::state_dir(),
        fs_paths::project_detail_dir(&entry.name),
    ] {
        assert!(
            !fs::read_dir(directory)
                .unwrap()
                .filter_map(|entry| entry.ok())
                .any(|entry| entry.file_name().to_string_lossy().ends_with(".tmp"))
        );
    }
}

#[test]
fn concurrent_registers_keep_every_index_entry() {
    let env = TestEnv::new();
    let first = env.project("first");
    let second = env.project("second");
    let barrier = std::sync::Arc::new(std::sync::Barrier::new(2));
    let threads = [first, second]
        .into_iter()
        .map(|project| {
            let barrier = barrier.clone();
            std::thread::spawn(move || {
                barrier.wait();
                register_project(project.to_str().unwrap()).unwrap()
            })
        })
        .collect::<Vec<_>>();
    for thread in threads {
        thread.join().unwrap();
    }
    let projects = load_index_strict().unwrap().projects;
    assert_eq!(projects.len(), 2);
    assert!(projects.iter().any(|entry| entry.name == "first"));
    assert!(projects.iter().any(|entry| entry.name == "second"));
}

#[test]
fn inverse_lock_order_is_rejected_before_migration_mutates_index() {
    let env = TestEnv::new();
    let old = env.project("old");
    let new = env.project("new");
    let entry = register_project(old.to_str().unwrap()).unwrap();
    let before = fs::read(fs_paths::projects_manifest_path()).unwrap();
    let _project = super::super::write_lock::lock_project_write().unwrap();
    let start = std::time::Instant::now();
    assert!(update_project_path(&entry.name, new.to_str().unwrap()).is_err());
    assert!(start.elapsed() < std::time::Duration::from_secs(1));
    assert_eq!(
        fs::read(fs_paths::projects_manifest_path()).unwrap(),
        before
    );
}
