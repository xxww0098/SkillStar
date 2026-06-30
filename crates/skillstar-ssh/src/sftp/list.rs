//! Remote skill discovery and listing over SFTP (read-only operations).
//!
//! `discover_remote_skills` scans `$HOME/.*` for `<dir>/skills/<name>/SKILL.md`
//! layouts; `list_remote_skills` walks one remote dir and reports each
//! subdirectory that contains a `SKILL.md` (so only genuine skills appear, not
//! stray dirs).

use anyhow::Result;
use serde::{Deserialize, Serialize};

use crate::client::RemoteExec;
use crate::hub::{REMOTE_HUB_CONTENT, shell_quote};
use crate::remote_fs::{RemoteDiscoveryFs, is_skill_entry};
use crate::types::{RemoteSkill, RemoteSkillLayout};

/// A known agent's skills directory on a remote host — the push targets the UI
/// offers. Mirrors the agent `project_skills_rel` paths from the builtin agent
/// table (kept here as a plain constant so the ssh crate doesn't depend on
/// `skillstar-projects`).
///
/// `~` is expanded by the SFTP server; paths are relative to the login $HOME.
pub const KNOWN_AGENT_SKILL_DIRS: &[(&str, &str)] = &[
    ("claude", "~/.claude/skills"),
    ("codex", "~/.codex/skills"),
    ("gemini", "~/.gemini/skills"),
    ("opencode", "~/.opencode/skills"),
    ("cursor", "~/.cursor/skills"),
    ("qoder", "~/.qoder/skills"),
    ("trae", "~/.trae/skills"),
    ("zcode", "~/.zcode/skills"),
    // Generic fallbacks some agents share.
    ("agent", "~/.agent/skills"),
];

/// One detected agent skills directory on the remote host.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RemoteAgentDir {
    /// Agent id (`claude`, `codex`, …) from [`KNOWN_AGENT_SKILL_DIRS`].
    pub agent: String,
    /// Absolute or `~`-prefixed path that exists on the remote.
    pub path: String,
}

/// An agent discovered by scanning the remote `$HOME`, with the skills found
/// under its `skills/` directory aggregated.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RemoteAgentSkills {
    /// Agent id derived from the parent dir name (`~/.grok/skills` → `grok`).
    pub agent: String,
    /// Absolute path of the agent's skills directory (`/root/.grok/skills`).
    pub path: String,
    /// Number of skills (dirs containing SKILL.md) under this agent.
    pub count: u32,
}

/// Result of a remote skill discovery scan: the agents found plus every skill
/// (carrying its `agent` so the UI can group/filter).
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct DiscoveryResult {
    /// Agents with at least one skill, sorted by agent name.
    pub agents: Vec<RemoteAgentSkills>,
    /// All skills across every agent, sorted by agent then name.
    pub skills: Vec<RemoteSkill>,
    /// Count of skills with `layout = standalone` (candidates for hub migration).
    #[serde(default)]
    pub needs_migration_count: u32,
}

/// Top-level hidden directories under `$HOME` that never hold agent skills and
/// can be slow/large to scan (cache stores, toolchains, secrets).
const SKIP_HOME_DIRS: &[&str] = &[
    ".cache",
    ".npm",
    ".config",
    ".ssh",
    ".local",
    ".docker",
    ".vscode-server",
    ".dotnet",
    ".bun",
    ".cargo",
    ".rustup",
    ".gnupg",
    ".pki",
    ".mozilla",
    ".nvm",
    ".pyenv",
    ".gradle",
    ".m2",
    ".electron-gyp",
    ".node-gyp",
    ".pm2",
    ".pip",
    ".kube",
    ".terraform",
];

/// Whether a top-level `$HOME` entry should be skipped during discovery.
pub(crate) fn should_skip_home_dir(name: &str) -> bool {
    name == "."
        || name == ".."
        || !name.starts_with('.')
        || SKIP_HOME_DIRS.contains(&name)
}

/// Derive agent id from a hidden home dir (`.codex` → `codex`).
pub(crate) fn agent_id_from_home_dir(name: &str) -> Option<String> {
    if !name.starts_with('.') || name.len() <= 1 {
        return None;
    }
    let id = name.trim_start_matches('.').to_string();
    if id.is_empty() {
        None
    } else {
        Some(id)
    }
}

