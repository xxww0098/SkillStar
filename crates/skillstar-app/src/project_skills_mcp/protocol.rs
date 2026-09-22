//! Tool arguments for the project-skills stdio server.
//!
//! Approval is not a tool parameter. This module calls recommend, inspect,
//! and apply. Form elicitation is the only path that records an elicitation
//! approval, and only after the client returns the current plan.

use std::time::Duration;

use chrono::Utc;
use rmcp::ServerHandler;
use rmcp::handler::server::router::tool::ToolRouter;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::{
    CallToolResult, ClientConfig, ContentBlock, Implementation, ProtocolVersion,
    ServerCapabilities, ServerConfig,
};
use rmcp::schemars::JsonSchema;
use rmcp::service::{ElicitationError, ElicitationMode, RequestContext, ServiceError};
use rmcp::{Peer, RoleServer, tool, tool_handler, tool_router};
use serde::{Deserialize, Serialize};
use skillstar_skills::projects::{ProjectDeployMode, SkillDiskKind};

use super::apply::{self, ApplyOutcome, Receipt};
use super::approval;
use super::inspect::{self, LoadHint, RuntimeVisibility};
use super::ort_cpu::active_reranker;
use super::plan::{self, DeploymentPlan, PlanAction};
use super::recommend::{self, RecommendRequest, Selection};

const SUPPORTED_PROTOCOL: &[ProtocolVersion] = &[ProtocolVersion::V_2026_07_28];

#[derive(Clone)]
pub(crate) struct ProjectSkillsMcp {
    tool_router: ToolRouter<Self>,
}

impl ProjectSkillsMcp {
    pub(crate) fn new() -> Self {
        Self {
            tool_router: Self::tool_router(),
        }
    }
}

#[tool_router]
impl ProjectSkillsMcp {
    /// Search installed skills. A plan is written only when `selection` is set.
    #[tool(
        name = "recommend_project_skills",
        description = "Search installed skills for a project. An explicit selection writes an unapproved deployment plan."
    )]
    async fn recommend_project_skills(
        &self,
        Parameters(args): Parameters<RecommendArgs>,
    ) -> CallToolResult {
        run_recommend(args)
    }

    /// Read project skill links. This session has not loaded them.
    #[tool(
        name = "get_project_skills",
        description = "Read project skill links. This session has not verified that the agent loaded them."
    )]
    async fn get_project_skills(&self, Parameters(args): Parameters<GetArgs>) -> CallToolResult {
        run_get(args)
    }

    /// Apply a plan that was approved outside this tool's arguments.
    #[tool(
        name = "apply_project_skills",
        description = "Apply an approved deployment plan. Approval is not a tool argument."
    )]
    async fn apply_project_skills(
        &self,
        context: RequestContext<RoleServer>,
        Parameters(args): Parameters<ApplyArgs>,
    ) -> CallToolResult {
        adopt_request_client(&context);
        run_apply(context.peer.clone(), args).await
    }
}

#[tool_handler(router = self.tool_router)]
impl ServerHandler for ProjectSkillsMcp {
    fn get_info(&self) -> ServerConfig {
        ServerConfig::new(ServerCapabilities::builder().enable_tools().build())
            .with_server_info(Implementation::new("skillstar", env!("CARGO_PKG_VERSION")))
            .with_protocol_version(ProtocolVersion::V_2026_07_28)
            .with_instructions(
                "SkillStar project skills. Recommend, read, and apply project skill links. Approval is not a tool argument. This session does not verify that an agent loaded a skill.",
            )
    }

    fn supported_protocol_versions(&self) -> std::borrow::Cow<'static, [ProtocolVersion]> {
        std::borrow::Cow::Borrowed(SUPPORTED_PROTOCOL)
    }
}

