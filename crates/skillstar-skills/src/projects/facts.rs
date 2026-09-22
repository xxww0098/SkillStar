//! Direct facts about project skill directories.
//!
//! One physical relative path is one row. The manifest and the directory's
//! immediate children are the only sources. Project source files are not read.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use skillstar_core::infra::{fs_ops, paths as fs_paths};

use super::binding::ObservedProject;
use super::owner::shared_path_owner;
use super::store::load_skills_list;
use super::types::{ProjectDeployMode, SkillsList};
use crate::agents::{self, AgentProfile};
use crate::content::validate_skill_name;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SkillDiskKind {
    Missing,
    Symlink,
    Junction,
    Directory,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkillPresence {
    pub name: String,
    pub in_manifest: bool,
    pub disk: SkillDiskKind,
    pub link_target: Option<PathBuf>,
    pub hub_present: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PhysicalSkillRow {
    pub project_skills_rel: String,
    pub owner_id: Option<String>,
    pub deploy_mode: Option<ProjectDeployMode>,
    pub readers: Vec<String>,
    pub skills: Vec<SkillPresence>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectSkillFacts {
    pub rows: Vec<PhysicalSkillRow>,
}

pub fn inspect_project_skills(binding: &ObservedProject) -> ProjectSkillFacts {
    let skills_list = binding
        .name
        .as_deref()
        .and_then(load_skills_list)
        .unwrap_or_default();
    let profiles = agents::list_profiles();
    let mut seen = BTreeSet::new();
    let mut rows = Vec::new();
    for profile in &profiles {
        if !profile.has_project_skills() {
            continue;
        }
        if !seen.insert(profile.project_skills_rel.clone()) {
            continue;
        }
        if let Some(row) = row_for(&binding.root, profile, &profiles, &skills_list) {
            rows.push(row);
        }
    }
    ProjectSkillFacts { rows }
}

fn row_for(
    root: &Path,
    profile: &AgentProfile,
    profiles: &[AgentProfile],
    skills_list: &SkillsList,
) -> Option<PhysicalSkillRow> {
    let rel = profile.project_skills_rel.clone();
    let directory = root.join(&rel);
    let decision = shared_path_owner(profiles, skills_list, &rel, "");
    let manifest_names = manifest_names(profiles, skills_list, &rel);
    let mut names = manifest_names.clone();
    if let Ok(entries) = std::fs::read_dir(&directory) {
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            if validate_skill_name(&name).is_err() {
                continue;
            }
            let path = entry.path();
            if skill_like(&path) {
                names.insert(name);
            }
        }
    }
    if names.is_empty() && !directory.is_dir() {
        return None;
    }
    let mut skills: Vec<SkillPresence> = names
        .into_iter()
        .map(|name| presence(&directory, &name, manifest_names.contains(&name)))
        .collect();
    skills.sort_by(|left, right| left.name.cmp(&right.name));
    Some(PhysicalSkillRow {
        project_skills_rel: rel.clone(),
        owner_id: decision.owner_id,
        deploy_mode: skills_list.deploy_modes.get(&rel).copied(),
        readers: decision.readers,
        skills,
    })
}

fn manifest_names(
    profiles: &[AgentProfile],
    skills_list: &SkillsList,
    rel: &str,
) -> BTreeSet<String> {
    let mut names = BTreeSet::new();
    for profile in profiles {
        if profile.project_skills_rel != rel {
            continue;
        }
        if let Some(skills) = skills_list.agents.get(&profile.id) {
            names.extend(skills.iter().cloned());
        }
    }
    names
}

fn presence(directory: &Path, name: &str, in_manifest: bool) -> SkillPresence {
    let path = directory.join(name);
    let (disk, link_target) = classify(&path);
    SkillPresence {
        name: name.to_string(),
        in_manifest,
        disk,
        link_target,
        hub_present: fs_paths::hub_skills_dir().join(name).exists(),
    }
}

fn skill_like(path: &Path) -> bool {
    !matches!(classify(path).0, SkillDiskKind::Missing)
}

fn classify(path: &Path) -> (SkillDiskKind, Option<PathBuf>) {
    #[cfg(windows)]
    if junction::exists(path).unwrap_or(false) && !path.is_symlink() {
        return (
            SkillDiskKind::Junction,
            fs_ops::read_link_resolved(path).ok(),
        );
    }
    if fs_ops::is_link(path) {
        return (
            SkillDiskKind::Symlink,
            fs_ops::read_link_resolved(path).ok(),
        );
    }
    if path.is_dir() {
        return (SkillDiskKind::Directory, None);
    }
    (SkillDiskKind::Missing, None)
}

#[cfg(test)]
mod project_skill_facts_tests {
    use super::{SkillDiskKind, inspect_project_skills};
    use crate::projects::observe_project;
    use skillstar_core::infra::paths as fs_paths;
    use std::fs;
    use std::path::PathBuf;
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
                .map(|duration| duration.as_nanos())
                .unwrap_or(0);
            let root = std::env::temp_dir().join(format!("skillstar-facts-{label}-{nanos}"));
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

    #[test]
    fn facts_collapse_shared_agents_skills_to_one_physical_row() {
        let env = EnvGuard::new("shared");
        let project = env.root.join("project");
        fs::create_dir_all(project.join(".agents/skills/demo")).unwrap();
        let observed = observe_project(project.to_str().unwrap()).unwrap();
        let facts = inspect_project_skills(&observed);
        let rows: Vec<_> = facts
            .rows
            .iter()
            .filter(|row| row.project_skills_rel == ".agents/skills")
            .collect();
        assert_eq!(rows.len(), 1);
        assert!(rows[0].readers.iter().any(|id| id == "deepseek"));
        assert_eq!(rows[0].skills.len(), 1);
        assert_eq!(rows[0].skills[0].name, "demo");
    }

    #[test]
    fn facts_distinguish_manifest_only_disk_only_and_foreign_symlink() {
        let env = EnvGuard::new("kinds");
        let project = env.root.join("project");
        let skill_root = project.join(".agents/skills");
        fs::create_dir_all(skill_root.join("on-disk")).unwrap();
        fs::create_dir_all(skill_root.join("disk-only")).unwrap();
        let elsewhere = env.root.join("elsewhere");
        fs::create_dir_all(&elsewhere).unwrap();
        #[cfg(unix)]
        std::os::unix::fs::symlink(&elsewhere, skill_root.join("foreign")).unwrap();
        #[cfg(windows)]
        junction::create(&elsewhere, skill_root.join("foreign")).unwrap();

        let entry = crate::projects::register_project(project.to_str().unwrap()).unwrap();
        let mut list = crate::projects::load_skills_list(&entry.name).unwrap_or_default();
        list.agents.insert(
            "codex".into(),
            vec!["manifest-only".into(), "on-disk".into()],
        );
        crate::projects::save_skills_list(&entry.name, &list).unwrap();

        let observed = observe_project(project.to_str().unwrap()).unwrap();
        let facts = inspect_project_skills(&observed);
        let row = facts
            .rows
            .iter()
            .find(|row| row.project_skills_rel == ".agents/skills")
            .unwrap();
        let kind = |name: &str| row.skills.iter().find(|skill| skill.name == name).unwrap();
        let manifest_only = kind("manifest-only");
        assert!(manifest_only.in_manifest);
        assert_eq!(manifest_only.disk, SkillDiskKind::Missing);
        let on_disk = kind("on-disk");
        assert!(on_disk.in_manifest);
        assert_eq!(on_disk.disk, SkillDiskKind::Directory);
        let disk_only = kind("disk-only");
        assert!(!disk_only.in_manifest);
        assert_eq!(disk_only.disk, SkillDiskKind::Directory);
        let foreign = kind("foreign");
        assert!(!foreign.in_manifest);
        assert!(matches!(
            foreign.disk,
            SkillDiskKind::Symlink | SkillDiskKind::Junction
        ));
        assert!(foreign.link_target.is_some());
    }

    #[test]
    fn facts_do_not_list_files_outside_project_skill_dirs() {
        let env = EnvGuard::new("outside");
        let project = env.root.join("project");
        fs::create_dir_all(&project).unwrap();
        fs::write(project.join("SECRET.txt"), "secret").unwrap();
        fs::create_dir_all(project.join(".agents/skills")).unwrap();
        fs::write(project.join(".agents/skills/notes.txt"), "not a skill").unwrap();
        fs::create_dir_all(project.join(".agents/skills/demo")).unwrap();
        let observed = observe_project(project.to_str().unwrap()).unwrap();
        let facts = inspect_project_skills(&observed);
        let rendered = format!("{facts:?}");
        assert!(!rendered.contains("SECRET"));
        assert!(!rendered.contains("notes.txt"));
        assert!(rendered.contains("demo"));
    }

    #[test]
    fn facts_do_not_register_a_project() {
        let env = EnvGuard::new("noreg");
        let project = env.root.join("project");
        fs::create_dir_all(&project).unwrap();
        let observed = observe_project(project.to_str().unwrap()).unwrap();
        let facts = inspect_project_skills(&observed);
        assert!(facts.rows.is_empty());
        assert!(observed.name.is_none());
        assert!(!fs_paths::projects_manifest_path().exists());
    }
}