/// Shell script used by [`resolve_skill_layout`] — extracted for unit tests.
pub(crate) fn layout_classify_shell_script(skill_path: &str, skill_name: &str) -> String {
    let hub_content = format!("{REMOTE_HUB_CONTENT}/{skill_name}");
    let path_q = shell_quote(skill_path);
    let hub_q = shell_quote(&hub_content);
    format!(
        r#"if [ -L {path_q} ]; then
  tgt=$(readlink {path_q} 2>/dev/null || true)
  case "$tgt" in
    *"/.skillstar/hub/content/{skill_name}"*|*".skillstar/hub/content/{skill_name}"*)
      if [ -f {hub_q}/SKILL.md ]; then
        echo hub_managed
        exit 0
      fi
      ;;
  esac
fi
echo standalone
"#
    )
}

/// One skill entry (directory or hub symlink) under an agent's `skills/` dir.
#[derive(Debug, Clone)]
pub(crate) struct ScannedSkillEntry {
    pub name: String,
    pub is_skill_entry: bool,
    pub has_skill_md: bool,
    pub size: u64,
    pub modified: Option<String>,
    pub layout: RemoteSkillLayout,
}

/// One `~/.<agent>` directory from a home scan.
#[derive(Debug, Clone)]
pub(crate) struct ScannedAgentEntry {
    pub dir_name: String,
    pub is_dir: bool,
    pub skills: Vec<ScannedSkillEntry>,
}

/// Build a [`DiscoveryResult`] from pre-collected scan data (pure, testable).
pub(crate) fn build_discovery_result(
    home: &str,
    entries: &[ScannedAgentEntry],
    known_fallback: &[RemoteAgentSkills],
) -> DiscoveryResult {
    let mut agents = Vec::new();
    let mut skills = Vec::new();
    let mut needs_migration_count = 0u32;

    for entry in entries {
        if !entry.is_dir || should_skip_home_dir(&entry.dir_name) {
            continue;
        }
        let Some(agent_id) = agent_id_from_home_dir(&entry.dir_name) else {
            continue;
        };
        let skills_dir = format!("{home}/{}/skills", entry.dir_name);
        let mut count = 0u32;
        for skill in &entry.skills {
            if !skill.is_skill_entry || !skill.has_skill_md {
                continue;
            }
            count += 1;
            if skill.layout == RemoteSkillLayout::Standalone {
                needs_migration_count += 1;
            }
            skills.push(RemoteSkill {
                name: skill.name.clone(),
                path: format!("{skills_dir}/{}", skill.name),
                agent: agent_id.clone(),
                size: skill.size,
                modified: skill.modified.clone(),
                layout: skill.layout,
            });
        }
        if count > 0 {
            agents.push(RemoteAgentSkills {
                agent: agent_id,
                path: skills_dir,
                count,
            });
        }
    }

    if agents.is_empty() {
        agents.extend(known_fallback.iter().cloned());
    }

    agents.sort_by(|a, b| a.agent.cmp(&b.agent));
    skills.sort_by(|a, b| a.agent.cmp(&b.agent).then(a.name.cmp(&b.name)));
    DiscoveryResult {
        agents,
        skills,
        needs_migration_count,
    }
}

/// Entry shape for [`filter_remote_skill_list`].
#[derive(Debug, Clone)]
pub(crate) struct ListDirEntry {
    pub name: String,
    pub is_skill_entry: bool,
    pub has_skill_md: bool,
    pub size: u64,
    pub modified: Option<String>,
}

/// Filter a remote `skills/` listing to genuine skills (dirs/symlinks with `SKILL.md`).
pub(crate) fn filter_remote_skill_list(
    remote_dir: &str,
    entries: &[ListDirEntry],
) -> Vec<RemoteSkill> {
    let base = remote_dir.trim_end_matches('/');
    let mut skills = Vec::new();
    for entry in entries {
        if entry.name.starts_with('.') || !entry.is_skill_entry || !entry.has_skill_md {
            continue;
        }
        skills.push(RemoteSkill {
            name: entry.name.clone(),
            path: format!("{base}/{}", entry.name),
            agent: String::new(),
            size: entry.size,
            modified: entry.modified.clone(),
            layout: RemoteSkillLayout::default(),
        });
    }
    skills.sort_by(|a, b| a.name.cmp(&b.name));
    skills
}

