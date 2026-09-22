//! Local stdio MCP for project-skill recommendation and enablement.
//!
//! This module is not the external MCP catalog (`crate::mcp`). It owns the
//! process that agents launch with `skillstar mcp serve --stdio`.

pub mod approval;
pub mod plan;
mod stdio;

pub use stdio::{is_mcp_invocation, serve, serve_with};

#[cfg(test)]
mod mcp_stdio_tests;
