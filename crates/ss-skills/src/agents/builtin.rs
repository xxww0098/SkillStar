//! Built-in Agent definitions for the skills CLI ecosystem.

use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use super::spec::AgentSpec;

#[derive(Clone, Copy)]
enum GlobalRoot {
    Home,
    Config,
    EnvOrHome(&'static str, &'static [&'static str]),
    OpenClaw,
    Reasonix,
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

const fn reasonix() -> GlobalDirDef {
    GlobalDirDef {
        root: GlobalRoot::Reasonix,
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
// their persisted identity. CLI/API normalization accepts the standard
// ids the CLIs publish. Every other row uses the standard id verbatim.
// `omp`, `deepseek` and `workbuddy` are SkillStar extensions kept after
// the shared block.
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
        // Codex standardized its global skills on the shared `~/.agents/skills`
        // location; `~/.codex/skills` is the legacy path it no longer documents.
        // Deployments to the canonical root are skipped, so this row is a
        // no-op consumer of the canonical copy.
        home(&[".agents", "skills"]),
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
        // Droid reads `.agents/skills` and `~/.agents/skills` as
        // compatibility locations; the global dir stays on
        // `~/.factory/skills` so removal still cleans up skills placed there
        // by earlier versions.
        home(&[".factory", "skills"]),
        ".agents/skills",
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
        "fx",
        "fx",
        // fx natively discovers these shared roots alongside its own `.fx/skills` roots.
        home(&[".agents", "skills"]),
        ".agents/skills",
    ),
    // Google's Gemini CLI — `~/.gemini`. Deliberately *not* the `antigravity`
    // row above: that one is Google Antigravity, a different product that
    // merely shares the `~/.gemini` prefix, so it cannot stand in for this
    // profile.
    (
        "gemini-cli",
        "Gemini CLI",
        home(&[".gemini", "skills"]),
        ".agents/skills",
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
        "grok",
        "Grok Build",
        env_or_home("GROK_HOME", &[".grok"], &["skills"]),
        ".grok/skills",
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
    // Kilo keeps `~/.kilocode` as the legacy config dir for installed
    // detection only; skills live in `~/.kilo/skills` and the shared
    // `.agents/skills` project path.
    (
        "kilo",
        "Kilo Code",
        home(&[".kilo", "skills"]),
        ".agents/skills",
    ),
    // Kimchi resolves this under a literal `~/.config` prefix, not XDG.
    (
        "kimchi",
        "Kimchi",
        home(&[".config", "kimchi", "harness", "skills"]),
        ".kimchi/skills",
    ),
    (
        "kimi-code-cli",
        "Kimi Code CLI",
        home(&[".agents", "skills"]),
        ".agents/skills",
    ),
    (
        "kiro",
        "Kiro CLI",
        home(&[".kiro", "skills"]),
        ".kiro/skills",
    ),
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
        "minimax-code",
        "MiniMax Code",
        home(&[".minimax", "skills"]),
        ".minimax/skills",
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
    // Pi natively reads the open-skills locations, so both of its rows are
    // the shared `~/.agents/skills` and `.agents/skills` paths.
    ("pi", "Pi", home(&[".agents", "skills"]), ".agents/skills"),
    (
        "posit-assistant",
        "Posit Assistant",
        home(&[".posit", "assistant", "skills"]),
        ".posit/assistant/skills",
    ),
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
        reasonix(),
        ".reasonix/skills",
    ),
    ("roo", "Roo Code", home(&[".roo", "skills"]), ".roo/skills"),
    (
        "sarvam-code",
        "Sarvam Code",
        home(&[".agents", "skills"]),
        ".agents/skills",
    ),
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
        "devin-desktop",
        "Devin Desktop",
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
    (
        "omp",
        "Oh My Pi",
        home(&[".omp", "agent", "skills"]),
        ".omp/skills",
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
    // WorkBuddy (Tencent) — SkillStar extension. Global
    // ~/.workbuddy/skills, project .workbuddy/skills.
    (
        "workbuddy",
        "WorkBuddy",
        home(&[".workbuddy", "skills"]),
        ".workbuddy/skills",
    ),
];

/// Agents that read a project skill directory besides their own
/// `project_skills_rel`. Deployment still writes only the selected path.
///
/// DeepSeek's own directory is `.dsh/skills`, and it also reads `.agents/skills`.
pub fn additional_project_skill_reads() -> &'static [(&'static str, &'static str)] {
    &[("deepseek", ".agents/skills")]
}

/// Home-relative directories that must receive the same global deployments as
/// an Agent's own `global_skills_dir`.
///
/// Antigravity supports multiple runtime surfaces — the desktop app (`antigravity`),
/// the CLI (`antigravity-cli`), and the ecosystem-wide shared directory (`~/.gemini/skills`).
/// One SkillStar profile (`antigravity`) serves all of them, keeping
/// `global_skills_dir` (`~/.gemini/antigravity/skills`) as the bookkeeping source of truth
/// and fanning out deployments to the CLI and shared directories through `GLOBAL_MIRROR_DEFS`.
const GLOBAL_MIRROR_DEFS: &[(&str, &[&[&str]])] = &[(
    "antigravity",
    &[
        &[".gemini", "antigravity-cli", "skills"],
        &[".gemini", "skills"],
        &[".gemini", "config", "skills"],
        &[".gemini", "antigravity-ide", "skills"],
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

fn non_blank_path(value: Option<&OsStr>) -> Option<PathBuf> {
    let value = value?;
    match value.to_str() {
        Some(text) => {
            let trimmed = text.trim();
            (!trimmed.is_empty()).then(|| PathBuf::from(trimmed))
        }
        None => (!value.is_empty()).then(|| PathBuf::from(value)),
    }
}

fn env_or_home_path(variable: &str, home: &Path, fallback: &[&str]) -> PathBuf {
    let env_val = std::env::var_os(variable);
    non_blank_path(env_val.as_deref()).unwrap_or_else(|| {
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

pub(crate) fn resolve_reasonix_skills_dir(
    home: &Path,
    reasonix_home: Option<&OsStr>,
    appdata: Option<&OsStr>,
    is_windows: bool,
) -> PathBuf {
    if let Some(custom) = non_blank_path(reasonix_home) {
        return custom.join("skills");
    }
    if is_windows {
        let roaming =
            non_blank_path(appdata).unwrap_or_else(|| home.join("AppData").join("Roaming"));
        return roaming.join("reasonix").join("skills");
    }
    home.join(".reasonix").join("skills")
}

fn reasonix_skills_dir(home: &Path) -> PathBuf {
    let reasonix_home = std::env::var_os("REASONIX_HOME");
    let appdata = std::env::var_os("APPDATA");
    resolve_reasonix_skills_dir(
        home,
        reasonix_home.as_deref(),
        appdata.as_deref(),
        cfg!(windows),
    )
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
            GlobalRoot::Reasonix => return reasonix_skills_dir(home),
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
#[path = "builtin_tests.rs"]
mod tests;