/// Probe whether a skill entry has a readable `SKILL.md` (direct path or hub content for symlinks).
async fn probe_has_skill_md<F: RemoteDiscoveryFs>(
    fs: &F,
    skill_path: &str,
    skill_name: &str,
    attrs: &russh_sftp::protocol::FileAttributes,
) -> bool {
    if fs
        .path_exists(&format!("{skill_path}/SKILL.md"))
        .await
    {
        return true;
    }
    if attrs.is_symlink() {
        return fs
            .path_exists(&format!("{REMOTE_HUB_CONTENT}/{skill_name}/SKILL.md"))
            .await;
    }
    false
}

/// Resolve layout: hub symlinks are inferred locally; standalone dirs use remote exec.
async fn resolve_skill_layout<E: RemoteExec, F: RemoteDiscoveryFs>(
    exec: &mut E,
    fs: &F,
    skill_path: &str,
    skill_name: &str,
    attrs: &russh_sftp::protocol::FileAttributes,
    has_skill_md: bool,
) -> RemoteSkillLayout {
    if !has_skill_md {
        return RemoteSkillLayout::Standalone;
    }
    if attrs.is_symlink()
        && fs
            .path_exists(&format!("{REMOTE_HUB_CONTENT}/{skill_name}/SKILL.md"))
            .await
    {
        return RemoteSkillLayout::HubManaged;
    }
    let script = layout_classify_shell_script(skill_path, skill_name);
    match exec.exec_script(&script).await {
        Ok(out) if out.trim() == "hub_managed" => RemoteSkillLayout::HubManaged,
        _ => RemoteSkillLayout::Standalone,
    }
}

/// Discover all agent skills on the remote host by scanning `$HOME/.*` for
/// `<dir>/skills/<name>/SKILL.md` layouts.
///
/// This is **discovery-based**, not a fixed-path lookup: any agent whose
/// `~/.<agent>/skills/` holds `SKILL.md`-bearing subdirs is reported (grok,
/// agents, claude, codex, … — known or not). [`KNOWN_AGENT_SKILL_DIRS`] is only
/// used as a fallback seed when the scan finds nothing (fresh server).
pub async fn discover_remote_skills<E, F>(exec: &mut E, fs: &F) -> Result<DiscoveryResult>
where
    E: RemoteExec,
    F: RemoteDiscoveryFs,
{
    let home = fs.canonicalize_home().await;
    let top = fs.read_dir(&home).await;

    let mut scan = Vec::new();
    for (name, attrs) in top {
        if should_skip_home_dir(&name) || !attrs.is_dir() {
            continue;
        }
        let skills_dir = format!("{home}/{name}/skills");
        let sub = fs.read_dir(&skills_dir).await;
        let mut skills = Vec::new();
        for (skill_name, skill_attrs) in sub {
            if skill_name.is_empty() || !is_skill_entry(&skill_attrs) {
                continue;
            }
            let skill_path = format!("{skills_dir}/{skill_name}");
            let has_skill_md =
                probe_has_skill_md(fs, &skill_path, &skill_name, &skill_attrs).await;
            let layout = resolve_skill_layout(
                exec,
                fs,
                &skill_path,
                &skill_name,
                &skill_attrs,
                has_skill_md,
            )
            .await;
            skills.push(ScannedSkillEntry {
                name: skill_name,
                is_skill_entry: true,
                has_skill_md,
                size: skill_attrs.size.unwrap_or(0),
                modified: skill_attrs
                    .mtime
                    .and_then(|t| chrono_like_rfc3339(t as i64)),
                layout,
            });
        }
        scan.push(ScannedAgentEntry {
            dir_name: name,
            is_dir: true,
            skills,
        });
    }

    let mut known_fallback = Vec::new();
    for (agent, path) in KNOWN_AGENT_SKILL_DIRS {
        if fs.path_exists(path).await {
            known_fallback.push(RemoteAgentSkills {
                agent: (*agent).to_string(),
                path: (*path).to_string(),
                count: 0,
            });
        }
    }

    Ok(build_discovery_result(&home, &scan, &known_fallback))
}

