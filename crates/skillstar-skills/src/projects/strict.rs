//! Incremental project-skill links that fail closed.
//!
//! The caller is expected to hold the project write lock. This function takes
//! it again; the lock re-enters on the same thread. A failed precheck or a
//! failed link leaves the project tree and the manifest unchanged.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use skillstar_core::infra::{fs_ops, paths as fs_paths};

use super::binding::{ObservedProject, contained_child};
use super::facts::inspect_project_skills;
use super::owner::shared_path_owner;
use super::store::{load_skills_list, save_skills_list};
use super::types::{ProjectDeployMode, SkillsList};
use super::write_lock::lock_project_write;
use crate::agents;
use crate::content::{self, snapshot};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StrictSkillStatus {
    Missing,
    Stale,
    RejectedCopy,
    Conflict,
    Already,
    Create,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StrictSkillReport {
    pub name: String,
    pub status: StrictSkillStatus,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StrictEnableReport {
    pub committed: bool,
    pub items: Vec<StrictSkillReport>,
}

pub fn enable_project_skills_strict(
    binding: &ObservedProject,
    agent_id: &str,
    skills: &[(String, String)],
) -> Result<StrictEnableReport> {
    let _guard = lock_project_write()?;
    if binding.name.is_none() {
        anyhow::bail!("project is not registered");
    }
    if agent_id.is_empty() || skills.is_empty() {
        anyhow::bail!("strict enable needs an agent and at least one skill");
    }
    let profiles = agents::list_profiles();
    let skills_list = binding
        .name
        .as_deref()
        .and_then(load_skills_list)
        .unwrap_or_default();
    let _facts = inspect_project_skills(binding);
    let profile = profiles
        .iter()
        .find(|profile| profile.id == agent_id && profile.has_project_skills())
        .context("agent has no project skills")?;
    let decision = shared_path_owner(
        &profiles,
        &skills_list,
        &profile.project_skills_rel,
        agent_id,
    );
    let owner_id = decision
        .owner_id
        .clone()
        .context("strict enable does not invent an owner")?;
    let mode = skills_list
        .deploy_modes
        .get(&profile.project_skills_rel)
        .copied();

    let mut items = Vec::with_capacity(skills.len());
    for (name, expected_hash) in skills {
        content::validate_skill_name(name)?;
        items.push(StrictSkillReport {
            name: name.clone(),
            status: classify_project_skill(
                &binding.root,
                &profile.project_skills_rel,
                name,
                expected_hash,
                mode,
            )?,
        });
    }
    if items.iter().any(|item| {
        !matches!(
            item.status,
            StrictSkillStatus::Already | StrictSkillStatus::Create
        )
    }) {
        return Ok(StrictEnableReport {
            committed: false,
            items,
        });
    }

    let directory = binding.root.join(&profile.project_skills_rel);
    let mut created = Vec::new();
    for item in items
        .iter()
        .filter(|item| item.status == StrictSkillStatus::Create)
    {
        let target = directory.join(&item.name);
        contained_child(&binding.root, &target)?;
    }
    if let Err(err) = link_new(&binding.root, &directory, &items, &mut created) {
        for path in created.iter().rev() {
            let _ = fs_ops::remove_symlink(path);
        }
        return Err(err);
    }

    let mut updated = skills_list;
    merge_names(&mut updated, &owner_id, skills);
    updated.deploy_modes.insert(
        profile.project_skills_rel.clone(),
        ProjectDeployMode::Symlink,
    );
    updated.updated_at = chrono::Utc::now().to_rfc3339();
    save_skills_list(binding.name.as_deref().unwrap(), &updated)?;
    Ok(StrictEnableReport {
        committed: true,
        items,
    })
}

pub fn classify_project_skill(
    root: &Path,
    rel: &str,
    name: &str,
    expected_hash: &str,
    mode: Option<ProjectDeployMode>,
) -> Result<StrictSkillStatus> {
    let hub = fs_paths::hub_skills_dir().join(name);
    if !hub.exists() {
        return Ok(StrictSkillStatus::Missing);
    }
    let actual = snapshot(name)?.content_hash;
    if actual != expected_hash {
        return Ok(StrictSkillStatus::Stale);
    }
    if mode == Some(ProjectDeployMode::Copy) {
        return Ok(StrictSkillStatus::RejectedCopy);
    }
    let target = root.join(rel).join(name);
    if fs_ops::is_link(&target) {
        let resolved = fs_ops::read_link_resolved(&target).ok();
        let hub_canonical = std::fs::canonicalize(&hub).ok();
        let same = match (resolved, hub_canonical) {
            (Some(link), Some(hub_path)) => {
                std::fs::canonicalize(&link).ok().as_ref() == Some(&hub_path) || link == hub_path
            }
            _ => false,
        };
        return Ok(if same {
            StrictSkillStatus::Already
        } else {
            StrictSkillStatus::Conflict
        });
    }
    if target.symlink_metadata().is_ok() {
        return Ok(StrictSkillStatus::Conflict);
    }
    Ok(StrictSkillStatus::Create)
}

fn link_new(
    root: &Path,
    directory: &Path,
    items: &[StrictSkillReport],
    created: &mut Vec<PathBuf>,
) -> Result<()> {
    let creates: Vec<_> = items
        .iter()
        .filter(|item| item.status == StrictSkillStatus::Create)
        .collect();
    if creates.is_empty() {
        return Ok(());
    }
    std::fs::create_dir_all(directory)
        .with_context(|| format!("create {}", directory.display()))?;
    for item in creates {
        let source = fs_paths::hub_skills_dir().join(&item.name);
        let target = directory.join(&item.name);
        contained_child(root, &target)?;
        fs_ops::create_symlink(&source, &target)
            .with_context(|| format!("link {}", target.display()))?;
        created.push(target);
    }
    Ok(())
}

fn merge_names(list: &mut SkillsList, owner_id: &str, skills: &[(String, String)]) {
    let entry = list.agents.entry(owner_id.to_string()).or_default();
    for (name, _) in skills {
        if !entry.iter().any(|existing| existing == name) {
            entry.push(name.clone());
        }
    }
}

#[cfg(test)]
mod strict_enable_tests {
    use super::{StrictSkillStatus, enable_project_skills_strict};
    use crate::content::snapshot;
    use crate::projects::{
        ProjectDeployMode, load_skills_list, observe_project, register_project, save_skills_list,
    };
    use skillstar_core::infra::paths as fs_paths;
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::time::{SystemTime, UNIX_EPOCH};

    struct EnvGuard {
        root: PathBuf,
        home: Option<std::ffi::OsString>,
        data: Option<std::ffi::OsString>,
        #[cfg(windows)]
        userprofile: Option<std::ffi::OsString>,
        _lock: std::sync::MutexGuard<'static, ()>,
    }

    impl EnvGuard {
        fn new(label: &str) -> Self {
            let _lock = crate::lock_test_env();
            let nanos = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos();
            let root = std::env::temp_dir().join(format!("skillstar-strict-{label}-{nanos}"));
            fs::create_dir_all(root.join("home")).unwrap();
            fs::create_dir_all(root.join("data")).unwrap();
            let guard = Self {
                home: std::env::var_os("HOME"),
                data: std::env::var_os("SKILLSTAR_DATA_DIR"),
                #[cfg(windows)]
                userprofile: std::env::var_os("USERPROFILE"),
                root,
                _lock,
            };
            unsafe {
                std::env::set_var("HOME", guard.root.join("home"));
                std::env::set_var("SKILLSTAR_DATA_DIR", guard.root.join("data"));
                #[cfg(windows)]
                std::env::set_var("USERPROFILE", guard.root.join("home"));
            }
            guard
        }
    }

    impl Drop for EnvGuard {
        fn drop(&mut self) {
            unsafe {
                restore("HOME", self.home.take());
                restore("SKILLSTAR_DATA_DIR", self.data.take());
                #[cfg(windows)]
                restore("USERPROFILE", self.userprofile.take());
            }
            let _ = fs::remove_dir_all(&self.root);
        }
    }

    unsafe fn restore(key: &str, previous: Option<std::ffi::OsString>) {
        match previous {
            Some(value) => unsafe { std::env::set_var(key, value) },
            None => unsafe { std::env::remove_var(key) },
        }
    }

    fn hub_skill(name: &str, body: &str) -> String {
        let dir = fs_paths::hub_skills_dir().join(name);
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("SKILL.md"), format!("# {name}\n\n{body}\n")).unwrap();
        snapshot(name).unwrap().content_hash
    }

    fn registered_project(root: &Path) -> crate::projects::ObservedProject {
        let project = root.join("project");
        fs::create_dir_all(&project).unwrap();
        register_project(project.to_str().unwrap()).unwrap();
        observe_project(project.to_str().unwrap()).unwrap()
    }

    fn manifest_bytes(name: &str) -> Vec<u8> {
        let path = fs_paths::project_detail_dir(name).join("skills-list.json");
        fs::read(&path).unwrap_or_default()
    }

    #[test]
    fn strict_enable_fails_closed_when_any_skill_is_missing_and_writes_nothing() {
        let env = EnvGuard::new("missing");
        let hash = hub_skill("present", "here");
        let binding = registered_project(&env.root);
        let before = manifest_bytes(binding.name.as_deref().unwrap());
        let tree = binding.root.join(".agents");
        let report = enable_project_skills_strict(
            &binding,
            "codex",
            &[
                ("present".into(), hash),
                ("absent".into(), "whatever".into()),
            ],
        )
        .unwrap();
        assert!(!report.committed);
        assert!(
            report
                .items
                .iter()
                .any(|item| item.status == StrictSkillStatus::Missing)
        );
        assert_eq!(manifest_bytes(binding.name.as_deref().unwrap()), before);
        assert!(!tree.exists());
    }

    #[test]
    fn strict_enable_rejects_hash_mismatch() {
        let env = EnvGuard::new("stale");
        let _hash = hub_skill("demo", "one");
        let binding = registered_project(&env.root);
        let report = enable_project_skills_strict(
            &binding,
            "codex",
            &[("demo".into(), "not-the-hash".into())],
        )
        .unwrap();
        assert!(!report.committed);
        assert_eq!(report.items[0].status, StrictSkillStatus::Stale);
        assert!(!binding.root.join(".agents/skills/demo").exists());
    }

    #[test]
    fn strict_enable_rejects_copy_mode_without_converting() {
        let env = EnvGuard::new("copy");
        let hash = hub_skill("demo", "body");
        let binding = registered_project(&env.root);
        let mut list = load_skills_list(binding.name.as_deref().unwrap()).unwrap_or_default();
        list.deploy_modes
            .insert(".agents/skills".into(), ProjectDeployMode::Copy);
        list.agents.insert("codex".into(), Vec::new());
        save_skills_list(binding.name.as_deref().unwrap(), &list).unwrap();
        let before = manifest_bytes(binding.name.as_deref().unwrap());
        let report =
            enable_project_skills_strict(&binding, "codex", &[("demo".into(), hash)]).unwrap();
        assert!(!report.committed);
        assert_eq!(report.items[0].status, StrictSkillStatus::RejectedCopy);
        assert_eq!(manifest_bytes(binding.name.as_deref().unwrap()), before);
        let saved = load_skills_list(binding.name.as_deref().unwrap()).unwrap();
        assert_eq!(
            saved.deploy_modes.get(".agents/skills"),
            Some(&ProjectDeployMode::Copy)
        );
    }

    #[test]
    fn strict_enable_does_not_replace_a_real_directory() {
        let env = EnvGuard::new("dir");
        let hash = hub_skill("demo", "body");
        let binding = registered_project(&env.root);
        let target = binding.root.join(".agents/skills/demo");
        fs::create_dir_all(&target).unwrap();
        fs::write(target.join("LOCAL.md"), "keep").unwrap();
        let report =
            enable_project_skills_strict(&binding, "codex", &[("demo".into(), hash)]).unwrap();
        assert_eq!(report.items[0].status, StrictSkillStatus::Conflict);
        assert_eq!(fs::read_to_string(target.join("LOCAL.md")).unwrap(), "keep");
        assert!(!skillstar_core::infra::fs_ops::is_link(&target));
    }

    #[test]
    fn strict_enable_does_not_retarget_an_existing_symlink() {
        let env = EnvGuard::new("retarget");
        let hash = hub_skill("demo", "body");
        let binding = registered_project(&env.root);
        let elsewhere = env.root.join("elsewhere");
        fs::create_dir_all(&elsewhere).unwrap();
        let target = binding.root.join(".agents/skills/demo");
        fs::create_dir_all(target.parent().unwrap()).unwrap();
        #[cfg(unix)]
        std::os::unix::fs::symlink(&elsewhere, &target).unwrap();
        #[cfg(windows)]
        junction::create(&elsewhere, &target).unwrap();
        let report =
            enable_project_skills_strict(&binding, "codex", &[("demo".into(), hash)]).unwrap();
        assert_eq!(report.items[0].status, StrictSkillStatus::Conflict);
        assert!(!report.committed);
    }

    #[test]
    fn strict_enable_is_idempotent_when_the_link_is_already_correct() {
        let env = EnvGuard::new("already");
        let hash = hub_skill("demo", "body");
        let binding = registered_project(&env.root);
        let first =
            enable_project_skills_strict(&binding, "codex", &[("demo".into(), hash.clone())])
                .unwrap();
        assert!(first.committed);
        let second =
            enable_project_skills_strict(&binding, "codex", &[("demo".into(), hash)]).unwrap();
        assert!(second.committed);
        assert_eq!(second.items[0].status, StrictSkillStatus::Already);
        let list = load_skills_list(binding.name.as_deref().unwrap()).unwrap();
        assert_eq!(list.agents.get("codex").unwrap(), &vec!["demo".to_string()]);
    }

    #[test]
    fn strict_enable_uses_one_owner_for_agents_skills() {
        let env = EnvGuard::new("owner");
        let hash = hub_skill("demo", "body");
        let binding = registered_project(&env.root);
        let mut list = load_skills_list(binding.name.as_deref().unwrap()).unwrap_or_default();
        list.agents.insert("opencode".into(), vec!["old".into()]);
        save_skills_list(binding.name.as_deref().unwrap(), &list).unwrap();
        enable_project_skills_strict(&binding, "codex", &[("demo".into(), hash)]).unwrap();
        let list = load_skills_list(binding.name.as_deref().unwrap()).unwrap();
        let owners: Vec<_> = list
            .agents
            .iter()
            .filter(|(id, names)| {
                names.iter().any(|name| name == "demo")
                    && matches!(id.as_str(), "codex" | "opencode")
            })
            .map(|(id, _)| id.as_str())
            .collect();
        assert_eq!(owners, ["opencode"]);
    }

    #[test]
    fn strict_symlink_failure_does_not_fall_back_to_copy() {
        let env = EnvGuard::new("fail-link");
        let hash = hub_skill("demo", "body");
        let binding = registered_project(&env.root);
        fs::write(binding.root.join(".agents"), "not-a-directory").unwrap();
        let err = enable_project_skills_strict(&binding, "codex", &[("demo".into(), hash)]);
        assert!(err.is_err());
        assert!(binding.root.join(".agents").is_file());
        assert!(!binding.root.join(".agents/skills/demo/SKILL.md").exists());
    }

    #[test]
    fn strict_enable_rejects_a_link_parent_outside_the_project() {
        let env = EnvGuard::new("escape");
        let hash = hub_skill("demo", "body");
        let binding = registered_project(&env.root);
        let outside = env.root.join("outside");
        fs::create_dir_all(&outside).unwrap();
        #[cfg(unix)]
        std::os::unix::fs::symlink(&outside, binding.root.join(".agents")).unwrap();
        #[cfg(windows)]
        junction::create(&outside, binding.root.join(".agents")).unwrap();
        let before = manifest_bytes(binding.name.as_deref().unwrap());
        let err = enable_project_skills_strict(&binding, "codex", &[("demo".into(), hash)]);
        assert!(err.is_err(), "{err:?}");
        assert_eq!(manifest_bytes(binding.name.as_deref().unwrap()), before);
        assert!(!outside.join("skills/demo").exists());
    }
}
