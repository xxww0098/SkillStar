//! Immutable project-skill deployment plans.
//!
//! A plan is written only for an explicit selection. Scores, expiry, and the
//! plan id are stored beside the hash and are not inputs to it.

use std::path::PathBuf;

use anyhow::{Context, Result};
use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use skillstar_skills::content::{self, SNAPSHOT_HASH_VERSION};
use uuid::Uuid;

const HASH_DOMAIN: &[u8] = b"skillstar.project-skill-plan.v1\0";
const TTL: Duration = Duration::minutes(15);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PlanAction {
    Create,
    Already,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlanSkill {
    pub name: String,
    pub content_hash: String,
    pub action: PlanAction,
}

#[derive(Debug, Clone)]
pub struct PlanDraft {
    pub root: PathBuf,
    pub will_register: bool,
    pub agent_ids: Vec<String>,
    pub skills: Vec<PlanSkill>,
    pub physical_rel: String,
    pub owner_id: String,
    pub affected_agents: Vec<String>,
    pub scores: Vec<f32>,
    pub reranker: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DeploymentPlan {
    pub plan_id: String,
    pub plan_hash: String,
    pub root: String,
    pub will_register: bool,
    pub agent_id: String,
    pub skills: Vec<PlanSkill>,
    pub physical_rel: String,
    pub owner_id: String,
    pub affected_agents: Vec<String>,
    pub scores: Vec<f32>,
    pub reranker: String,
    pub created_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
}

pub fn create_plan(draft: PlanDraft, now: DateTime<Utc>) -> Result<Option<DeploymentPlan>> {
    if draft.skills.is_empty() {
        return Ok(None);
    }
    if draft.agent_ids.len() != 1 {
        anyhow::bail!("a plan has exactly one agent");
    }
    if draft.skills.len() > 8 {
        anyhow::bail!("a plan has at most 8 skills");
    }
    let agent_id = draft.agent_ids[0].clone();
    let profile_ok = skillstar_skills::agents::list_profiles()
        .iter()
        .any(|profile| profile.id == agent_id && profile.has_project_skills());
    if !profile_ok {
        anyhow::bail!("agent {agent_id} has no project skills");
    }
    for skill in &draft.skills {
        content::validate_skill_name(&skill.name)?;
    }
    let root = draft
        .root
        .to_str()
        .context("project root is not utf-8")?
        .to_string();
    let mut skills = draft.skills;
    skills.sort_by(|left, right| left.name.cmp(&right.name));
    let mut affected = draft.affected_agents;
    affected.sort();
    affected.dedup();
    let plan_hash = plan_hash(
        &root,
        draft.will_register,
        &skills,
        &draft.physical_rel,
        &draft.owner_id,
        &affected,
    );
    let plan = DeploymentPlan {
        plan_id: Uuid::new_v4().to_string(),
        plan_hash,
        root,
        will_register: draft.will_register,
        agent_id,
        skills,
        physical_rel: draft.physical_rel,
        owner_id: draft.owner_id,
        affected_agents: affected,
        scores: draft.scores,
        reranker: draft.reranker,
        created_at: now,
        expires_at: now + TTL,
    };
    write_plan(&plan)?;
    Ok(Some(plan))
}

pub fn load_plan(plan_id: &str, now: DateTime<Utc>) -> Result<DeploymentPlan> {
    let path = plan_path(plan_id)?;
    let bytes = std::fs::read(&path).with_context(|| format!("read {}", path.display()))?;
    let plan: DeploymentPlan = serde_json::from_slice(&bytes).context("parse deployment plan")?;
    if plan.plan_id != plan_id {
        anyhow::bail!("plan id does not match its file");
    }
    if now >= plan.expires_at {
        anyhow::bail!("deployment plan expired");
    }
    let expected = plan_hash(
        &plan.root,
        plan.will_register,
        &plan.skills,
        &plan.physical_rel,
        &plan.owner_id,
        &plan.affected_agents,
    );
    if expected != plan.plan_hash {
        anyhow::bail!("deployment plan hash does not match its contents");
    }
    Ok(plan)
}

/// Live, unexpired plans whose root is the same directory as `project_path`.
pub fn pending_plans_for_root(
    project_path: &str,
    now: DateTime<Utc>,
) -> Result<Vec<DeploymentPlan>> {
    let dir = skillstar_core::infra::paths::state_dir().join("project-skill-plans");
    let Ok(entries) = std::fs::read_dir(&dir) else {
        return Ok(Vec::new());
    };
    let mut plans = Vec::new();
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        let Some(plan_id) = name.strip_suffix(".json") else {
            continue;
        };
        if plan_id.ends_with(".tmp") {
            continue;
        }
        let Ok(plan) = load_plan(plan_id, now) else {
            continue;
        };
        if same_directory(&plan.root, project_path) {
            plans.push(plan);
        }
    }
    plans.sort_by(|left, right| left.plan_id.cmp(&right.plan_id));
    Ok(plans)
}

fn same_directory(plan_root: &str, project_path: &str) -> bool {
    match (
        std::fs::canonicalize(plan_root),
        std::fs::canonicalize(project_path),
    ) {
        (Ok(plan), Ok(project)) => plan == project,
        _ => plan_root == project_path,
    }
}

fn plan_hash(
    root: &str,
    will_register: bool,
    skills: &[PlanSkill],
    physical_rel: &str,
    owner_id: &str,
    affected_agents: &[String],
) -> String {
    let mut hasher = Sha256::new();
    hasher.update(HASH_DOMAIN);
    feed(&mut hasher, root.as_bytes());
    hasher.update([u8::from(will_register)]);
    for skill in skills {
        feed(&mut hasher, skill.name.as_bytes());
        feed(&mut hasher, skill.content_hash.as_bytes());
        hasher.update(SNAPSHOT_HASH_VERSION.to_le_bytes());
    }
    feed(&mut hasher, physical_rel.as_bytes());
    feed(&mut hasher, owner_id.as_bytes());
    for agent in affected_agents {
        feed(&mut hasher, agent.as_bytes());
    }
    for skill in skills {
        let action = match skill.action {
            PlanAction::Create => "create",
            PlanAction::Already => "already",
        };
        feed(&mut hasher, action.as_bytes());
    }
    hex_encode(hasher.finalize().as_ref())
}

fn hex_encode(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut encoded = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        encoded.push(HEX[(byte >> 4) as usize] as char);
        encoded.push(HEX[(byte & 0xf) as usize] as char);
    }
    encoded
}

