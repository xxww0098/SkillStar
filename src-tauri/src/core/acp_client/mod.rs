//! ACP (Agent Client Protocol) integration for SkillStar.
//!
//! Launches an external Agent (Claude Code / OpenCode / Codex) as a subprocess
//! and drives a read-only conversation about a skill repo: the agent may read
//! files under the session's working directory and nothing else.
mod client;
mod runner;

#[cfg(test)]
mod tests;

pub use runner::run_read_only_conversation_via_acp;
