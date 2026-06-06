//! ACP (Agent Client Protocol) integration for SkillStar.
//!
//! Launches an external Agent (Claude Code / OpenCode / Codex) as a subprocess
//! and sends it a task to analyse a skill repo and generate a working setup
//! script.  The agent does ALL the heavy lifting.
#![allow(dead_code)]

mod client;
mod runner;

#[cfg(test)]
mod tests;

#[allow(unused_imports)]
pub use client::*;
#[allow(unused_imports)]
pub use runner::*;
