//! Recommend installed skills and, only for an explicit valid selection,
//! write an unapproved deployment plan.

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use skillstar_core::infra::paths as fs_paths;
use skillstar_core::types::parse_skill_content;
use skillstar_skills::content::{self, snapshot};
use skillstar_skills::projects::{
    StrictSkillStatus, classify_project_skill, inspect_project_skills, observe_project,
    shared_path_owner,
};
use skillstar_skills::team::{installed_skill_names, search_installed_skills};
use std::collections::HashSet;

use super::plan::{DeploymentPlan, PlanAction, PlanDraft, PlanSkill, create_plan};
use super::ranker::{RankedCandidate, SkillReranker};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ApprovalState {
    Absent,
}

#[derive(Debug, Clone)]
pub struct Selection {
    pub agent_id: String,
    pub skill_names: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct RecommendRequest {
    pub project_path: String,
    pub query: String,
    pub constraints: Vec<String>,
    pub focus_paths: Vec<String>,
    pub selection: Option<Selection>,
    pub catalog_scope: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Candidate {
    pub name: String,
    pub description: String,
    pub score: f32,
}

#[derive(Debug, Clone)]
pub struct Recommendation {
    pub candidates: Vec<Candidate>,
    pub plan: Option<DeploymentPlan>,
    pub approval: ApprovalState,
    pub blocked: Option<String>,
}

pub fn recommend_project_skills(
    request: RecommendRequest,
    reranker: &dyn SkillReranker,
    now: DateTime<Utc>,
) -> Result<Recommendation> {
    validate(&request)?;
    let observed = observe_project(&request.project_path)?;
    let search_text = search_text(&request);
    let hits = search_installed_skills(&search_text, 12)?;
    let ranked = reranker.rerank(
        hits.into_iter()
            .map(|hit| RankedCandidate {
                name: hit.id,
                score: hit.score,
            })
            .collect(),
    );
    let candidates = ranked
        .iter()
        .map(|hit| Candidate {
            description: description_excerpt(&hit.name),
            name: hit.name.clone(),
            score: hit.score,
        })
        .collect();

    let Some(selection) = request.selection else {
        return Ok(Recommendation {
            candidates,
            plan: None,
            approval: ApprovalState::Absent,
            blocked: None,
        });
    };

    let installed: HashSet<String> = installed_skill_names().into_iter().collect();
    if let Some(name) = selection
        .skill_names
        .iter()
        .find(|name| !installed.contains(*name))
    {
        return Ok(Recommendation {
            candidates,
            plan: None,
            approval: ApprovalState::Absent,
            blocked: Some(format!("{name} is not installed")),
        });
    }

    let profiles = skillstar_skills::agents::list_profiles();
    let profile = profiles
        .iter()
        .find(|profile| profile.id == selection.agent_id && profile.has_project_skills())
        .context("agent has no project skills")?;
    let facts = inspect_project_skills(&observed);
    let mode = facts
        .rows
        .iter()
        .find(|row| row.project_skills_rel == profile.project_skills_rel)
        .and_then(|row| row.deploy_mode);
    let skills_list = observed
        .name
        .as_deref()
        .and_then(skillstar_skills::projects::load_skills_list)
        .unwrap_or_default();
    let owner = shared_path_owner(
        &profiles,
        &skills_list,
        &profile.project_skills_rel,
        &selection.agent_id,
    );
    let mut plan_skills = Vec::new();
    for name in &selection.skill_names {
        let hash = snapshot(name)?.content_hash;
        let status = classify_project_skill(
            &observed.root,
            &profile.project_skills_rel,
            name,
            &hash,
            mode,
        )?;
        if !matches!(
            status,
            StrictSkillStatus::Create | StrictSkillStatus::Already
        ) {
            return Ok(Recommendation {
                candidates,
                plan: None,
                approval: ApprovalState::Absent,
                blocked: Some(format!("{name} is {status:?}")),
            });
        }
        plan_skills.push(PlanSkill {
            name: name.clone(),
            content_hash: hash,
            action: match status {
                StrictSkillStatus::Already => PlanAction::Already,
                _ => PlanAction::Create,
            },
        });
    }

    let draft = PlanDraft {
        root: observed.root,
        will_register: observed.name.is_none(),
        agent_ids: vec![selection.agent_id],
        skills: plan_skills,
        physical_rel: profile.project_skills_rel.clone(),
        owner_id: owner.owner_id.unwrap_or_else(|| profile.id.clone()),
        affected_agents: owner.readers,
        scores: Vec::new(),
        reranker: reranker.name().to_string(),
    };
    let plan = create_plan(draft, now)?;
    Ok(Recommendation {
        candidates,
        plan,
        approval: ApprovalState::Absent,
        blocked: None,
    })
}

fn validate(request: &RecommendRequest) -> Result<()> {
    if request.catalog_scope != "installed" {
        anyhow::bail!("catalog_scope must be installed");
    }
    if request.query.chars().count() > 2000 {
        anyhow::bail!("query is longer than 2000 characters");
    }
    if request.constraints.len() > 20
        || request
            .constraints
            .iter()
            .any(|item| item.chars().count() > 200)
    {
        anyhow::bail!("constraints exceed 20 items or 200 characters");
    }
    if request.focus_paths.len() > 50
        || request
            .focus_paths
            .iter()
            .any(|item| item.chars().count() > 200)
    {
        anyhow::bail!("focus_paths exceed 50 items or 200 characters");
    }
    if let Some(selection) = &request.selection {
        if selection.agent_id.is_empty() || selection.skill_names.len() > 8 {
            anyhow::bail!("selection needs one agent and at most 8 skills");
        }
    }
    Ok(())
}

fn search_text(request: &RecommendRequest) -> String {
    let mut parts = Vec::with_capacity(1 + request.constraints.len() + request.focus_paths.len());
    parts.push(request.query.as_str());
    parts.extend(request.constraints.iter().map(String::as_str));
    parts.extend(request.focus_paths.iter().map(String::as_str));
    parts.join(" ")
}

fn description_excerpt(name: &str) -> String {
    let raw = content::read_raw(name).ok().or_else(|| {
        std::fs::read_to_string(fs_paths::local_skills_dir().join(name).join("SKILL.md")).ok()
    });
    raw.and_then(|raw| parse_skill_content(name.to_string(), raw).description)
        .unwrap_or_default()
}

#[cfg(test)]
mod recommend_project_skills_tests {
    use super::super::ranker::PassthroughReranker;
    use super::{ApprovalState, RecommendRequest, Selection, recommend_project_skills};
    use chrono::{TimeZone, Utc};
    use skillstar_core::infra::paths as fs_paths;
    use skillstar_skills::team::recall;
    use std::fs;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn now() -> chrono::DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 9, 22, 4, 0, 0).unwrap()
    }

