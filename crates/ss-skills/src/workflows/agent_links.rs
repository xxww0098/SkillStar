//! Per-pair outcome of linking Skills into global Agent directories.
//!
//! GUI and CLI flows that link many `(Skill, Agent)` pairs at once must not
//! collapse partial results into "done": a name collision or a failed link is
//! something the user needs to see. Every pair lands in exactly one bucket.

use serde::Serialize;

use crate::deployment::{self, SKIP_CANONICAL_ROOT_AGENT, ToggleSkillOutcome};

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentLinkIssue {
    pub agent_id: String,
    pub skill: String,
    pub reason: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentLinkReport {
    /// `(agent_id, skill)` pairs now in the requested state. An Agent that
    /// reads the canonical root directly counts as served.
    pub applied: Vec<(String, String)>,
    /// Pairs left alone on purpose, e.g. an unmanaged same-name directory.
    pub skipped: Vec<AgentLinkIssue>,
    pub failed: Vec<AgentLinkIssue>,
}

impl AgentLinkReport {
    pub fn is_clean(&self) -> bool {
        self.skipped.is_empty() && self.failed.is_empty()
    }

    pub fn served(&self, agent_id: &str) -> bool {
        self.applied.iter().any(|(agent, _)| agent == agent_id)
    }

    fn attempted(&self, agent_id: &str) -> bool {
        self.served(agent_id)
            || self
                .skipped
                .iter()
                .chain(&self.failed)
                .any(|issue| issue.agent_id == agent_id)
    }

    /// One line per problem pair, for a notice; `None` when every pair applied.
    pub fn problem_summary(&self) -> Option<String> {
        if self.is_clean() {
            return None;
        }
        let lines = self
            .failed
            .iter()
            .map(|issue| format!("{} → {}: {}", issue.skill, issue.agent_id, issue.reason))
            .chain(self.skipped.iter().map(|issue| {
                format!(
                    "{} → {}: skipped, {}",
                    issue.skill, issue.agent_id, issue.reason
                )
            }))
            .collect::<Vec<_>>();
        Some(lines.join("\n"))
    }

    pub(crate) fn link(&mut self, skill: &str, agent_id: &str, enable: bool) {
        match deployment::toggle_skill_for_agent(skill, agent_id, enable) {
            Ok(ToggleSkillOutcome::Applied) => {
                self.applied.push((agent_id.to_string(), skill.to_string()));
            }
            Ok(ToggleSkillOutcome::Skipped { code, .. }) if code == SKIP_CANONICAL_ROOT_AGENT => {
                self.applied.push((agent_id.to_string(), skill.to_string()));
            }
            Ok(ToggleSkillOutcome::Skipped { reason, .. }) => self.skipped.push(AgentLinkIssue {
                agent_id: agent_id.to_string(),
                skill: skill.to_string(),
                reason,
            }),
            Err(err) => self.failed.push(AgentLinkIssue {
                agent_id: agent_id.to_string(),
                skill: skill.to_string(),
                reason: format!("{err:#}"),
            }),
        }
    }

    /// Whether a deck rail may record this Agent's new state: nothing to link,
    /// or at least one pair landed. A rail never claims an Agent where every
    /// link failed.
    pub(crate) fn rail_may_change(&self, agent_id: &str) -> bool {
        !self.attempted(agent_id) || self.served(agent_id)
    }
}

/// Enabled Agents with a global Skills directory — the population install-time
/// deploys and the deck rail target.
pub(crate) fn enabled_global_agent_ids() -> Vec<String> {
    crate::agents::list_profiles()
        .into_iter()
        .filter(|profile| profile.enabled && profile.has_global_skills())
        .map(|profile| profile.id)
        .collect()
}
