//! Built-in Agent definitions synchronized with `vercel-labs/skills`.

use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use super::spec::AgentSpec;

#[derive(Clone, Copy)]
enum GlobalRoot {
    Home,
    Config,
    EnvOrHome(&'static str, &'static [&'static str]),
    OpenClaw,
    Unsupported,
}

#[derive(Clone, Copy)]
struct GlobalDirDef {
    root: GlobalRoot,
    subdirs: &'static [&'static str],
}

type BuiltinAgentDef = (&'static str, &'static str, GlobalDirDef, &'static str);

const fn home(subdirs: &'static [&'static str]) -> GlobalDirDef {
    GlobalDirDef {
        root: GlobalRoot::Home,
        subdirs,
    }
}

const fn config(subdirs: &'static [&'static str]) -> GlobalDirDef {
    GlobalDirDef {
        root: GlobalRoot::Config,
        subdirs,
    }
}

const fn env_or_home(
    variable: &'static str,
    fallback: &'static [&'static str],
    subdirs: &'static [&'static str],
) -> GlobalDirDef {
    GlobalDirDef {
        root: GlobalRoot::EnvOrHome(variable, fallback),
        subdirs,
    }
}

const fn openclaw() -> GlobalDirDef {
    GlobalDirDef {
        root: GlobalRoot::OpenClaw,
        subdirs: &[],
    }
}

const fn unsupported() -> GlobalDirDef {
    GlobalDirDef {
        root: GlobalRoot::Unsupported,
        subdirs: &[],
    }
}

