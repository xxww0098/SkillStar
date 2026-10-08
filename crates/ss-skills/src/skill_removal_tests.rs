use super::*;
use crate::skill_lock::{self, SkillLockEntry, SourceType};
use crate::test_sandbox::Sandbox;
use ss_core::infra::paths;

fn lock_entry() -> SkillLockEntry {
    SkillLockEntry {
        source: "o/r".into(),
        source_type: SourceType::Github,
        source_url: "https://github.com/o/r.git".into(),
        git_ref: Some("main".into()),
        skill_path: None,
        skill_folder_hash: Some("abc".into()),
        installed_at: "2026-01-01T00:00:00Z".into(),
        updated_at: "2026-01-01T00:00:00Z".into(),
        extra: Default::default(),
    }
}

fn write_lock(name: &str) {
    skill_lock::mutate(|lock| {
        lock.upsert(name, lock_entry());
    })
    .unwrap();
}

fn link_agent(sandbox: &Sandbox, name: &str, canonical: &std::path::Path) -> std::path::PathBuf {
    let agent_dir = sandbox.home().join(".claude/skills");
    std::fs::create_dir_all(&agent_dir).unwrap();
    let agent = agent_dir.join(name);
    ss_core::infra::fs_ops::create_symlink(canonical, &agent).unwrap();
    agent
}

#[cfg(unix)]
struct ModeGuard {
    path: std::path::PathBuf,
    mode: u32,
}

#[cfg(unix)]
impl ModeGuard {
    fn readonly(path: &std::path::Path) -> Self {
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(path).unwrap().permissions().mode();
        let mut perms = std::fs::metadata(path).unwrap().permissions();
        perms.set_mode(0o555);
        std::fs::set_permissions(path, perms).unwrap();
        Self {
            path: path.to_path_buf(),
            mode,
        }
    }
}

#[cfg(unix)]
impl Drop for ModeGuard {
    fn drop(&mut self) {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = std::fs::metadata(&self.path)
            .map(|metadata| metadata.permissions())
            .unwrap_or_else(|_| std::fs::Permissions::from_mode(0o755));
        perms.set_mode(self.mode);
        let _ = std::fs::set_permissions(&self.path, perms);
    }
}

#[test]
fn uninstall_is_idempotent_and_removes_an_owned_agent_link() {
    let sandbox = Sandbox::new();
    let canonical = paths::agents_skill_dir("foo");
    std::fs::create_dir_all(&canonical).unwrap();
    std::fs::write(canonical.join("SKILL.md"), "keep").unwrap();
    write_lock("foo");
    let agent = link_agent(&sandbox, "foo", &canonical);

    uninstall_skill("foo").unwrap();
    assert!(!canonical.exists());
    assert!(!skill_lock::load().skills.contains_key("foo"));
    assert!(agent.symlink_metadata().is_err());

    uninstall_skill("foo").unwrap();
    assert!(!canonical.exists());
}

#[test]
#[cfg(unix)]
fn uninstall_keeps_the_canonical_copy_when_agent_cleanup_fails() {
    let sandbox = Sandbox::new();
    let canonical = paths::agents_skill_dir("foo");
    std::fs::create_dir_all(&canonical).unwrap();
    std::fs::write(canonical.join("SKILL.md"), "keep").unwrap();
    write_lock("foo");
    let agent = link_agent(&sandbox, "foo", &canonical);
    let _mode = ModeGuard::readonly(agent.parent().unwrap());

    let error = uninstall_skill("foo").unwrap_err();
    assert!(error.contains("Failed to remove Skill"), "{error}");
    assert!(canonical.join("SKILL.md").is_file());
    assert!(skill_lock::load().skills.contains_key("foo"));
    assert!(ss_core::infra::fs_ops::is_link(&agent));
}

#[test]
fn release_local_hub_link_keeps_the_original_and_is_idempotent() {
    let sandbox = Sandbox::new();
    local_skill::create("foo", Some("---\nname: foo\ndescription: d\n---\nbody\n")).unwrap();
    write_lock("foo");
    let local = paths::local_skills_dir().join("foo/SKILL.md");
    let hub = paths::hub_skills_dir().join("foo");
    let agent = link_agent(&sandbox, "foo", &paths::local_skills_dir().join("foo"));

    release_local_hub_link("foo").unwrap();
    assert!(local.is_file());
    assert!(hub.symlink_metadata().is_err());
    assert!(!skill_lock::load().skills.contains_key("foo"));
    assert!(agent.symlink_metadata().is_err());

    release_local_hub_link("foo").unwrap();
    assert!(
        local.is_file(),
        "a second release still keeps the local original"
    );
}

#[test]
#[cfg(unix)]
fn deleting_a_local_skill_keeps_the_files_when_agent_cleanup_fails() {
    let sandbox = Sandbox::new();
    local_skill::create("foo", Some("---\nname: foo\ndescription: d\n---\nbody\n")).unwrap();
    let local = paths::local_skills_dir().join("foo/SKILL.md");
    let hub = paths::hub_skills_dir().join("foo");
    let agent = link_agent(&sandbox, "foo", &paths::local_skills_dir().join("foo"));
    let _mode = ModeGuard::readonly(agent.parent().unwrap());

    let error = local_skill::delete("foo").unwrap_err();
    assert!(
        error.to_string().contains("Failed to remove Skill"),
        "{error:#}"
    );
    assert!(local.is_file());
    assert!(ss_core::infra::fs_ops::is_link(&hub));
    assert!(ss_core::infra::fs_ops::is_link(&agent));
}

#[test]
fn channel_commit_failure_restores_the_canonical_copy_and_keeps_deployments() {
    let sandbox = Sandbox::new();
    let canonical = paths::agents_skill_dir("foo");
    std::fs::create_dir_all(&canonical).unwrap();
    std::fs::write(canonical.join("SKILL.md"), "keep").unwrap();
    write_lock("foo");
    let agent = link_agent(&sandbox, "foo", &canonical);

    let error =
        uninstall_hub_skill_with_commit("foo", || Err::<(), &str>("commit failed")).unwrap_err();
    assert!(!error.committed);
    assert!(error.rollback_complete);
    assert_eq!(
        std::fs::read_to_string(canonical.join("SKILL.md")).unwrap(),
        "keep"
    );
    assert!(skill_lock::load().skills.contains_key("foo"));
    assert!(ss_core::infra::fs_ops::is_link(&agent));
}

#[test]
#[cfg(unix)]
fn channel_deployment_cleanup_failure_stays_committed() {
    let sandbox = Sandbox::new();
    let canonical = paths::agents_skill_dir("foo");
    std::fs::create_dir_all(&canonical).unwrap();
    std::fs::write(canonical.join("SKILL.md"), "keep").unwrap();
    write_lock("foo");
    let agent = link_agent(&sandbox, "foo", &canonical);
    let _mode = ModeGuard::readonly(agent.parent().unwrap());

    let error = uninstall_hub_skill_with_commit("foo", || Ok::<(), &str>(())).unwrap_err();
    assert!(error.committed, "{error:?}");
    assert!(error.rollback_complete, "{error:?}");
    assert!(
        error.message.contains("deployment cleanup failed"),
        "{error:?}"
    );
    assert!(!canonical.exists());
    assert!(!skill_lock::load().skills.contains_key("foo"));
    assert!(ss_core::infra::fs_ops::is_link(&agent));
}
