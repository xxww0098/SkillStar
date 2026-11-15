//! Local stdio MCP for project-skill recommendation and enablement.
//!
//! Owns the process that agents launch with `skillstar mcp serve --stdio`.

pub mod apply;
pub mod approval;
pub mod cli_approve;
mod gateway;
pub mod inspect;
pub mod plan;
pub mod protocol;
pub mod ranker;
pub mod recommend;
mod stdio;

pub use cli_approve::run_approve;
pub use stdio::{is_mcp_invocation, is_mcp_serve, serve, serve_with};

#[cfg(test)]
mod mcp_stdio_tests;
#[cfg(test)]
pub(crate) mod test_support;