// The three legacy SkillStar ids (`claude`, `kiro`, `hermes`) retain
// their persisted identity. CLI/API normalization accepts the corresponding
// upstream ids. Every other row uses the upstream id verbatim. `grok`,
// `omp`, `gemini-cli`, `deepseek` and `maka` are SkillStar extensions kept
// after the synchronized upstream block.
const BUILTIN_AGENT_DEFS: &[BuiltinAgentDef] = &[
    (
        "aider-desk",
        "AiderDesk",
        home(&[".aider-desk", "skills"]),
        ".aider-desk/skills",
    ),
    (
        "amp",
        "Amp",
        config(&["agents", "skills"]),
        ".agents/skills",
    ),
    // One row for all of Antigravity: the app, the CLI and the IDE are three
    // installed states of the same product, so they share a single profile and
    // fan out through `GLOBAL_MIRROR_DEFS` below.
    (
        "antigravity",
        "Antigravity",
        home(&[".gemini", "antigravity", "skills"]),
        ".agents/skills",
    ),
    (
        "astrbot",
        "AstrBot",
        home(&[".astrbot", "data", "skills"]),
        "data/skills",
    ),
    (
        "autohand-code",
        "Autohand Code CLI",
        env_or_home("AUTOHAND_HOME", &[".autohand"], &["skills"]),
        ".autohand/skills",
    ),
    (
        "augment",
        "Augment",
        home(&[".augment", "skills"]),
        ".augment/skills",
    ),
    ("bob", "IBM Bob", home(&[".bob", "skills"]), ".bob/skills"),
    (
        "claude",
        "Claude Code",
        env_or_home("CLAUDE_CONFIG_DIR", &[".claude"], &["skills"]),
        ".claude/skills",
    ),
    ("openclaw", "OpenClaw", openclaw(), "skills"),
    (
        "cline",
        "Cline",
        home(&[".agents", "skills"]),
        ".agents/skills",
    ),
    (
        "codearts-agent",
        "CodeArts Agent",
        home(&[".codeartsdoer", "skills"]),
        ".codeartsdoer/skills",
    ),
    (
        "codebuddy",
        "CodeBuddy",
        home(&[".codebuddy", "skills"]),
        ".codebuddy/skills",
    ),
    (
        "codemaker",
        "Codemaker",
        home(&[".codemaker", "skills"]),
        ".codemaker/skills",
    ),
    (
        "codestudio",
        "Code Studio",
        home(&[".codestudio", "skills"]),
        ".codestudio/skills",
    ),
    (
        "codex",
        "Codex",
        env_or_home("CODEX_HOME", &[".codex"], &["skills"]),
        ".agents/skills",
    ),
    (
        "command-code",
        "Command Code",
        home(&[".commandcode", "skills"]),
        ".commandcode/skills",
    ),
    (
        "continue",
        "Continue",
        home(&[".continue", "skills"]),
        ".continue/skills",
    ),
    (
        "cortex",
        "Cortex Code",
        home(&[".snowflake", "cortex", "skills"]),
        ".cortex/skills",
    ),
    (
        "crush",
        "Crush",
        home(&[".config", "crush", "skills"]),
        ".crush/skills",
    ),
    (
        "cursor",
        "Cursor",
        home(&[".cursor", "skills"]),
        ".agents/skills",
    ),
    (
        "deepagents",
        "Deep Agents",
        home(&[".deepagents", "agent", "skills"]),
        ".agents/skills",
    ),
    (
        "devin",
        "Devin for Terminal",
        config(&["devin", "skills"]),
        ".devin/skills",
    ),
    (
        "dexto",
        "Dexto",
        home(&[".agents", "skills"]),
        ".agents/skills",
    ),
    (
        "droid",
        "Droid",
        home(&[".factory", "skills"]),
        ".factory/skills",
    ),
    ("eve", "Eve", unsupported(), "agent/skills"),
    (
        "firebender",
        "Firebender",
        home(&[".firebender", "skills"]),
        ".agents/skills",
    ),
    (
        "forgecode",
        "ForgeCode",
        home(&[".forge", "skills"]),
        ".forge/skills",
    ),
    (
        "github-copilot",
        "GitHub Copilot",
        home(&[".copilot", "skills"]),
        ".agents/skills",
    ),
    (
        "goose",
        "Goose",
        config(&["goose", "skills"]),
        ".goose/skills",
    ),
    (
        "hermes",
        "Hermes Agent",
        env_or_home("HERMES_HOME", &[".hermes"], &["skills"]),
        ".hermes/skills",
    ),
    (
        "inference-sh",
        "inference.sh",
        home(&[".inferencesh", "skills"]),
        ".inferencesh/skills",
    ),
    ("jazz", "Jazz", home(&[".jazz", "skills"]), ".jazz/skills"),
    (
        "junie",
        "Junie",
        home(&[".junie", "skills"]),
        ".junie/skills",
    ),
    (
        "iflow-cli",
        "iFlow CLI",
        home(&[".iflow", "skills"]),
        ".iflow/skills",
    ),
    (
        "kilo",
        "Kilo Code",
        home(&[".kilocode", "skills"]),
        ".kilocode/skills",
    ),
    (
        "kimi-code-cli",
        "Kimi Code CLI",
        home(&[".agents", "skills"]),
        ".agents/skills",
    ),
    ("kiro", "Kiro", home(&[".kiro", "skills"]), ".kiro/skills"),
    ("kode", "Kode", home(&[".kode", "skills"]), ".kode/skills"),
    (
        "lingma",
        "Lingma",
        home(&[".lingma", "skills"]),
        ".lingma/skills",
    ),
    (
        "loaf",
        "Loaf",
        home(&[".agents", "skills"]),
        ".agents/skills",
    ),
    (
        "mcpjam",
        "MCPJam",
        home(&[".mcpjam", "skills"]),
        ".mcpjam/skills",
    ),
    (
        "mistral-vibe",
        "Mistral Vibe",
        env_or_home("VIBE_HOME", &[".vibe"], &["skills"]),
        ".vibe/skills",
    ),
    (
        "moxby",
        "Moxby",
        home(&[".moxby", "skills"]),
        ".moxby/skills",
    ),
    ("mux", "Mux", home(&[".mux", "skills"]), ".mux/skills"),
    (
        "neovate",
        "Neovate",
        home(&[".neovate", "skills"]),
        ".neovate/skills",
    ),
    (
        "opencode",
        "OpenCode",
        config(&["opencode", "skills"]),
        ".agents/skills",
    ),
    (
        "openhands",
        "OpenHands",
        home(&[".openhands", "skills"]),
        ".openhands/skills",
    ),
    ("ona", "Ona", home(&[".ona", "skills"]), ".ona/skills"),
    ("pi", "Pi", home(&[".pi", "agent", "skills"]), ".pi/skills"),
    (
        "qoder",
        "Qoder",
        home(&[".qoder", "skills"]),
        ".qoder/skills",
    ),
    (
        "qoder-cn",
        "Qoder CN",
        home(&[".qoder-cn", "skills"]),
        ".qoder/skills",
    ),
    (
        "qwen-code",
        "Qwen Code",
        home(&[".qwen", "skills"]),
        ".qwen/skills",
    ),
    (
        "replit",
        "Replit",
        config(&["agents", "skills"]),
        ".agents/skills",
    ),
    (
        "reasonix",
        "Reasonix",
        home(&[".reasonix", "skills"]),
        ".reasonix/skills",
    ),
    ("roo", "Roo Code", home(&[".roo", "skills"]), ".roo/skills"),
    (
        "rovodev",
        "Rovo Dev",
        home(&[".rovodev", "skills"]),
        ".rovodev/skills",
    ),
    (
        "tabnine-cli",
        "Tabnine CLI",
        home(&[".tabnine", "agent", "skills"]),
        ".tabnine/agent/skills",
    ),
    (
        "terramind",
        "Terramind",
        home(&[".terramind", "skills"]),
        ".terramind/skills",
    ),
    (
        "tinycloud",
        "Tinycloud",
        home(&[".tinycloud", "skills"]),
        ".tinycloud/skills",
    ),
    ("trae", "Trae", home(&[".trae", "skills"]), ".trae/skills"),
    (
        "trae-cn",
        "Trae CN",
        home(&[".trae-cn", "skills"]),
        ".trae/skills",
    ),
    (
        "warp",
        "Warp",
        home(&[".agents", "skills"]),
        ".agents/skills",
    ),
    (
        "windsurf",
        "Windsurf",
        home(&[".codeium", "windsurf", "skills"]),
        ".windsurf/skills",
    ),
    ("zed", "Zed", home(&[".agents", "skills"]), ".agents/skills"),
    (
        "zcode",
        "ZCode",
        home(&[".zcode", "skills"]),
        ".zcode/skills",
    ),
    (
        "zencoder",
        "Zencoder",
        home(&[".zencoder", "skills"]),
        ".zencoder/skills",
    ),
    (
        "zenflow",
        "Zenflow",
        home(&[".zencoder", "skills"]),
        ".zencoder/skills",
    ),
    (
        "pochi",
        "Pochi",
        home(&[".pochi", "skills"]),
        ".pochi/skills",
    ),
    (
        "promptscript",
        "PromptScript",
        unsupported(),
        ".agents/skills",
    ),
    ("adal", "AdaL", home(&[".adal", "skills"]), ".adal/skills"),
    (
        "universal",
        "Universal",
        config(&["agents", "skills"]),
        ".agents/skills",
    ),
    ("grok", "Grok", home(&[".grok", "skills"]), ".grok/skills"),
    (
        "omp",
        "Oh My Pi",
        home(&[".omp", "agent", "skills"]),
        ".omp/skills",
    ),
    // Google's Gemini CLI — `~/.gemini`, the same root its MCP target writes
    // (`~/.gemini/settings.json`). Deliberately *not* the `antigravity` row
    // above: that one is Google Antigravity, a different product that merely
    // shares the `~/.gemini` prefix, so it cannot stand in for this profile. Without this row the `gemini-cli` MCP target has no
    // Agent profile to hang its per-server toggle and Agent filter on
    // (`src/features/mcp/lib/agentTargets.ts`).
    (
        "gemini-cli",
        "Gemini CLI",
        home(&[".gemini", "skills"]),
        ".gemini/skills",
    ),
    // DeepSeek Harness (DSH) — the dsh CLI reads user skills from
    // ~/.dsh/skills (or $DSH_HOME/skills) and project skills from
    // .dsh/skills. DSH also reads the shared .agents/skills dir, but its
    // primary project skill root is .dsh/skills, which is this target.
    (
        "deepseek",
        "DeepSeek Harness",
        env_or_home("DSH_HOME", &[".dsh"], &["skills"]),
        ".dsh/skills",
    ),
    // Apache Maka (Incubating). Global and project skills live under
    // `.maka/skills`; Maka also reads the shared `.agents/skills` dir as a
    // lower-precedence compatibility path, so this target is the exclusive
    // SkillStar deploy surface. MCP writes the released `Maka` profile
    // (`<config>/Maka/workspaces/default/mcp.json`), not `Maka Dev`.
    ("maka", "Maka", home(&[".maka", "skills"]), ".maka/skills"),
];

