//! The `AgentSpec` trait: the static identity of one agent.
//!
//! Built-in agents (driven by the `BUILTIN_AGENT_DEFS` table) and user-defined
//! custom agents both implement this single trait, so every consumer can treat
//! them uniformly — there is no `if id == "antigravity"` branching anywhere.

use std::path::{Path, PathBuf};

/// The static description of an agent: where its skills live and how it routes
/// project-level skills. Implemented by both built-in and custom agents.
pub(crate) trait AgentSpec {
    fn id(&self) -> &str;
    fn display_name(&self) -> &str;
    fn icon(&self) -> &str;

    /// Resolve the absolute global skills directory given the user's home dir.
    ///
    /// Built-in agents join fixed home-relative subdir segments
    /// (`home.extend([".claude", "skills"])`); custom agents expand a `~/`
    /// prefix or use a literal path. Keeping the resolution behind this method
    /// means the two strategies never get mixed up.
    fn resolve_global_dir(&self, home: &Path) -> PathBuf;

    /// Project-level skills path relative to a project root, e.g.
    /// `".claude/skills"`. `None` means the agent is global-only (OpenClaw) —
    /// it is excluded from all project-level operations.
    fn project_skills_rel(&self) -> Option<&str>;
}
