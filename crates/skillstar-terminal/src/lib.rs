//! Pure terminal helpers for SkillStar: Launch Deck config, CLI registry, and session management.
//!
//! This crate owns the pure types, config persistence, CLI detection, tree utilities,
//! script generation, and terminal launch logic.

pub mod config;
pub mod pane_command;
pub mod provider_env;
pub mod registry;
pub mod script_builder;
pub mod session;
pub mod terminal_launcher;
pub mod types;

// Re-export commonly used items
pub use config::{
    LaunchConfig, LaunchMode, LayoutNode, SplitDirection, collect_leaf_panes, count_panes,
    default_config, delete_config, deployable_layout, load_config, save_config, validate,
};
pub use pane_command::{
    PaneCommandSpec, build_posix_pane_command, pane_command_spec, shell_escape,
};
pub use registry::{binary_name_for_agent, find_cli_binary, list_available_clis};
pub use session::session_name;
pub use types::{AgentCliInfo, DeployResult, LaunchScriptKind};
