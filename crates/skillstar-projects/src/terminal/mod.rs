//! Terminal backend: CLI agent registry, session naming, and deploy types.

pub mod registry;
pub mod session;
pub mod types;

pub use registry::{agent_cli_entries, binary_name_for_agent, find_cli_binary, list_available_clis};
pub use session::session_name;
pub use types::{AgentCliInfo, DeployResult, LaunchScriptKind};
