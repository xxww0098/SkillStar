//! Terminal approval for an existing deployment plan.
//!
//! This is not an MCP tool and does not deploy. The diff uses the same text
//! as form elicitation.

use std::io::{BufRead, Write};

use anyhow::{Context, Result, bail};
use chrono::{DateTime, Utc};

use super::approval::record_from_skillstar;
use super::plan::load_plan;
use super::protocol::plan_confirmation;

pub fn run_approve(
    plan_id: &str,
    input: &mut dyn BufRead,
    is_terminal: bool,
    output: &mut dyn Write,
    now: DateTime<Utc>,
) -> Result<()> {
    let plan = load_plan(plan_id, now)?;
    let message = plan_confirmation(&plan);
    writeln!(output, "{message}").context("write plan diff")?;
    output.flush().context("flush plan diff")?;
    let line = read_confirmation(input, is_terminal)?;
    let expected = format!("approve {}", plan.plan_hash);
    if line != expected {
        bail!("confirmation must be `approve {}`", plan.plan_hash);
    }
    record_from_skillstar(&plan.plan_id, &plan.plan_hash)?;
    writeln!(output, "Approved {}.", plan.plan_id).context("write approval result")?;
    Ok(())
}

fn read_confirmation(input: &mut dyn BufRead, is_terminal: bool) -> Result<String> {
    let mut line = String::new();
    let read = input.read_line(&mut line).context("read confirmation")?;
    if read == 0 {
        if is_terminal {
            bail!("confirmation must be `approve <plan_hash>`");
        }
        bail!("non-interactive: pipe `approve <plan_hash>`");
    }
    Ok(line.trim_end_matches(['\n', '\r']).to_string())
}

#[cfg(test)]
mod cli_approve_tests {
    use super::run_approve;
    use crate::project_skills_mcp::approval::record_from_elicitation;
    use crate::project_skills_mcp::plan::{PlanAction, PlanDraft, PlanSkill, create_plan};
    use chrono::{TimeZone, Utc};
    use skillstar_core::infra::paths as fs_paths;
    use std::fs;
    use std::io::Cursor;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn now() -> chrono::DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 9, 22, 8, 0, 0).unwrap()
    }

    struct EnvGuard {
        root: PathBuf,
        home: Option<std::ffi::OsString>,
        data: Option<std::ffi::OsString>,
        #[cfg(windows)]
        userprofile: Option<std::ffi::OsString>,
        _lock: tokio::sync::MutexGuard<'static, ()>,
    }

    impl EnvGuard {
        fn new(label: &str) -> Self {
            let _lock = futures::executor::block_on(crate::test_support::ENV_LOCK.lock());
            let nanos = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos();
            let root = std::env::temp_dir().join(format!("skillstar-cli-approve-{label}-{nanos}"));
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

    fn plan(root: &std::path::Path) -> crate::project_skills_mcp::plan::DeploymentPlan {
        create_plan(
            PlanDraft {
                root: root.to_path_buf(),
                will_register: true,
                agent_ids: vec!["codex".into()],
                skills: vec![PlanSkill {
                    name: "demo".into(),
                    content_hash: "hash-demo".into(),
                    action: PlanAction::Create,
                }],
                physical_rel: ".agents/skills".into(),
                owner_id: "codex".into(),
                affected_agents: vec!["codex".into()],
                scores: Vec::new(),
                reranker: "passthrough".into(),
            },
            now(),
        )
        .unwrap()
        .unwrap()
    }

    fn approval_file(plan_id: &str) -> PathBuf {
        fs_paths::state_dir()
            .join("project-skill-approvals")
            .join(format!("{plan_id}.json"))
    }

    #[test]
    fn cli_approve_requires_the_exact_plan_hash_line() {
        let env = EnvGuard::new("exact");
        let project = env.root.join("project");
        fs::create_dir_all(&project).unwrap();
        let stored = plan(&project);
        let mut output = Vec::new();
        let missing = run_approve(
            &stored.plan_id,
            &mut Cursor::new(""),
            false,
            &mut output,
            now(),
        );
        assert!(missing.unwrap_err().to_string().contains("non-interactive"));
        assert!(!approval_file(&stored.plan_id).exists());

        output.clear();
        let wrong = run_approve(
            &stored.plan_id,
            &mut Cursor::new("approve deadbeef\n"),
            false,
            &mut output,
            now(),
        );
        assert!(wrong.unwrap_err().to_string().contains("approve"));
        assert!(!approval_file(&stored.plan_id).exists());
        let shown = String::from_utf8(output.clone()).unwrap();
        assert!(shown.contains(&stored.plan_hash), "{shown}");
        assert!(shown.contains(".agents/skills/demo/SKILL.md"), "{shown}");

        output.clear();
        let line = format!("approve {}\n", stored.plan_hash);
        run_approve(
            &stored.plan_id,
            &mut Cursor::new(line.into_bytes()),
            false,
            &mut output,
            now(),
        )
        .unwrap();
        let record: serde_json::Value =
            serde_json::from_slice(&fs::read(approval_file(&stored.plan_id)).unwrap()).unwrap();
        assert_eq!(record["source"], "skillstar");
        assert_eq!(record["plan_hash"], stored.plan_hash);
    }

    #[test]
    fn cli_approve_rejects_a_plan_already_approved_by_elicitation() {
        let env = EnvGuard::new("elicited");
        let project = env.root.join("project");
        fs::create_dir_all(&project).unwrap();
        let stored = plan(&project);
        record_from_elicitation(&stored.plan_id, &stored.plan_hash).unwrap();
        let line = format!("approve {}\n", stored.plan_hash);
        let err = run_approve(
            &stored.plan_id,
            &mut Cursor::new(line.into_bytes()),
            false,
            &mut Vec::new(),
            now(),
        )
        .unwrap_err();
        assert!(err.to_string().contains("another source"), "{err}");
        let record: serde_json::Value =
            serde_json::from_slice(&fs::read(approval_file(&stored.plan_id)).unwrap()).unwrap();
        assert_eq!(record["source"], "elicitation");
    }

    #[test]
    fn cli_approve_does_not_deploy() {
        let env = EnvGuard::new("no-deploy");
        let project = env.root.join("project");
        fs::create_dir_all(&project).unwrap();
        let stored = plan(&project);
        let line = format!("approve {}\n", stored.plan_hash);
        run_approve(
            &stored.plan_id,
            &mut Cursor::new(line.into_bytes()),
            false,
            &mut Vec::new(),
            now(),
        )
        .unwrap();
        assert!(!project.join(".agents").exists());
        assert!(!fs_paths::projects_manifest_path().exists());
        assert!(approval_file(&stored.plan_id).exists());
    }
}