/// List skills under a remote directory. A subdirectory counts as a skill iff
/// it contains a `SKILL.md`.
pub async fn list_remote_skills<F: RemoteDiscoveryFs>(
    fs: &F,
    remote_dir: &str,
) -> Result<Vec<RemoteSkill>> {
    let entries = fs.read_dir(remote_dir).await;
    let base = remote_dir.trim_end_matches('/');

    let mut list_entries = Vec::new();
    for (name, attrs) in entries {
        let is_skill_entry = is_skill_entry(&attrs);
        let has_skill_md = if is_skill_entry && !name.starts_with('.') {
            let skill_path = format!("{base}/{name}");
            probe_has_skill_md(fs, &skill_path, &name, &attrs).await
        } else {
            false
        };
        list_entries.push(ListDirEntry {
            name,
            is_skill_entry,
            has_skill_md,
            size: attrs.size.unwrap_or(0),
            modified: attrs.mtime.and_then(|t| chrono_like_rfc3339(t as i64)),
        });
    }
    Ok(filter_remote_skill_list(remote_dir, &list_entries))
}

/// Best-effort RFC3339 formatting of a Unix timestamp.
fn chrono_like_rfc3339(secs: i64) -> Option<String> {
    // Avoid pulling chrono into this crate for one call; format manually.
    // Sufficient for display; not used for ordering.
    let days = secs.div_euclid(86_400);
    let _rem = secs.rem_euclid(86_400);
    // Civil-from-days (Howard Hinnant's algorithm).
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = if m <= 2 { y + 1 } else { y };
    Some(format!("{year:04}-{m:02}-{d:02}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn chrono_like_rfc3339_format_has_date_shape() {
        let s = chrono_like_rfc3339(1_700_000_000).unwrap();
        assert_eq!(s.len(), 10); // YYYY-MM-DD
        assert!(s.starts_with("20"));
    }

    #[test]
    fn rfc3339_helper_formats_date() {
        chrono_like_rfc3339_format_has_date_shape();
    }

    #[test]
    fn should_skip_home_dir_filters_blacklist_and_non_hidden() {
        assert!(should_skip_home_dir(".cache"));
        assert!(should_skip_home_dir(".ssh"));
        assert!(should_skip_home_dir(".pm2"));
        assert!(should_skip_home_dir("Documents"));
        assert!(should_skip_home_dir("."));
        assert!(!should_skip_home_dir(".codex"));
        assert!(!should_skip_home_dir(".grok"));
    }

    #[test]
    fn agent_id_from_home_dir_strips_leading_dot() {
        assert_eq!(agent_id_from_home_dir(".codex").as_deref(), Some("codex"));
        assert_eq!(agent_id_from_home_dir(".grok").as_deref(), Some("grok"));
        assert!(agent_id_from_home_dir("codex").is_none());
        assert!(agent_id_from_home_dir(".").is_none());
    }

    #[test]
    fn layout_classify_script_targets_hub_content() {
        let script = layout_classify_shell_script(
            "/root/.codex/skills/my-skill",
            "my-skill",
        );
        assert!(script.contains("hub/content/my-skill"));
        assert!(script.contains("hub_managed"));
        assert!(script.contains("readlink"));
    }

    /// vps-yy-style VPS layout: codex hub-managed + grok standalone + cache skipped.
    #[test]
    fn build_discovery_result_vps_yy_layout() {
        let home = "/root";
        let scan = vec![
            ScannedAgentEntry {
                dir_name: ".cache".into(),
                is_dir: true,
                skills: vec![ScannedSkillEntry {
                    name: "junk".into(),
                    is_skill_entry: true,
                    has_skill_md: true,
                    size: 0,
                    modified: None,
                    layout: RemoteSkillLayout::Standalone,
                }],
            },
            ScannedAgentEntry {
                dir_name: ".codex".into(),
                is_dir: true,
                skills: vec![
                    ScannedSkillEntry {
                        name: "real-skill".into(),
                        is_skill_entry: true,
                        has_skill_md: true,
                        size: 1024,
                        modified: None,
                        layout: RemoteSkillLayout::HubManaged,
                    },
                    ScannedSkillEntry {
                        name: "not-a-skill".into(),
                        is_skill_entry: true,
                        has_skill_md: false,
                        size: 0,
                        modified: None,
                        layout: RemoteSkillLayout::Standalone,
                    },
                ],
            },
            ScannedAgentEntry {
                dir_name: ".grok".into(),
                is_dir: true,
                skills: vec![ScannedSkillEntry {
                    name: "standalone-one".into(),
                    is_skill_entry: true,
                    has_skill_md: true,
                    size: 512,
                    modified: None,
                    layout: RemoteSkillLayout::Standalone,
                }],
            },
            ScannedAgentEntry {
                dir_name: ".npm".into(),
                is_dir: true,
                skills: vec![],
            },
        ];

        let result = build_discovery_result(home, &scan, &[]);
        assert_eq!(result.agents.len(), 2);
        assert_eq!(result.skills.len(), 2);
        assert_eq!(result.needs_migration_count, 1);

        let codex = result.agents.iter().find(|a| a.agent == "codex").unwrap();
        assert_eq!(codex.count, 1);
        assert_eq!(codex.path, "/root/.codex/skills");

        let grok = result.agents.iter().find(|a| a.agent == "grok").unwrap();
        assert_eq!(grok.count, 1);

        let hub_skill = result
            .skills
            .iter()
            .find(|s| s.name == "real-skill")
            .unwrap();
        assert_eq!(hub_skill.agent, "codex");
        assert_eq!(hub_skill.layout, RemoteSkillLayout::HubManaged);
        assert_eq!(hub_skill.path, "/root/.codex/skills/real-skill");

        let standalone = result
            .skills
            .iter()
            .find(|s| s.name == "standalone-one")
            .unwrap();
        assert_eq!(standalone.agent, "grok");
        assert_eq!(standalone.layout, RemoteSkillLayout::Standalone);
    }

    #[test]
    fn build_discovery_result_seeds_known_dirs_when_scan_empty() {
        let fallback = vec![RemoteAgentSkills {
            agent: "claude".into(),
            path: "~/.claude/skills".into(),
            count: 0,
        }];
        let result = build_discovery_result("/root", &[], &fallback);
        assert_eq!(result.agents.len(), 1);
        assert_eq!(result.agents[0].agent, "claude");
        assert!(result.skills.is_empty());
    }

    #[test]
    fn filter_remote_skill_list_keeps_only_skill_md_dirs() {
        let entries = vec![
            ListDirEntry {
                name: "good-skill".into(),
                is_skill_entry: true,
                has_skill_md: true,
                size: 100,
                modified: None,
            },
            ListDirEntry {
                name: "hub-link".into(),
                is_skill_entry: true,
                has_skill_md: true,
                size: 0,
                modified: None,
            },
            ListDirEntry {
                name: "empty-dir".into(),
                is_skill_entry: true,
                has_skill_md: false,
                size: 0,
                modified: None,
            },
            ListDirEntry {
                name: ".hidden".into(),
                is_skill_entry: true,
                has_skill_md: true,
                size: 0,
                modified: None,
            },
            ListDirEntry {
                name: "readme.md".into(),
                is_skill_entry: false,
                has_skill_md: false,
                size: 10,
                modified: None,
            },
        ];
        let skills = filter_remote_skill_list("~/.codex/skills", &entries);
        assert_eq!(skills.len(), 2);
        assert_eq!(skills[0].name, "good-skill");
        assert_eq!(skills[1].name, "hub-link");
    }

    use crate::remote_fs::{MockRemoteExec, MockRemoteFs};

    /// Drives the real `discover_remote_skills` entry point on a vps-yy mock tree.
    #[tokio::test]
    async fn discover_remote_skills_vps_yy_mock_fs() {
        let mut exec = MockRemoteExec::default();
        let fs = MockRemoteFs::vps_yy_layout();
        let result = discover_remote_skills(&mut exec, &fs).await.unwrap();

        assert_eq!(result.agents.len(), 2);
        assert_eq!(result.skills.len(), 2);
        assert_eq!(result.needs_migration_count, 1);

        let hub = result.skills.iter().find(|s| s.name == "hub-skill").unwrap();
        assert_eq!(hub.agent, "codex");
        assert_eq!(hub.layout, RemoteSkillLayout::HubManaged);

        let standalone = result
            .skills
            .iter()
            .find(|s| s.name == "standalone-one")
            .unwrap();
        assert_eq!(standalone.agent, "grok");
        assert_eq!(standalone.layout, RemoteSkillLayout::Standalone);
    }

    /// Drives the real `list_remote_skills` entry point including hub symlinks.
    #[tokio::test]
    async fn list_remote_skills_includes_hub_symlinks() {
        let fs = MockRemoteFs::vps_yy_layout();
        let skills = list_remote_skills(&fs, "/root/.codex/skills")
            .await
            .unwrap();
        assert_eq!(skills.len(), 1);
        assert_eq!(skills[0].name, "hub-skill");
    }
}