/// Home-relative directories that must receive the same global deployments as
/// an Agent's own `global_skills_dir`.
///
/// Antigravity installs as three independent states — the app (`antigravity`),
/// the CLI (`antigravity-cli`) and the IDE (`antigravity-ide`) — and each state
/// reads skills only from its own `builtin/skills`. One SkillStar profile
/// therefore has to land in all three, which is what the deployment layer
/// replays after every link/unlink.
const GLOBAL_MIRROR_DEFS: &[(&str, &[&[&str]])] = &[(
    "antigravity",
    &[
        &[".gemini", "antigravity", "builtin", "skills"],
        &[".gemini", "antigravity-cli", "builtin", "skills"],
        &[".gemini", "antigravity-ide", "builtin", "skills"],
    ],
)];

/// Resolve the mirror directories for `agent_id`; empty for every Agent that
/// has a single skills directory (all of them but Antigravity today).
pub(crate) fn mirror_dirs(agent_id: &str, home: &Path) -> Vec<PathBuf> {
    GLOBAL_MIRROR_DEFS
        .iter()
        .find(|(id, _)| *id == agent_id)
        .map(|(_, dirs)| {
            dirs.iter()
                .map(|parts| {
                    parts
                        .iter()
                        .fold(home.to_path_buf(), |p, part| p.join(part))
                })
                .collect()
        })
        .unwrap_or_default()
}

