//! Local stdio MCP for project-skill recommendation and enablement.
//!
//! This module is not the external MCP catalog (`crate::mcp`). It owns the
//! process that agents launch with `skillstar mcp serve --stdio`.

pub mod apply;
pub mod approval;
pub mod cli_approve;
pub mod host;
pub mod inspect;
pub mod ort_cpu;
pub mod plan;
pub mod protocol;
pub mod ranker;
pub mod recommend;
mod stdio;

pub use cli_approve::run_approve;
pub use stdio::{is_mcp_invocation, is_mcp_serve, serve, serve_with};

#[cfg(test)]
mod mcp_stdio_tests;
