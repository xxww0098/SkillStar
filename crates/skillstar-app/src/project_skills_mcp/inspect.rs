//! Read project skill facts and a load hint. This does not register or deploy.

use anyhow::Result;
use skillstar_skills::projects::{ProjectSkillFacts, inspect_project_skills, observe_project};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuntimeVisibility {
    Unverified,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoadHint {
    pub skill_path: String,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectSkillsView {
    pub registered: bool,
    pub facts: ProjectSkillFacts,
    pub runtime_visibility: RuntimeVisibility,
    pub load_hints: Vec<LoadHint>,
}

pub fn get_project_skills(project_path: &str) -> Result<ProjectSkillsView> {
    let observed = observe_project(project_path)?;
    let facts = inspect_project_skills(&observed);
    let load_hints = hints(&facts);
    Ok(ProjectSkillsView {
        registered: observed.name.is_some(),
        facts,
        runtime_visibility: RuntimeVisibility::Unverified,
        load_hints,
    })
}

fn hints(facts: &ProjectSkillFacts) -> Vec<LoadHint> {
    let mut hints = Vec::new();
    for row in &facts.rows {
        for skill in &row.skills {
            let skill_path =
                format!("{}/{}/SKILL.md", row.project_skills_rel, skill.name).replace('\\', "/");
            hints.push(LoadHint {
                message: format!("{skill_path}：当前会话未验证，需要该 Agent 自己重新发现"),
                skill_path,
            });
        }
    }
    hints
}

#[cfg(test)]
mod get_project_skills_tests {
    use super::{RuntimeVisibility, get_project_skills};
    use skillstar_core::infra::paths as fs_paths;
    use skillstar_skills::projects::{inspect_project_skills, observe_project};
    use std::fs;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    struct EnvGuard {
        root: PathBuf,
        home: Option<std::ffi::OsString>,
        data: Option<std::ffi::OsString>,
        _lock: tokio::sync::MutexGuard<'static, ()>,
    }

    impl EnvGuard {
        fn new(label: &str) -> Self {
            let _lock = futures::executor::block_on(crate::test_support::ENV_LOCK.lock());
            let nanos = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos();
            let root = std::env::temp_dir().join(format!("skillstar-get-{label}-{nanos}"));
            fs::create_dir_all(root.join("home")).unwrap();
            fs::create_dir_all(root.join("data")).unwrap();
            let guard = Self {
                home: std::env::var_os("HOME"),
                data: std::env::var_os("SKILLSTAR_DATA_DIR"),
                root,
                _lock,
            };
            unsafe {
                std::env::set_var("HOME", guard.root.join("home"));
                std::env::set_var("SKILLSTAR_DATA_DIR", guard.root.join("data"));
            }
            guard
        }
    }

    impl Drop for EnvGuard {
        fn drop(&mut self) {
            unsafe {
                restore("HOME", self.home.take());
                restore("SKILLSTAR_DATA_DIR", self.data.take());
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

    #[test]
    fn get_unregistered_project_writes_nothing() {
        let env = EnvGuard::new("unreg");
        let project = env.root.join("project");
        fs::create_dir_all(&project).unwrap();
        let view = get_project_skills(project.to_str().unwrap()).unwrap();
        assert!(!view.registered);
        assert!(view.facts.rows.is_empty());
        assert!(!fs_paths::projects_manifest_path().exists());
    }

    #[test]
    fn get_runtime_visibility_is_unverified() {
        let env = EnvGuard::new("vis");
        let project = env.root.join("project");
        fs::create_dir_all(&project).unwrap();
        let view = get_project_skills(project.to_str().unwrap()).unwrap();
        assert_eq!(view.runtime_visibility, RuntimeVisibility::Unverified);
    }

    #[test]
    fn get_uses_inspect_project_skills() {
        let env = EnvGuard::new("inspect");
        let project = env.root.join("project");
        fs::create_dir_all(project.join(".agents/skills/demo")).unwrap();
        let path = project.to_str().unwrap();
        let view = get_project_skills(path).unwrap();
        let observed = observe_project(path).unwrap();
        assert_eq!(view.facts, inspect_project_skills(&observed));
        assert_eq!(
            view.facts
                .rows
                .iter()
                .filter(|row| row.project_skills_rel == ".agents/skills")
                .count(),
            1
        );
    }

    #[test]
    fn get_load_hint_names_the_physical_rel() {
        let env = EnvGuard::new("hint");
        let project = env.root.join("project");
        fs::create_dir_all(project.join(".agents/skills/demo")).unwrap();
        let view = get_project_skills(project.to_str().unwrap()).unwrap();
        let hint = view
            .load_hints
            .iter()
            .find(|hint| hint.skill_path.contains(".agents/skills/demo/SKILL.md"))
            .expect("load hint");
        assert!(hint.message.contains("当前会话未验证"));
        assert!(!hint.skill_path.contains('\\'));
        assert!(
            !hint
                .message
                .contains(&fs_paths::hub_skills_dir().to_string_lossy().to_string())
        );
    }
}
