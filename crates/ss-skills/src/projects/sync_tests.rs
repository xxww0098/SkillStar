use super::*;
use ss_core::infra::fs_ops;
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

    fn project(&self) -> PathBuf {
        let project = self.root.path().join("project");
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

fn list(agent: &str, names: &[&str], mode: ProjectDeployMode) -> SkillsList {
    let relative = match agent {
        "claude" => ".claude/skills",
        _ => ".agents/skills",
    };
    SkillsList {
        agents: HashMap::from([(
            agent.into(),
            names.iter().map(|name| (*name).into()).collect(),
        )]),
        deploy_modes: HashMap::from([(relative.into(), mode)]),
        ..SkillsList::default()
    }
}

fn make_oversized_depth(source: &Path) {
    let mut path = source.join("too-deep");
    for _ in 0..=crate::materialize::COPY_LIMITS.max_depth {
        path = path.join("nested");
    }
    fs::create_dir_all(&path).unwrap();
    fs::write(path.join("payload"), "oversized tree").unwrap();
}

#[test]
fn path_plans_union_shared_members_once() {
    let _env = TestEnv::new();
    let profiles = agent_profile::list_profiles();
    let mut manifest = list("codex", &["alpha", "shared"], ProjectDeployMode::Copy);
    manifest
        .agents
        .insert("pi".into(), vec!["beta".into(), "shared".into()]);
    let plans = build_path_plans(&manifest, &profiles);
    assert_eq!(plans.len(), 1);
    assert_eq!(plans[0].profile.project_skills_rel, ".agents/skills");
    assert_eq!(plans[0].skill_names, ["alpha", "shared", "beta"]);
    assert_eq!(plans[0].mode, ProjectDeployMode::Copy);
}

#[test]
fn full_sync_refreshes_copy_and_preserves_copy_mode() {
    let env = TestEnv::new();
    let project = env.project();
    let source = env.source("demo", "old");
    let manifest = list("claude", &["demo"], ProjectDeployMode::Copy);
    assert_eq!(
        full_sync(project.to_str().unwrap(), &manifest, None).unwrap(),
        1
    );
    fs::write(source.join("SKILL.md"), "new").unwrap();
    assert_eq!(
        full_sync(project.to_str().unwrap(), &manifest, None).unwrap(),
        1
    );
    let target = project.join(".claude/skills/demo");
    assert!(!fs_ops::is_link(&target));
    assert_eq!(fs::read_to_string(target.join("SKILL.md")).unwrap(), "new");
    assert_eq!(
        crate::deployment::ownership::owned_project_deployment(&target, "demo"),
        crate::deployment::Ownership::Copy
    );
}

#[test]
fn copy_stage_failure_keeps_owned_copy_and_owned_link() {
    for initial_mode in [ProjectDeployMode::Copy, ProjectDeployMode::Symlink] {
        let env = TestEnv::new();
        let project = env.project();
        let source = env.source("demo", "old");
        let manifest = list("claude", &["demo"], initial_mode);
        full_sync(project.to_str().unwrap(), &manifest, None).unwrap();
        let target = project.join(".claude/skills/demo");
        let original_kind = crate::deployment::ownership::owned_project_deployment(&target, "demo");
        make_oversized_depth(&source);
        let copy = list("claude", &["demo"], ProjectDeployMode::Copy);
        let err = full_sync(project.to_str().unwrap(), &copy, None).unwrap_err();
        assert!(format!("{err:#}").contains("nested deeper"), "{err:#}");
        assert_eq!(fs::read_to_string(target.join("SKILL.md")).unwrap(), "old");
        assert_eq!(
            crate::deployment::ownership::owned_project_deployment(&target, "demo"),
            original_kind
        );
        let residuals = fs::read_dir(target.parent().unwrap())
            .unwrap()
            .filter_map(|entry| entry.ok())
            .any(|entry| {
                entry
                    .file_name()
                    .to_string_lossy()
                    .starts_with(".skillstar-")
            });
        assert!(!residuals, "failed stage must clean its temporary entries");
    }
}

#[test]
fn full_sync_reports_conflicts_and_missing_sources_without_losing_old_items() {
    let env = TestEnv::new();
    let project = env.project();
    let missing_source = env.source("missing", "old");
    env.source("modified", "original");
    let initial = list(
        "claude",
        &["missing", "modified", "deselected"],
        ProjectDeployMode::Copy,
    );
    env.source("deselected", "remove");
    full_sync(project.to_str().unwrap(), &initial, None).unwrap();
    let directory = project.join(".claude/skills");
    fs::remove_dir_all(missing_source).unwrap();
    fs::write(directory.join("modified/SKILL.md"), "user edit").unwrap();
    fs::create_dir_all(directory.join("foreign")).unwrap();
    fs::write(directory.join("foreign/LOCAL.md"), "user file").unwrap();
    env.source("foreign", "hub");
    env.source("good", "good");
    let desired = list(
        "claude",
        &["missing", "modified", "foreign", "good"],
        ProjectDeployMode::Copy,
    );
    let err = full_sync(project.to_str().unwrap(), &desired, None).unwrap_err();
    let text = format!("{err:#}");
    assert!(text.contains("3 failure(s)"), "{text}");
    assert!(text.contains("hub source is missing"), "{text}");
    assert!(text.contains("foreign entry"), "{text}");
    assert_eq!(
        fs::read_to_string(directory.join("missing/SKILL.md")).unwrap(),
        "old"
    );
    assert_eq!(
        fs::read_to_string(directory.join("modified/SKILL.md")).unwrap(),
        "user edit"
    );
    assert_eq!(
        fs::read_to_string(directory.join("foreign/LOCAL.md")).unwrap(),
        "user file"
    );
    assert_eq!(
        fs::read_to_string(directory.join("good/SKILL.md")).unwrap(),
        "good"
    );
    assert!(!directory.join("deselected").exists());
}

#[test]
fn full_sync_keeps_dangling_owned_link_when_source_vanishes() {
    let env = TestEnv::new();
    let project = env.project();
    let source = env.source("demo", "old");
    let manifest = list("claude", &["demo"], ProjectDeployMode::Symlink);
    full_sync(project.to_str().unwrap(), &manifest, None).unwrap();
    let target = project.join(".claude/skills/demo");
    if !fs_ops::is_link(&target) {
        return;
    }
    let original = fs_ops::read_link_resolved(&target).unwrap();
    fs::remove_dir_all(source).unwrap();
    assert!(full_sync(project.to_str().unwrap(), &manifest, None).is_err());
    assert!(fs_ops::is_link(&target));
    assert_eq!(fs_ops::read_link_resolved(&target).unwrap(), original);
}

#[test]
fn shared_path_cleanup_keeps_other_selected_members_and_unmentioned_agents() {
    let env = TestEnv::new();
    let project = env.project();
    env.source("alpha", "alpha");
    env.source("beta", "beta");
    let mut initial = list("codex", &["alpha"], ProjectDeployMode::Copy);
    initial.agents.insert("pi".into(), vec!["beta".into()]);
    initial.agents.insert("claude".into(), vec!["alpha".into()]);
    assert_eq!(
        full_sync(project.to_str().unwrap(), &initial, None).unwrap(),
        3
    );
    let desired = list("pi", &["beta"], ProjectDeployMode::Copy);
    assert_eq!(
        full_sync(project.to_str().unwrap(), &desired, Some(&["codex".into()])).unwrap(),
        1
    );
    assert!(!project.join(".agents/skills/alpha").exists());
    assert!(project.join(".agents/skills/beta/SKILL.md").is_file());
    assert!(project.join(".claude/skills/alpha/SKILL.md").is_file());
}

#[test]
fn unsafe_names_and_relative_paths_fail_before_persisting() {
    let env = TestEnv::new();
    let project = env.project();
    let invalid = list("claude", &["../outside"], ProjectDeployMode::Copy);
    assert!(full_sync(project.to_str().unwrap(), &invalid, None).is_err());
    assert!(
        save_and_sync(
            project.to_str().unwrap(),
            invalid.agents,
            invalid.deploy_modes
        )
        .is_err()
    );
    assert!(!fs_paths::projects_manifest_path().exists());
    assert!(!project.join(".claude").exists());
    let mut invalid = SkillsList::default();
    invalid
        .deploy_modes
        .insert("../outside".into(), ProjectDeployMode::Copy);
    assert!(save_skills_list("project", &invalid).is_err());
    assert!(!fs_paths::project_detail_dir("project").exists());
}

#[test]
fn sync_rejects_parent_escape_without_touching_outside_tree() {
    let env = TestEnv::new();
    let project = env.project();
    env.source("demo", "hub");
    let outside = env.root.path().join("outside");
    fs::create_dir_all(outside.join("skills/demo")).unwrap();
    fs::write(outside.join("skills/demo/SKILL.md"), "keep").unwrap();
    fs_ops::create_symlink(&outside, &project.join(".claude")).unwrap();
    let manifest = list("claude", &["demo"], ProjectDeployMode::Copy);
    assert!(full_sync(project.to_str().unwrap(), &manifest, None).is_err());
    assert!(
        save_and_sync(
            project.to_str().unwrap(),
            manifest.agents,
            manifest.deploy_modes
        )
        .is_err()
    );
    assert_eq!(
        fs::read_to_string(outside.join("skills/demo/SKILL.md")).unwrap(),
        "keep"
    );
    assert!(!fs_paths::projects_manifest_path().exists());
}

#[test]
fn physical_directory_aliases_share_the_desired_union() {
    let env = TestEnv::new();
    let project = env.project();
    env.source("alpha", "alpha");
    env.source("beta", "beta");
    let shared = project.join("shared");
    fs::create_dir_all(shared.join("skills")).unwrap();
    fs_ops::create_symlink(&shared, &project.join(".claude")).unwrap();
    fs_ops::create_symlink(&shared, &project.join(".agents")).unwrap();
    let mut manifest = list("claude", &["alpha"], ProjectDeployMode::Copy);
    manifest.agents.insert("codex".into(), vec!["beta".into()]);
    manifest
        .deploy_modes
        .insert(".agents/skills".into(), ProjectDeployMode::Copy);
    assert_eq!(
        full_sync(project.to_str().unwrap(), &manifest, None).unwrap(),
        2
    );
    assert!(shared.join("skills/alpha/SKILL.md").is_file());
    assert!(shared.join("skills/beta/SKILL.md").is_file());
}

#[test]
fn sync_writers_do_not_default_over_a_damaged_manifest() {
    let env = TestEnv::new();
    let project = env.project();
    env.source("demo", "hub");
    let entry = register_project(project.to_str().unwrap()).unwrap();
    let path = fs_paths::project_detail_dir(&entry.name).join("skills-list.json");
    fs::write(&path, "{broken").unwrap();
    let agents = HashMap::from([("claude".into(), vec!["demo".into()])]);
    assert!(save_and_sync(project.to_str().unwrap(), agents, HashMap::new()).is_err());
    assert!(
        add_skills_to_project_with_mode(
            project.to_str().unwrap(),
            &["demo".into()],
            &["claude".into()],
            ProjectDeployMode::Symlink,
        )
        .is_err()
    );
    assert_eq!(fs::read_to_string(&path).unwrap(), "{broken");
    assert!(!project.join(".claude").exists());
}

#[test]
fn physical_plans_merge_actual_directory_aliases_and_choose_copy_for_observation() {
    let env = TestEnv::new();
    let project = env.project();
    let shared = project.join("shared");
    fs::create_dir_all(shared.join("skills")).unwrap();
    fs_ops::create_symlink(&shared, &project.join(".claude")).unwrap();
    fs_ops::create_symlink(&shared, &project.join(".agents")).unwrap();
    let profiles = agent_profile::list_profiles();
    let mut manifest = list(
        "codex",
        &["common", "codex-only"],
        ProjectDeployMode::Symlink,
    );
    manifest
        .agents
        .insert("claude".into(), vec!["common".into(), "claude-only".into()]);
    manifest
        .deploy_modes
        .insert(".claude/skills".into(), ProjectDeployMode::Copy);
    let plans = build_physical_path_plans(Some(&project), &manifest, &profiles).unwrap();
    assert_eq!(plans.len(), 1);
    assert_eq!(plans[0].skill_names.len(), 3);
    assert!(plans[0].skill_names.iter().any(|name| name == "common"));
    assert!(plans[0].skill_names.iter().any(|name| name == "codex-only"));
    assert!(
        plans[0]
            .skill_names
            .iter()
            .any(|name| name == "claude-only")
    );
    assert_eq!(plans[0].mode, ProjectDeployMode::Copy);
    assert_eq!(
        project_path_key(Some(&project), ".claude/skills").unwrap(),
        project_path_key(Some(&project), ".agents/skills").unwrap()
    );
    let metadata_plans = build_physical_path_plans(None, &manifest, &profiles).unwrap();
    assert_eq!(
        metadata_plans.len(),
        2,
        "without a live root, alias identity stays relative"
    );
    assert_ne!(
        project_path_key(None, ".claude/skills").unwrap(),
        project_path_key(None, ".agents/skills").unwrap()
    );
    assert!(project_path_key(Some(&project), "../escape").is_err());
    assert!(project_path_key(None, "../escape").is_err());
}

#[test]
fn physical_plans_do_not_merge_skill_links_that_share_one_hub_source() {
    let env = TestEnv::new();
    let project = env.project();
    env.source("demo", "hub");
    let mut manifest = list("claude", &["demo"], ProjectDeployMode::Symlink);
    manifest.agents.insert("codex".into(), vec!["demo".into()]);
    assert_eq!(
        full_sync(project.to_str().unwrap(), &manifest, None).unwrap(),
        2
    );
    let profiles = agent_profile::list_profiles();
    let plans = build_physical_path_plans(Some(&project), &manifest, &profiles).unwrap();
    assert_eq!(
        plans.len(),
        2,
        "alias identity belongs to the parent deployment directory"
    );
}

#[test]
fn actual_alias_copy_inspection_refresh_and_forget_use_one_physical_membership() {
    let env = TestEnv::new();
    let project = env.project();
    let shared = project.join("shared");
    fs::create_dir_all(shared.join("skills")).unwrap();
    fs_ops::create_symlink(&shared, &project.join(".claude")).unwrap();
    fs_ops::create_symlink(&shared, &project.join(".agents")).unwrap();
    let source = env.source("demo", "old");
    env.source("claude-only", "claude");
    env.source("codex-only", "codex");
    let mut manifest = list("claude", &["demo", "claude-only"], ProjectDeployMode::Copy);
    manifest
        .agents
        .insert("codex".into(), vec!["demo".into(), "codex-only".into()]);
    manifest
        .deploy_modes
        .insert(".agents/skills".into(), ProjectDeployMode::Copy);
    let (name, count) = save_and_sync(
        project.to_str().unwrap(),
        manifest.agents,
        manifest.deploy_modes,
    )
    .unwrap();
    assert_eq!(count, 3);
    let records = super::super::inspect_project_copies(&name).unwrap();
    assert_eq!(records.len(), 3, "each physical copy must have one record");
    let demo = records
        .iter()
        .find(|record| record.skill_name() == "demo")
        .unwrap();
    let relative = demo.relative_path().to_string();
    fs::write(source.join("SKILL.md"), "new").unwrap();
    assert_eq!(
        super::super::refresh_stale_copies(project.to_str().unwrap()).unwrap(),
        1
    );
    assert_eq!(
        fs::read_to_string(shared.join("skills/demo/SKILL.md")).unwrap(),
        "new"
    );
    let marker = fs::read(
        shared
            .join("skills/demo")
            .join(crate::deployment::ownership::DEPLOY_MARKER),
    )
    .unwrap();
    super::super::forget_project_copy(&name, &relative, "demo").unwrap();
    let saved = load_skills_list_strict(&name).unwrap().unwrap();
    assert!(
        saved
            .agents
            .values()
            .all(|members| !members.iter().any(|member| member == "demo"))
    );
    assert!(
        saved.agents["claude"]
            .iter()
            .any(|member| member == "claude-only")
    );
    assert!(
        saved.agents["codex"]
            .iter()
            .any(|member| member == "codex-only")
    );
    assert_eq!(
        super::super::inspect_project_copies(&name).unwrap().len(),
        2
    );
    assert_eq!(
        fs::read(
            shared
                .join("skills/demo")
                .join(crate::deployment::ownership::DEPLOY_MARKER)
        )
        .unwrap(),
        marker
    );
    assert_eq!(
        fs::read_to_string(shared.join("skills/demo/SKILL.md")).unwrap(),
        "new"
    );
}

#[test]
fn missing_root_alias_forget_uses_only_relative_metadata_and_leaves_moved_tree() {
    let env = TestEnv::new();
    let project = env.project();
    let shared = project.join("shared");
    fs::create_dir_all(shared.join("skills")).unwrap();
    fs_ops::create_symlink(&shared, &project.join(".claude")).unwrap();
    fs_ops::create_symlink(&shared, &project.join(".agents")).unwrap();
    env.source("demo", "old");
    let mut manifest = list("claude", &["demo"], ProjectDeployMode::Copy);
    manifest.agents.insert("codex".into(), vec!["demo".into()]);
    manifest.agents.insert("pi".into(), vec!["demo".into()]);
    manifest
        .deploy_modes
        .insert(".agents/skills".into(), ProjectDeployMode::Copy);
    let (name, _) = save_and_sync(
        project.to_str().unwrap(),
        manifest.agents,
        manifest.deploy_modes,
    )
    .unwrap();
    let moved = env.root.path().join("moved");
    fs::rename(&project, &moved).unwrap();
    super::super::forget_project_copy(&name, ".agents/skills", "demo").unwrap();
    let saved = load_skills_list_strict(&name).unwrap().unwrap();
    assert!(!saved.agents.contains_key("codex"));
    assert!(!saved.agents.contains_key("pi"));
    assert_eq!(saved.agents["claude"], ["demo"]);
    assert!(
        !project.exists(),
        "metadata-only recovery must not recreate the missing root"
    );
    assert_eq!(
        fs::read_to_string(moved.join("shared/skills/demo/SKILL.md")).unwrap(),
        "old"
    );
    assert!(
        moved
            .join("shared/skills/demo")
            .join(crate::deployment::ownership::DEPLOY_MARKER)
            .is_file()
    );
}

#[test]
fn sync_rejects_inverse_lock_order_without_waiting() {
    let env = TestEnv::new();
    let project = env.project();
    let _project = super::super::write_lock::lock_project_write().unwrap();
    let start = std::time::Instant::now();
    let err = full_sync(project.to_str().unwrap(), &SkillsList::default(), None).unwrap_err();
    assert!(format!("{err:#}").contains("lock"), "{err:#}");
    assert!(start.elapsed() < std::time::Duration::from_secs(1));
}