pub(crate) struct BuiltinAgentData {
    pub id: String,
    pub display_name: String,
    pub icon: String,
    global: GlobalDirDef,
    pub project_skills_rel: String,
}

pub(crate) fn builtin_agent_data() -> &'static [BuiltinAgentData] {
    static CACHED: OnceLock<Vec<BuiltinAgentData>> = OnceLock::new();
    CACHED.get_or_init(|| {
        BUILTIN_AGENT_DEFS
            .iter()
            .map(|(id, name, global, rel)| BuiltinAgentData {
                id: (*id).to_string(),
                display_name: (*name).to_string(),
                icon: format!("lobe:{id}"),
                global: *global,
                project_skills_rel: (*rel).to_string(),
            })
            .collect()
    })
}

fn config_home(home: &Path) -> PathBuf {
    std::env::var_os("XDG_CONFIG_HOME")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| home.join(".config"))
}

fn env_or_home_path(variable: &str, home: &Path, fallback: &[&str]) -> PathBuf {
    std::env::var_os(variable)
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            fallback
                .iter()
                .fold(home.to_path_buf(), |p, part| p.join(part))
        })
}

fn openclaw_skills_dir(home: &Path) -> PathBuf {
    for root in [".openclaw", ".clawdbot", ".moltbot"] {
        let path = home.join(root);
        if path.exists() {
            return path.join("skills");
        }
    }
    home.join(".openclaw").join("skills")
}

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
        let mut base = match self.0.global.root {
            GlobalRoot::Home => home.to_path_buf(),
            GlobalRoot::Config => config_home(home),
            GlobalRoot::EnvOrHome(variable, fallback) => env_or_home_path(variable, home, fallback),
            GlobalRoot::OpenClaw => return openclaw_skills_dir(home),
            GlobalRoot::Unsupported => return PathBuf::new(),
        };
        base.extend(self.0.global.subdirs);
        base
    }

    fn supports_global(&self) -> bool {
        !matches!(self.0.global.root, GlobalRoot::Unsupported)
    }

    fn project_skills_rel(&self) -> Option<&str> {
        let rel = self.0.project_skills_rel.as_str();
        if rel.is_empty() { None } else { Some(rel) }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    const UPSTREAM_AGENT_IDS: &[&str] = &[
        "aider-desk",
        "amp",
        "antigravity",
        "antigravity-cli",
        "astrbot",
        "autohand-code",
        "augment",
        "bob",
        "claude-code",
        "openclaw",
        "cline",
        "codearts-agent",
        "codebuddy",
        "codemaker",
        "codestudio",
        "codex",
        "command-code",
        "continue",
        "cortex",
        "crush",
        "cursor",
        "deepagents",
        "devin",
        "dexto",
        "droid",
        "eve",
        "firebender",
        "forgecode",
        "github-copilot",
        "goose",
        "hermes-agent",
        "inference-sh",
        "jazz",
        "junie",
        "iflow-cli",
        "kilo",
        "kimi-code-cli",
        "kiro-cli",
        "kode",
        "lingma",
        "loaf",
        "mcpjam",
        "mistral-vibe",
        "moxby",
        "mux",
        "neovate",
        "opencode",
        "openhands",
        "ona",
        "pi",
        "qoder",
        "qoder-cn",
        "qwen-code",
        "replit",
        "reasonix",
        "roo",
        "rovodev",
        "tabnine-cli",
        "terramind",
        "tinycloud",
        "trae",
        "trae-cn",
        "warp",
        "windsurf",
        "zed",
        "zcode",
        "zencoder",
        "zenflow",
        "pochi",
        "promptscript",
        "adal",
        "universal",
    ];

    /// Upstream ids fold onto persisted SkillStar ids through the same alias
    /// table the CLI and IPC use, so this test cannot drift from it.
    fn skillstar_id(upstream: &str) -> &str {
        crate::agents::compatible_profile_id(upstream)
    }

    #[test]
    fn covers_every_upstream_agent_id() {
        let ids = BUILTIN_AGENT_DEFS
            .iter()
            .map(|row| row.0)
            .collect::<HashSet<_>>();
        let missing = UPSTREAM_AGENT_IDS
            .iter()
            .filter(|id| !ids.contains(skillstar_id(id)))
            .collect::<Vec<_>>();
        assert!(
            missing.is_empty(),
            "missing upstream Agent ids: {missing:?}"
        );
    }

    #[test]
    fn builtin_agent_ids_are_unique() {
        let mut seen = HashSet::new();
        for (id, ..) in BUILTIN_AGENT_DEFS {
            assert!(seen.insert(*id), "duplicate builtin Agent id: {id}");
        }
    }

    #[test]
    fn builtin_agent_fields_are_well_formed() {
        for (id, name, _global, rel) in BUILTIN_AGENT_DEFS {
            assert!(!id.is_empty(), "blank Agent id");
            assert!(!name.is_empty(), "blank display_name for {id}");
            assert!(!rel.is_empty(), "blank project skills path for {id}");
        }
    }

    #[test]
    fn antigravity_mirrors_every_installed_state() {
        let home = Path::new("/tmp/skillstar-test-home");
        let dirs = mirror_dirs("antigravity", home);
        assert_eq!(
            dirs,
            [
                home.join(".gemini/antigravity/builtin/skills"),
                home.join(".gemini/antigravity-cli/builtin/skills"),
                home.join(".gemini/antigravity-ide/builtin/skills"),
            ]
        );
        assert!(mirror_dirs("claude", home).is_empty());
        // The dropped `antigravity-cli` profile id still resolves, so persisted
        // prefs and `--agent antigravity-cli` keep reaching the single row.
        assert!(
            !mirror_dirs(
                crate::agents::compatible_profile_id("antigravity-cli"),
                home
            )
            .is_empty()
        );
    }

    #[test]
    fn project_only_agents_have_no_global_path() {
        let home = Path::new("/tmp/skillstar-test-home");
        for id in ["eve", "promptscript"] {
            let data = builtin_agent_data()
                .iter()
                .find(|agent| agent.id == id)
                .unwrap();
            let spec = BuiltinSpec(data);
            assert!(!spec.supports_global());
            assert!(spec.resolve_global_dir(home).as_os_str().is_empty());
        }
    }
}
