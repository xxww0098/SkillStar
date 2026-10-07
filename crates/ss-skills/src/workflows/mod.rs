//! Skill-only use cases shared by GUI and CLI adapters.
//! Marketplace resolution and other cross-domain composition stay in ss-app.

pub mod agent_links;
pub mod agent_managed_skills;
pub mod global_deploy;
pub mod skill_group_links;

#[cfg(test)]
mod workflow_tests;
