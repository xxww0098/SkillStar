//! Built-in agent definitions (the data table) and their `AgentSpec` impl.

use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use super::spec::AgentSpec;

// ── Built-in agent definitions (data table) ─────────────────────────
// (id, display_name, icon, home_subdirs, project_skills_rel)
//
// Adding a new agent requires only one line here. `project_skills_rel` of ""
// marks a global-only agent (no project-level skills). Antigravity shares the
// ~/.gemini home root with Gemini but owns a distinct `.agents/skills` project
// path, so the two never collide — that relationship is expressed purely as
// data in this table, with no code special-case anywhere.
const BUILTIN_AGENT_DEFS: &[(&str, &str, &str, &[&str], &str)] = &[
    (
        "opencode",
        "OpenCode",
        "agents/opencode.svg",
        &[".config", "opencode", "skills"],
        ".opencode/skills",
    ),
    (
        "claude",
        "Claude Code",
        "agents/claude.svg",
        &[".claude", "skills"],
        ".claude/skills",
    ),
    (
        "codex",
        "Codex CLI",
        "agents/codex.svg",
        &[".codex", "skills"],
        ".codex/skills",
    ),
    (
        "antigravity",
        "Antigravity",
        "agents/antigravity.svg",
        &[".gemini", "antigravity", "skills"],
        ".agents/skills",
    ),
    (
        "gemini",
        "Gemini CLI",
        "agents/gemini.svg",
        &[".gemini", "skills"],
        ".gemini/skills",
    ),
    (
        "cursor",
        "Cursor",
        "agents/cursor.svg",
        &[".cursor", "skills"],
        ".cursor/skills",
    ),
    (
        "qoder",
        "Qoder",
        "agents/qoder-color.svg",
        &[".qoder", "skills"],
        ".qoder/skills",
    ),
    (
        "trae",
        "Trae",
        "agents/trae-color.svg",
        &[".trae", "skills"],
        ".trae/skills",
    ),
    (
        "openclaw",
        "OpenClaw",
        "agents/openclaw.svg",
        &[".openclaw", "skills"],
        "",
    ),
    (
        "hermes",
        "Hermes",
        "agents/hermes.svg",
        &[".hermes", "skills"],
        ".hermes/skills",
    ),
];

/// Owned form of a built-in agent definition, cached once.
pub(crate) struct BuiltinAgentData {
    pub id: String,
    pub display_name: String,
    pub icon: String,
    pub subdirs: &'static [&'static str],
    pub project_skills_rel: String,
}

/// The built-in agent table, materialized once into owned structs.
pub(crate) fn builtin_agent_data() -> &'static [BuiltinAgentData] {
    static CACHED: OnceLock<Vec<BuiltinAgentData>> = OnceLock::new();
    CACHED.get_or_init(|| {
        BUILTIN_AGENT_DEFS
            .iter()
            .map(|(id, name, icon, subdirs, rel)| BuiltinAgentData {
                id: (*id).to_string(),
                display_name: (*name).to_string(),
                icon: (*icon).to_string(),
                subdirs,
                project_skills_rel: (*rel).to_string(),
            })
            .collect()
    })
}

/// `AgentSpec` view over one built-in agent.
pub(crate) struct BuiltinSpec(pub &'static BuiltinAgentData);

impl AgentSpec for BuiltinSpec {
    fn id(&self) -> &str {
        &self.0.id
    }
    fn display_name(&self) -> &str {
        &self.0.display_name
    }
    fn icon(&self) -> &str {
        &self.0.icon
    }
    fn resolve_global_dir(&self, home: &Path) -> PathBuf {
        let mut dir = home.to_path_buf();
        dir.extend(self.0.subdirs);
        dir
    }
    fn project_skills_rel(&self) -> Option<&str> {
        let rel = self.0.project_skills_rel.as_str();
        if rel.is_empty() { None } else { Some(rel) }
    }
}