    struct EnvGuard {
        root: PathBuf,
        home: Option<std::ffi::OsString>,
        data: Option<std::ffi::OsString>,
        hub: Option<std::ffi::OsString>,
        _lock: tokio::sync::MutexGuard<'static, ()>,
    }

    impl EnvGuard {
        fn new(label: &str) -> Self {
            let _lock = futures::executor::block_on(crate::test_support::ENV_LOCK.lock());
            let nanos = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos();
            let root = std::env::temp_dir().join(format!("skillstar-recommend-{label}-{nanos}"));
            fs::create_dir_all(root.join("home")).unwrap();
            fs::create_dir_all(root.join("data")).unwrap();
            let guard = Self {
                home: std::env::var_os("HOME"),
                data: std::env::var_os("SKILLSTAR_DATA_DIR"),
                hub: std::env::var_os("SKILLSTAR_HUB_DIR"),
                root,
                _lock,
            };
            unsafe {
                std::env::set_var("HOME", guard.root.join("home"));
                std::env::set_var("SKILLSTAR_DATA_DIR", guard.root.join("data"));
                std::env::remove_var("SKILLSTAR_HUB_DIR");
            }
            guard
        }
    }

    impl Drop for EnvGuard {
        fn drop(&mut self) {
            unsafe {
                restore("HOME", self.home.take());
                restore("SKILLSTAR_DATA_DIR", self.data.take());
                restore("SKILLSTAR_HUB_DIR", self.hub.take());
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

    fn write_skill(name: &str, description: &str, body: &str) {
        let dir = fs_paths::hub_skills_dir().join(name);
        fs::create_dir_all(&dir).unwrap();
        fs::write(
            dir.join("SKILL.md"),
            format!("---\nname: {name}\ndescription: {description}\n---\n\n# {name}\n\n{body}\n"),
        )
        .unwrap();
    }

    fn request(path: &str, selection: Option<Selection>) -> RecommendRequest {
        RecommendRequest {
            project_path: path.to_string(),
            query: "pull request review tests".into(),
            constraints: Vec::new(),
            focus_paths: Vec::new(),
            selection,
            catalog_scope: "installed".into(),
        }
    }

    #[test]
    fn recommend_without_selection_does_not_write_a_plan_or_register() {
        let env = EnvGuard::new("none");
        write_skill("pr-review", "Review pull requests", "Ask for tests.");
        let project = env.root.join("project");
        fs::create_dir_all(&project).unwrap();
        let recommendation = recommend_project_skills(
            request(project.to_str().unwrap(), None),
            &PassthroughReranker,
            now(),
        )
        .unwrap();
        assert!(recommendation.plan.is_none());
        assert_eq!(recommendation.approval, ApprovalState::Absent);
        assert!(!fs_paths::projects_manifest_path().exists());
        let plans = env.root.join("data/state/project-skill-plans");
        assert!(!plans.exists() || fs::read_dir(plans).unwrap().next().is_none());
    }

    #[test]
    fn recommend_selection_unknown_to_the_hub_does_not_write_a_plan() {
        let env = EnvGuard::new("unknown");
        write_skill("pr-review", "Review pull requests", "Ask for tests.");
        let project = env.root.join("project");
        fs::create_dir_all(&project).unwrap();
        let recommendation = recommend_project_skills(
            request(
                project.to_str().unwrap(),
                Some(Selection {
                    agent_id: "codex".into(),
                    skill_names: vec!["not-installed".into()],
                }),
            ),
            &PassthroughReranker,
            now(),
        )
        .unwrap();
        assert!(recommendation.plan.is_none());
        assert!(recommendation.blocked.unwrap().contains("not installed"));
        assert!(!fs_paths::projects_manifest_path().exists());
    }

    #[test]
    fn recommend_conflict_does_not_write_a_plan() {
        let env = EnvGuard::new("conflict");
        write_skill("demo", "A demo skill", "Body stays out of the excerpt.");
        let project = env.root.join("project");
        let target = project.join(".agents/skills/demo");
        fs::create_dir_all(&target).unwrap();
        fs::write(target.join("LOCAL.md"), "keep").unwrap();
        let recommendation = recommend_project_skills(
            request(
                project.to_str().unwrap(),
                Some(Selection {
                    agent_id: "codex".into(),
                    skill_names: vec!["demo".into()],
                }),
            ),
            &PassthroughReranker,
            now(),
        )
        .unwrap();
        assert!(recommendation.plan.is_none());
        assert!(recommendation.blocked.unwrap().contains("Conflict"));
        assert_eq!(fs::read_to_string(target.join("LOCAL.md")).unwrap(), "keep");
    }

    #[test]
    fn recommend_does_not_read_project_source_files() {
        let env = EnvGuard::new("secret");
        write_skill("pr-review", "Review pull requests", "Ask for tests.");
        let project = env.root.join("project");
        fs::create_dir_all(project.join(".agents/skills")).unwrap();
        let secret = project.join("SECRET.txt");
        fs::write(&secret, b"do-not-read").unwrap();
        let mut request = request(project.to_str().unwrap(), None);
        request.focus_paths = vec!["SECRET.txt".into()];
        let _ = recommend_project_skills(request, &PassthroughReranker, now()).unwrap();
        assert_eq!(fs::read(&secret).unwrap(), b"do-not-read");
    }

    #[test]
    fn recommend_keeps_bm25_order_when_ranker_is_passthrough() {
        let env = EnvGuard::new("order");
        write_skill(
            "pr-review",
            "Review pull requests and leave blocking comments",
            "Always request tests for API changes. Flag missing coverage.",
        );
        write_skill(
            "frontend-design",
            "UI layout and visual hierarchy for marketing pages",
            "Use a restrained palette. Avoid decorative gradients.",
        );
        let project = env.root.join("project");
        fs::create_dir_all(&project).unwrap();
        let recommendation = recommend_project_skills(
            request(project.to_str().unwrap(), None),
            &PassthroughReranker,
            now(),
        )
        .unwrap();
        assert_eq!(recommendation.candidates[0].name, "pr-review");
        assert!(
            recommendation.candidates[0]
                .description
                .contains("Review pull requests")
        );
        assert!(
            !recommendation.candidates[0]
                .description
                .contains("Always request")
        );
    }

    #[test]
    fn recommend_rejects_unknown_catalog_scope() {
        let env = EnvGuard::new("scope");
        let project = env.root.join("project");
        fs::create_dir_all(&project).unwrap();
        let mut request = request(project.to_str().unwrap(), None);
        request.catalog_scope = "marketplace".into();
        let err = recommend_project_skills(request, &PassthroughReranker, now()).unwrap_err();
        assert!(err.to_string().contains("installed"), "{err}");
    }

    #[test]
    fn recommend_does_not_append_recall_events() {
        let env = EnvGuard::new("recall");
        write_skill(
            "pr-review",
            "Review pull requests and leave blocking comments",
            "Always request tests for API changes.",
        );
        let project = env.root.join("project");
        fs::create_dir_all(&project).unwrap();
        let _ = recall(
            "pull request review",
            8,
            Utc.with_ymd_and_hms(2026, 9, 9, 12, 0, 0).unwrap(),
        )
        .unwrap();
        let store = fs_paths::state_dir().join("team.json");
        let before = fs::read(&store).unwrap();
        let _ = recommend_project_skills(
            request(project.to_str().unwrap(), None),
            &PassthroughReranker,
            now(),
        )
        .unwrap();
        assert_eq!(fs::read(&store).unwrap(), before);
    }
}
