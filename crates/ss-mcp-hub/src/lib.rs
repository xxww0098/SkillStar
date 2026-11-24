//! External MCP server management, 3-layer lossless agent config synchronization,
//! health/tool probing, and built-in MCP store (`featured` + embedded snapshot + MCP Registry).
//!
//! Aligned with magpie's MCP agents excluding Gemini CLI (35 agents; `agy` converged to `antigravity`).

pub mod edit;
pub mod env_refs;
pub mod error;
pub mod formats;
pub mod library;
pub mod model;
pub mod probe;
pub mod store;
pub mod targets;

pub use edit::file::{BackupEntry, BackupRestoreResult, list_backups, restore_backup};
pub use error::{McpHubError, McpHubResult};
pub use formats::{ParsedPasteResult, parse_pasted_config};
pub use library::{
    batch_remove_servers, batch_set_paused, get_server, get_snapshot, import_found_server,
    read_library_unlocked as read_library_unlocked_wrapper, remove_found_server, remove_server,
    remove_server_delta, save_server, save_server_delta, save_servers_batch,
    set_server_agents_delta, set_server_paused, set_server_paused_delta, sync_all,
    toggle_server_agent, toggle_server_agent_delta,
};
pub use model::{
    AppliedAgentLedger, CheckResult, ConflictStrategy, DiscoveredTool, FoundServer, LibraryDelta,
    LibraryFile, LibrarySnapshot, McpAgentTarget, McpFormat, McpServer, ProjectSnippet,
    StoreInputSpec, StoreInstallRequest, StoreItem, StoreSearchResponse, SyncWarning, Transport,
};
pub use probe::check_server;
pub use store::{install_store_server, install_store_server_delta, query_store};
pub use targets::{
    McpPathRoots, ProjectTargetSpec, builtin_targets, canonical_agent_id, detect_agent_targets,
    project_targets, project_targets::build_project_snippets,
};
