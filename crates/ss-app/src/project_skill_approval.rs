//! Thin facade forwarding `ss-mcp` project-skill plan inspection and
//! desktop approval without adding a `ss-gui-bridge -> ss-mcp` dependency.

use std::path::Path;

use anyhow::Result;
pub use ss_mcp::{
    ApprovalRecord, ApprovalSource, PendingPlanOperation, PendingProjectSkillPlan,
    plan::PlanAction,
};

/// Read the latest unexpired, unapplied project-skill plan summary for `project_root`.
pub fn pending_project_skill_plan(
    project_root: &Path,
) -> Result<Option<PendingProjectSkillPlan>> {
    ss_mcp::pending_project_skill_plan(project_root)
}

/// Verify `plan_id` matches `plan_hash` and record a SkillStar desktop approval
/// without deploying skills or registering the project.
pub fn approve_project_skill_plan(plan_id: &str, plan_hash: &str) -> Result<ApprovalRecord> {
    ss_mcp::approve_project_skill_plan(plan_id, plan_hash)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{Duration, Utc};
    use ss_mcp::plan::{PlanDraft, PlanSkill, create_plan};

    #[tokio::test]
    async fn pending_and_approve_round_trip_without_deploying() {
        let _guard = crate::test_support::ENV_LOCK.lock().await;
        let tmp = tempfile::tempdir().unwrap();
        let data_dir = tmp.path().join("data");
        let home_dir = tmp.path().join("home");
        let project = tmp.path().join("project");
        std::fs::create_dir_all(&data_dir).unwrap();
        std::fs::create_dir_all(&home_dir).unwrap();
        std::fs::create_dir_all(&project).unwrap();

        let prev_home = std::env::var_os("HOME");
        let prev_data = std::env::var_os("SKILLSTAR_DATA_DIR");
        unsafe {
            std::env::set_var("HOME", &home_dir);
            std::env::set_var("SKILLSTAR_DATA_DIR", &data_dir);
        }

        let canonical = std::fs::canonicalize(&project).unwrap();
        assert!(pending_project_skill_plan(&project).unwrap().is_none());

        let now = Utc::now();
        let created = create_plan(
            PlanDraft {
                root: canonical.clone(),
                will_register: true,
                agent_ids: vec!["codex".into()],
                skills: vec![PlanSkill {
                    name: "demo".into(),
                    content_hash: "sha256:abc".into(),
                    action: PlanAction::Create,
                }],
                physical_rel: ".agents/skills".into(),
                owner_id: "codex".into(),
                affected_agents: vec!["codex".into()],
                scores: vec![1.0],
                reranker: "passthrough".into(),
            },
            now,
        )
        .unwrap()
        .unwrap();

        let pending = pending_project_skill_plan(&project)
            .unwrap()
            .expect("pending plan should exist");
        assert_eq!(pending.plan_id, created.plan_id);
        assert_eq!(pending.plan_hash, created.plan_hash);
        assert!(!pending.approved);
        assert_eq!(pending.operations.len(), 1);
        assert_eq!(pending.operations[0].name, "demo");
        assert_eq!(pending.operations[0].action, PlanAction::Create);

        let approval =
            approve_project_skill_plan(&pending.plan_id, &pending.plan_hash).unwrap();
        assert_eq!(approval.plan_id, pending.plan_id);
        assert_eq!(approval.source, ApprovalSource::Skillstar);
        assert!(!project.join(".agents").exists());

        let after = pending_project_skill_plan(&project)
            .unwrap()
            .expect("plan remains visible until applied");
        assert!(after.approved);
        assert_eq!(after.approval_source, Some(ApprovalSource::Skillstar));

        let expired_root = tmp.path().join("other");
        std::fs::create_dir_all(&expired_root).unwrap();
        assert!(pending_project_skill_plan(&expired_root).unwrap().is_none());

        let err = approve_project_skill_plan(&pending.plan_id, "wrong-hash").unwrap_err();
        assert!(err.to_string().contains("hash mismatch"), "{err}");

        // Restore env before dropping guard or tempdir.
        unsafe {
            match prev_home {
                Some(v) => std::env::set_var("HOME", v),
                None => std::env::remove_var("HOME"),
            }
            match prev_data {
                Some(v) => std::env::set_var("SKILLSTAR_DATA_DIR", v),
                None => std::env::remove_var("SKILLSTAR_DATA_DIR"),
            }
        }
        let _ = now + Duration::minutes(1);
    }
}