/// `2026-07-28` has no initialize session. Form elicitation still reads the
/// peer record, so copy this request's client capabilities there first.
fn adopt_request_client(context: &RequestContext<RoleServer>) {
    let Some(capabilities) = context.client_capabilities() else {
        return;
    };
    let client_info = context
        .client_info()
        .unwrap_or_else(|| Implementation::new("client", "0"));
    let version = context
        .protocol_version()
        .unwrap_or(ProtocolVersion::V_2026_07_28);
    context
        .peer
        .set_peer_info(ClientConfig::new(capabilities, client_info).with_protocol_version(version));
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct RecommendArgs {
    project_path: String,
    query: String,
    #[serde(default)]
    constraints: Vec<String>,
    #[serde(default)]
    focus_paths: Vec<String>,
    #[serde(default)]
    selection: Option<SelectionArgs>,
    catalog_scope: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct SelectionArgs {
    agent_id: String,
    skill_names: Vec<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct GetArgs {
    project_path: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct ApplyArgs {
    plan_id: String,
    idempotency_key: String,
}

#[derive(Debug, Serialize)]
struct RecommendToolResult {
    candidates: Vec<CandidateView>,
    approval: &'static str,
    blocked: Option<String>,
    plan: Option<PlanView>,
}

#[derive(Debug, Serialize)]
struct CandidateView {
    name: String,
    description: String,
    score: f32,
}

#[derive(Debug, Serialize)]
struct PlanView {
    plan_id: String,
    plan_hash: String,
    root: String,
    will_register: bool,
    agent_id: String,
    owner_id: String,
    physical_rel: String,
    affected_agents: Vec<String>,
    skills: Vec<PlanSkillView>,
}

#[derive(Debug, Serialize)]
struct PlanSkillView {
    name: String,
    action: PlanAction,
    skill_path: String,
}

#[derive(Debug, Serialize)]
struct GetToolResult {
    registered: bool,
    runtime_visibility: RuntimeVisibility,
    load_hints: Vec<LoadHint>,
    rows: Vec<RowView>,
}

#[derive(Debug, Serialize)]
struct RowView {
    project_skills_rel: String,
    owner_id: Option<String>,
    deploy_mode: Option<ProjectDeployMode>,
    readers: Vec<String>,
    skills: Vec<SkillView>,
}

#[derive(Debug, Serialize)]
struct SkillView {
    name: String,
    in_manifest: bool,
    disk: SkillDiskKind,
    hub_present: bool,
}

#[derive(Debug, Serialize)]
struct ApplyToolResult {
    outcome: &'static str,
    receipt: Option<Receipt>,
}

fn run_recommend(args: RecommendArgs) -> CallToolResult {
    let request = RecommendRequest {
        project_path: args.project_path,
        query: args.query,
        constraints: args.constraints,
        focus_paths: args.focus_paths,
        catalog_scope: args.catalog_scope,
        selection: args.selection.map(|selection| Selection {
            agent_id: selection.agent_id,
            skill_names: selection.skill_names,
        }),
    };
    let recommendation =
        match recommend::recommend_project_skills(request, &active_reranker(), Utc::now()) {
            Ok(recommendation) => recommendation,
            Err(err) => return failed(err),
        };
    let result = RecommendToolResult {
        candidates: recommendation
            .candidates
            .into_iter()
            .map(|candidate| CandidateView {
                name: candidate.name,
                description: candidate.description,
                score: candidate.score,
            })
            .collect(),
        approval: "absent",
        blocked: recommendation.blocked,
        plan: recommendation.plan.map(|plan| PlanView {
            skills: plan
                .skills
                .iter()
                .map(|skill| PlanSkillView {
                    skill_path: format!("{}/{}/SKILL.md", plan.physical_rel, skill.name)
                        .replace('\\', "/"),
                    name: skill.name.clone(),
                    action: skill.action,
                })
                .collect(),
            plan_id: plan.plan_id,
            plan_hash: plan.plan_hash,
            root: plan.root,
            will_register: plan.will_register,
            agent_id: plan.agent_id,
            owner_id: plan.owner_id,
            physical_rel: plan.physical_rel,
            affected_agents: plan.affected_agents,
        }),
    };
    let text = recommend_text(&result);
    respond(text, &result)
}

fn run_get(args: GetArgs) -> CallToolResult {
    let view = match inspect::get_project_skills(&args.project_path) {
        Ok(view) => view,
        Err(err) => return failed(err),
    };
    let result = GetToolResult {
        registered: view.registered,
        runtime_visibility: view.runtime_visibility,
        load_hints: view.load_hints,
        rows: view
            .facts
            .rows
            .into_iter()
            .map(|row| RowView {
                skills: row
                    .skills
                    .into_iter()
                    .map(|skill| SkillView {
                        name: skill.name,
                        in_manifest: skill.in_manifest,
                        disk: skill.disk,
                        hub_present: skill.hub_present,
                    })
                    .collect(),
                project_skills_rel: row.project_skills_rel,
                owner_id: row.owner_id,
                deploy_mode: row.deploy_mode,
                readers: row.readers,
            })
            .collect(),
    };
    let count: usize = result.rows.iter().map(|row| row.skills.len()).sum();
    respond(
        format!("{count} project skill(s). This session has not verified they are loaded."),
        &result,
    )
}

#[derive(Debug, Deserialize, Serialize, JsonSchema, PartialEq, Eq)]
struct PlanAcceptance {
    plan_hash: String,
    root: String,
    will_register: bool,
    owner_id: String,
    affected_agents: String,
    changes: String,
}

rmcp::elicit_safe!(PlanAcceptance);

async fn run_apply(peer: Peer<RoleServer>, args: ApplyArgs) -> CallToolResult {
    if peer
        .supported_elicitation_modes()
        .contains(&ElicitationMode::Form)
    {
        return run_apply_with_elicitation(peer, args).await;
    }
    run_apply_direct(args)
}

async fn run_apply_with_elicitation(peer: Peer<RoleServer>, args: ApplyArgs) -> CallToolResult {
    if !apply::valid_key(&args.idempotency_key) {
        return CallToolResult::error(vec![ContentBlock::text(
            "idempotency key must match [A-Za-z0-9_-]{1,64}",
        )]);
    }
    let plan = match plan::load_plan(&args.plan_id, Utc::now()) {
        Ok(plan) => plan,
        Err(err) => return failed(err),
    };
    let expected = plan_acceptance(&plan);
    let accepted = match peer
        .elicit_with_timeout::<PlanAcceptance>(
            confirmation_message(&expected),
            Some(Duration::from_secs(120)),
        )
        .await
    {
        Ok(Some(accepted)) => accepted,
        Ok(None) => return declined(),
        Err(err) if elicitation_refused(&err) => return declined(),
        Err(err) => {
            return CallToolResult::error(vec![ContentBlock::text(err.to_string())]);
        }
    };
    if accepted != expected {
        return CallToolResult::error(vec![ContentBlock::text(
            "elicitation acceptance does not match the plan",
        )]);
    }
    if let Err(err) = approval::record_from_elicitation(&plan.plan_id, &plan.plan_hash) {
        return failed(err);
    }
    run_apply_direct(args)
}

fn plan_acceptance(plan: &DeploymentPlan) -> PlanAcceptance {
    PlanAcceptance {
        changes: plan
            .skills
            .iter()
            .map(|skill| {
                let action = match skill.action {
                    PlanAction::Create => "create",
                    PlanAction::Already => "already",
                };
                let path =
                    format!("{}/{}/SKILL.md", plan.physical_rel, skill.name).replace('\\', "/");
                format!("{action} {path}")
            })
            .collect::<Vec<_>>()
            .join("\n"),
        affected_agents: plan.affected_agents.join(", "),
        plan_hash: plan.plan_hash.clone(),
        root: plan.root.clone(),
        will_register: plan.will_register,
        owner_id: plan.owner_id.clone(),
    }
}

pub(crate) fn plan_confirmation(plan: &DeploymentPlan) -> String {
    confirmation_message(&plan_acceptance(plan))
}

fn confirmation_message(acceptance: &PlanAcceptance) -> String {
    format!(
        "Confirm this project skill deployment.\nroot: {root}\nwill_register: {will}\nowner: {owner}\naffected_agents: {affected}\nchanges:\n{changes}\nplan_hash: {hash}\nSubmit these values unchanged.",
        root = acceptance.root,
        will = acceptance.will_register,
        owner = acceptance.owner_id,
        affected = acceptance.affected_agents,
        changes = acceptance.changes,
        hash = acceptance.plan_hash,
    )
}

fn elicitation_refused(err: &ElicitationError) -> bool {
    matches!(
        err,
        ElicitationError::UserDeclined
            | ElicitationError::UserCancelled
            | ElicitationError::NoContent
            | ElicitationError::Service(ServiceError::Timeout { .. })
    )
}

fn declined() -> CallToolResult {
    respond(
        "Declined. Nothing was written.",
        &ApplyToolResult {
            outcome: "declined",
            receipt: None,
        },
    )
}

fn run_apply_direct(args: ApplyArgs) -> CallToolResult {
    let outcome =
        match apply::apply_project_skills(&args.plan_id, &args.idempotency_key, Utc::now()) {
            Ok(outcome) => outcome,
            Err(err) => return failed(err),
        };
    let (label, receipt, text) = match outcome {
        ApplyOutcome::ApprovalRequired => (
            "approval_required",
            None,
            "Approval required. Nothing was written.".to_string(),
        ),
        ApplyOutcome::Applied(receipt) => (
            "applied",
            Some(receipt),
            "Applied. This session has not verified the skills are loaded.".to_string(),
        ),
        ApplyOutcome::Partial(receipt) => (
            "partial",
            Some(receipt),
            "Partially applied. This session has not verified the skills are loaded.".to_string(),
        ),
    };
    respond(
        text,
        &ApplyToolResult {
            outcome: label,
            receipt,
        },
    )
}

fn recommend_text(result: &RecommendToolResult) -> String {
    let count = result.candidates.len();
    if let Some(blocked) = &result.blocked {
        return format!("{count} candidate(s). Blocked: {blocked}.");
    }
    if let Some(plan) = &result.plan {
        return format!("{count} candidate(s). Unapproved plan {}.", plan.plan_id);
    }
    format!("{count} candidate(s). No deployment plan.")
}

fn respond(text: impl Into<String>, value: &impl Serialize) -> CallToolResult {
    match serde_json::to_value(value) {
        Ok(value) => {
            let mut result = CallToolResult::structured(value);
            result.content = vec![ContentBlock::text(text)];
            result
        }
        Err(err) => CallToolResult::error(vec![ContentBlock::text(err.to_string())]),
    }
}

fn failed(err: anyhow::Error) -> CallToolResult {
    CallToolResult::error(vec![ContentBlock::text(err.to_string())])
}

#[cfg(test)]
#[path = "protocol_tests.rs"]
mod project_skills_mcp_protocol_tests;

#[cfg(test)]
#[path = "elicitation_tests.rs"]
mod project_skills_mcp_elicitation_tests;
