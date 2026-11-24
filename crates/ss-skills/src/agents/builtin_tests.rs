use super::*;
use std::collections::HashSet;
use std::ffi::OsStr;

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
    "fx",
    "gemini-cli",
    "github-copilot",
    "goose",
    "grok",
    "hermes-agent",
    "inference-sh",
    "jazz",
    "junie",
    "iflow-cli",
    "kilo",
    "kimchi",
    "kimi-code-cli",
    "kiro-cli",
    "kode",
    "lingma",
    "loaf",
    "mcpjam",
    "minimax-code",
    "mistral-vibe",
    "moxby",
    "mux",
    "neovate",
    "opencode",
    "openhands",
    "ona",
    "pi",
    "posit-assistant",
    "qoder",
    "qoder-cn",
    "qwen-code",
    "replit",
    "reasonix",
    "roo",
    "rovodev",
    "sarvam-code",
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

const SKILLSTAR_EXTENSION_IDS: &[&str] = &["omp", "deepseek", "workbuddy"];

/// Standard ids the Agent CLIs publish fold onto persisted SkillStar ids
/// through the same alias table the CLI and IPC use, so this test cannot
/// drift from it.
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
fn builtin_rows_match_upstream_plus_documented_extensions() {
    let expected: HashSet<&str> = UPSTREAM_AGENT_IDS
        .iter()
        .map(|id| skillstar_id(id))
        .chain(SKILLSTAR_EXTENSION_IDS.iter().copied())
        .collect();
    let actual: HashSet<&str> = BUILTIN_AGENT_DEFS.iter().map(|row| row.0).collect();

    let extra: Vec<&str> = actual.difference(&expected).copied().collect();
    let missing: Vec<&str> = expected.difference(&actual).copied().collect();
    assert!(
        extra.is_empty() && missing.is_empty(),
        "builtin rows drifted from upstream + extensions (extra={extra:?}, missing={missing:?})"
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
            home.join(".gemini/antigravity-cli/skills"),
            home.join(".gemini/skills"),
            home.join(".gemini/config/skills"),
            home.join(".gemini/antigravity-ide/skills"),
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

#[test]
fn fx_uses_shared_open_skills_roots() {
    let home = Path::new("/tmp/skillstar-test-home");
    let data = builtin_agent_data()
        .iter()
        .find(|agent| agent.id == "fx")
        .expect("fx builtin should exist");
    let spec = BuiltinSpec(data);
    assert_eq!(spec.resolve_global_dir(home), home.join(".agents/skills"));
    assert_eq!(spec.project_skills_rel(), Some(".agents/skills"));
}

#[test]
fn reasonix_resolves_env_windows_appdata_and_unix_home() {
    let home = Path::new("/tmp/skillstar-test-home");
    let roaming = Path::new("/tmp/skillstar-roaming");
    let custom = Path::new("/tmp/custom-reasonix");

    // Unix ignores APPDATA and uses ~/.reasonix/skills.
    assert_eq!(
        resolve_reasonix_skills_dir(home, None, Some(roaming.as_os_str()), false),
        home.join(".reasonix").join("skills")
    );

    // Windows uses APPDATA/reasonix/skills when set and non-blank.
    assert_eq!(
        resolve_reasonix_skills_dir(home, None, Some(roaming.as_os_str()), true),
        roaming.join("reasonix").join("skills")
    );

    // Windows falls back to ~/AppData/Roaming/reasonix/skills when APPDATA is absent or blank.
    assert_eq!(
        resolve_reasonix_skills_dir(home, None, None, true),
        home.join("AppData")
            .join("Roaming")
            .join("reasonix")
            .join("skills")
    );
    assert_eq!(
        resolve_reasonix_skills_dir(home, None, Some(OsStr::new("   ")), true),
        home.join("AppData")
            .join("Roaming")
            .join("reasonix")
            .join("skills")
    );

    // REASONIX_HOME overrides both Unix and Windows defaults; blank values fall back.
    for is_windows in [false, true] {
        assert_eq!(
            resolve_reasonix_skills_dir(
                home,
                Some(custom.as_os_str()),
                Some(roaming.as_os_str()),
                is_windows,
            ),
            custom.join("skills")
        );
    }
    assert_eq!(
        resolve_reasonix_skills_dir(home, Some(OsStr::new("   ")), None, false),
        home.join(".reasonix").join("skills")
    );
}