fn feed(hasher: &mut Sha256, bytes: &[u8]) {
    hasher.update((bytes.len() as u32).to_le_bytes());
    hasher.update(bytes);
    hasher.update([0]);
}

fn write_plan(plan: &DeploymentPlan) -> Result<()> {
    let path = plan_path(&plan.plan_id)?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let tmp = path.with_extension("json.tmp");
    let bytes = serde_json::to_vec_pretty(plan).context("serialize deployment plan")?;
    std::fs::write(&tmp, &bytes).with_context(|| format!("write {}", tmp.display()))?;
    std::fs::rename(&tmp, &path).with_context(|| format!("rename {}", path.display()))?;
    Ok(())
}

fn plan_path(plan_id: &str) -> Result<PathBuf> {
    if plan_id.is_empty()
        || plan_id.contains(['/', '\\'])
        || !plan_id
            .chars()
            .all(|ch| ch.is_ascii_hexdigit() || ch == '-')
    {
        anyhow::bail!("invalid plan id");
    }
    Ok(skillstar_core::infra::paths::state_dir()
        .join("project-skill-plans")
        .join(format!("{plan_id}.json")))
}

#[cfg(test)]
mod deployment_plan_tests {
    use super::{PlanAction, PlanDraft, PlanSkill, create_plan, load_plan};
    use chrono::{Duration, TimeZone, Utc};
    use std::fs;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn now() -> chrono::DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 9, 22, 3, 0, 0).unwrap()
    }

    fn skill(name: &str, action: PlanAction) -> PlanSkill {
        PlanSkill {
            name: name.to_string(),
            content_hash: format!("hash-{name}"),
            action,
        }
    }

    fn draft(skills: Vec<PlanSkill>, agents: Vec<&str>) -> PlanDraft {
        PlanDraft {
            root: PathBuf::from("/tmp/demo"),
            will_register: true,
            agent_ids: agents.into_iter().map(str::to_string).collect(),
            skills,
            physical_rel: ".agents/skills".into(),
            owner_id: "codex".into(),
            affected_agents: vec!["deepseek".into(), "codex".into()],
            scores: vec![0.9],
            reranker: "passthrough".into(),
        }
    }

    struct EnvGuard {
        root: PathBuf,
        home: Option<std::ffi::OsString>,
        data: Option<std::ffi::OsString>,
        _lock: tokio::sync::MutexGuard<'static, ()>,
    }

    impl EnvGuard {
        fn new() -> Self {
            let _lock = futures::executor::block_on(crate::test_support::ENV_LOCK.lock());
            let nanos = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos();
            let root = std::env::temp_dir().join(format!("skillstar-plan-{nanos}"));
            fs::create_dir_all(root.join("home")).unwrap();
            fs::create_dir_all(root.join("data")).unwrap();
            let home = std::env::var_os("HOME");
            let data = std::env::var_os("SKILLSTAR_DATA_DIR");
            unsafe {
                std::env::set_var("HOME", root.join("home"));
                std::env::set_var("SKILLSTAR_DATA_DIR", root.join("data"));
            }
            Self {
                root,
                home,
                data,
                _lock,
            }
        }
    }

    impl Drop for EnvGuard {
        fn drop(&mut self) {
            unsafe {
                match self.home.take() {
                    Some(value) => std::env::set_var("HOME", value),
                    None => std::env::remove_var("HOME"),
                }
                match self.data.take() {
                    Some(value) => std::env::set_var("SKILLSTAR_DATA_DIR", value),
                    None => std::env::remove_var("SKILLSTAR_DATA_DIR"),
                }
            }
            let _ = fs::remove_dir_all(&self.root);
        }
    }

    #[test]
    fn plan_requires_explicit_selection() {
        let _env = EnvGuard::new();
        let created = create_plan(draft(Vec::new(), vec!["codex"]), now()).unwrap();
        assert!(created.is_none());
        let dir = _env.root.join("data/state/project-skill-plans");
        assert!(!dir.exists() || fs::read_dir(&dir).unwrap().next().is_none());
    }

    #[test]
    fn plan_hash_ignores_scores_and_expiry() {
        let _env = EnvGuard::new();
        let mut first_draft = draft(vec![skill("demo", PlanAction::Create)], vec!["codex"]);
        first_draft.scores = vec![0.1];
        first_draft.reranker = "passthrough".into();
        let mut second_draft = draft(vec![skill("demo", PlanAction::Create)], vec!["codex"]);
        second_draft.scores = vec![0.99];
        second_draft.reranker = "ort".into();
        let first = create_plan(first_draft, now()).unwrap().unwrap();
        let second = create_plan(second_draft, now() + Duration::minutes(3))
            .unwrap()
            .unwrap();
        assert_eq!(first.plan_hash, second.plan_hash);
        assert_ne!(first.expires_at, second.expires_at);
        assert_ne!(first.scores, second.scores);
    }

    #[test]
    fn expired_plan_is_rejected_without_rewriting_it() {
        let _env = EnvGuard::new();
        let plan = create_plan(
            draft(vec![skill("demo", PlanAction::Create)], vec!["codex"]),
            now(),
        )
        .unwrap()
        .unwrap();
        let path = skillstar_core::infra::paths::state_dir()
            .join("project-skill-plans")
            .join(format!("{}.json", plan.plan_id));
        let before = fs::read(&path).unwrap();
        let err = load_plan(&plan.plan_id, now() + Duration::minutes(16)).unwrap_err();
        assert!(err.to_string().contains("expired"), "{err}");
        assert_eq!(fs::read(&path).unwrap(), before);
    }

    #[test]
    fn plan_store_follows_skillstar_data_dir() {
        let env = EnvGuard::new();
        let plan = create_plan(
            draft(vec![skill("demo", PlanAction::Already)], vec!["codex"]),
            now(),
        )
        .unwrap()
        .unwrap();
        let path = env
            .root
            .join("data/state/project-skill-plans")
            .join(format!("{}.json", plan.plan_id));
        assert!(path.is_file(), "{}", path.display());
        let loaded = load_plan(&plan.plan_id, now()).unwrap();
        assert_eq!(loaded.plan_hash, plan.plan_hash);
    }

    #[test]
    fn plan_rejects_more_than_one_agent_or_more_than_eight_skills() {
        let _env = EnvGuard::new();
        let two_agents = create_plan(
            draft(
                vec![skill("demo", PlanAction::Create)],
                vec!["codex", "opencode"],
            ),
            now(),
        );
        assert!(two_agents.is_err());
        let nine: Vec<_> = (0..9)
            .map(|index| skill(&format!("skill-{index}"), PlanAction::Create))
            .collect();
        let too_many = create_plan(draft(nine, vec!["codex"]), now());
        assert!(too_many.is_err());
    }
}
