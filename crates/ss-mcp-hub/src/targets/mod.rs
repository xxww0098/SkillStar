pub mod paths;
mod paths_ext;
pub mod project_targets;

pub use paths::{
    BuiltInTargetSpec, McpPathRoots, builtin_targets, canonical_agent_id, detect_agent_targets,
    resolve_target_spec,
};
pub use project_targets::{ProjectTargetSpec, build_project_snippets, project_targets};
