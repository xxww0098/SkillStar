//! Filesystem detection helpers: install status + synced-skill counting.

use std::path::Path;

use skillstar_core::infra::path_env::{binary_on_enriched_path, desktop_app_installed};

use super::spec::AgentSpec;

/// Count how many managed skill entries (symlinks, junctions, or copies) exist
/// in a directory.
pub(crate) fn count_symlinks(dir: &Path) -> u32 {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return 0;
    };
    entries
        .flatten()
        .filter(|e| {
            let Ok(ft) = e.file_type() else {
                return false;
            };

            // Fast paths using dirent file_type (no stat calls on Unix, fast on Windows)
            if ft.is_symlink() {
                return true;
            }

            // Fallback for Windows junction points which might not be marked as symlinks
            #[cfg(windows)]
            if skillstar_core::infra::fs_ops::is_link(&e.path()) {
                return true;
            }

            // Fallback for copied directories
            if ft.is_dir() {
                let mut p = e.path();
                p.push("SKILL.md");
                return p.exists();
            }

            false
        })
        .count() as u32
}

/// Detect installation without mutating the filesystem.
///
/// Detection strategy is driven by the spec's [`AgentSpec::binary_name`] plus
/// optional desktop-app / alternate-CLI probes keyed by agent id:
///
/// - **CLI agents** (binary name set, e.g. `claude`, `gemini`): considered
///   installed if any of:
///   1. the executable is reachable in the *enriched* PATH
///      ([`skillstar_core::infra::path_env::which_in_enriched`]);
///   2. a known desktop app bundle/install path exists (hybrid GUI+CLI agents
///      such as ZCode);
///   3. the skills directory itself exists (`global_skills_dir`) — never its
///      parent. Restricting the dir fallback preserves shared-home-root
///      disambiguation (Antigravity's `~/.gemini` must not false-positive
///      Gemini).
/// - **IDE / global-only agents** (no binary name): considered installed if any of:
///   1. a known desktop app is present (Cursor / Kiro / Trae / Qoder / …);
///   2. an alternate CLI for that agent is on PATH (e.g. Antigravity's `agy`);
///   3. the skills directory or its parent (the agent config root) exists.
///
/// Creating a missing skills directory is reserved for an explicit deploy/link
/// operation; this function never writes to the filesystem.
pub(crate) fn detect_installed(spec: &dyn AgentSpec, global_skills_dir: &Path) -> bool {
    if let Some(binary) = spec.binary_name() {
        // The directory fallback is deliberately the skills dir itself (not
        // its parent), avoiding shared-home-root false positives such as
        // Antigravity's ~/.gemini being mistaken for Gemini CLI.
        return cli_agent_installed_from_signals(
            binary_on_enriched_path(binary),
            desktop_app_for_agent(spec.id()),
            global_skills_dir.is_dir(),
        );
    }

    desktop_app_for_agent(spec.id())
        || alternate_cli_for_agent(spec.id())
        || global_skills_dir.is_dir()
        || global_skills_dir
            .parent()
            .is_some_and(|parent| parent.is_dir())
}

fn cli_agent_installed_from_signals(
    binary_found: bool,
    desktop_app_found: bool,
    skills_dir_found: bool,
) -> bool {
    binary_found || desktop_app_found || skills_dir_found
}

/// Known desktop-app product names for agents that ship as GUI installs.
///
/// Mapped by agent id (not display name). Keep this table in lockstep with
/// real product names under `/Applications` / Windows Programs folders.
fn desktop_app_for_agent(agent_id: &str) -> bool {
    desktop_app_name_for_agent(agent_id).is_some_and(desktop_app_installed)
}

fn desktop_app_name_for_agent(agent_id: &str) -> Option<&'static str> {
    match agent_id {
        "claude" => Some("Claude"),
        "cursor" => Some("Cursor"),
        "kiro" => Some("Kiro"),
        "trae" => Some("Trae"),
        "qoder" => Some("Qoder"),
        "zcode" => Some("ZCode"),
        "antigravity" => Some("Antigravity"),
        _ => None,
    }
}

/// Alternate CLI binaries that prove an IDE agent is present even when the
/// config root has not been created yet (never-launched install).
fn alternate_cli_for_agent(agent_id: &str) -> bool {
    match agent_id {
        // Antigravity CLI ships as `agy` (also used by Launch Deck-adjacent tooling).
        "antigravity" => binary_on_enriched_path("agy"),
        // Cursor installs `cursor` / `cursor-agent` shims into ~/.local/bin.
        "cursor" => {
            binary_on_enriched_path("cursor") || binary_on_enriched_path("cursor-agent")
        }
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn detect_installed_falls_back_to_dir_for_spec_without_binary() {
        // A spec with no binary name must use directory presence. Use a spec
        // stub over a tempdir to keep this hermetic.
        struct DirOnlySpec;
        impl AgentSpec for DirOnlySpec {
            fn id(&self) -> &str {
                "test"
            }
            fn display_name(&self) -> &str {
                "Test"
            }
            fn icon(&self) -> &str {
                "test.svg"
            }
            fn resolve_global_dir(&self, home: &Path) -> PathBuf {
                home.join("skills")
            }
            fn project_skills_rel(&self) -> Option<&str> {
                Some(".test/skills")
            }
            // binary_name left as default None
        }

        let tmp = tempfile::tempdir().unwrap();
        // neither skills dir nor parent-only: parent (tmp root) exists, so true
        assert!(detect_installed(&DirOnlySpec, &tmp.path().join("skills")));
        // when parent does not exist -> false
        let missing = tmp.path().join("nope");
        assert!(!detect_installed(&DirOnlySpec, &missing.join("skills")));
    }

    #[test]
    fn detect_installed_uses_path_for_spec_with_binary() {
        // `cargo` ships with the rust toolchain and is essentially always on PATH
        // in this repo's dev environment, so it's a stable positive case.
        struct CargoSpec;
        impl AgentSpec for CargoSpec {
            fn id(&self) -> &str {
                "test-cargo"
            }
            fn display_name(&self) -> &str {
                "Test Cargo"
            }
            fn icon(&self) -> &str {
                "test.svg"
            }
            fn resolve_global_dir(&self, home: &Path) -> PathBuf {
                home.join(".test-cargo")
            }
            fn project_skills_rel(&self) -> Option<&str> {
                Some(".test-cargo/skills")
            }
            fn binary_name(&self) -> Option<&str> {
                Some("cargo")
            }
        }

        assert!(detect_installed(
            &CargoSpec,
            Path::new("/nonexistent/should/not/matter")
        ));
    }

    #[test]
    fn detect_installed_binary_not_present_returns_false() {
        struct FakeSpec;
        impl AgentSpec for FakeSpec {
            fn id(&self) -> &str {
                "fake"
            }
            fn display_name(&self) -> &str {
                "Fake"
            }
            fn icon(&self) -> &str {
                "fake.svg"
            }
            fn resolve_global_dir(&self, home: &Path) -> PathBuf {
                home.join(".fake")
            }
            fn project_skills_rel(&self) -> Option<&str> {
                Some(".fake/skills")
            }
            fn binary_name(&self) -> Option<&str> {
                Some("skillstar-definitely-not-a-real-bin-xyz")
            }
        }

        assert!(!detect_installed(&FakeSpec, Path::new("/nonexistent")));
    }

    #[test]
    fn detect_installed_binary_missing_falls_back_to_skills_dir() {
        // Mirrors the ZCode / Codex-on-broken-install case: the CLI binary is
        // absent from PATH but the agent has already laid down its skills dir
        // (`~/.zcode/skills`, `~/.codex/skills`). Detection must still report
        // installed so the user can link skills to it.
        struct GuiCliSpec;
        impl AgentSpec for GuiCliSpec {
            fn id(&self) -> &str {
                "gui-cli"
            }
            fn display_name(&self) -> &str {
                "GUI CLI"
            }
            fn icon(&self) -> &str {
                "gui.svg"
            }
            fn resolve_global_dir(&self, home: &Path) -> PathBuf {
                home.join(".gui-cli").join("skills")
            }
            fn project_skills_rel(&self) -> Option<&str> {
                Some(".gui-cli/skills")
            }
            fn binary_name(&self) -> Option<&str> {
                Some("skillstar-definitely-not-a-real-bin-xyz")
            }
        }

        let tmp = tempfile::tempdir().unwrap();
        let skills_dir = tmp.path().join(".gui-cli").join("skills");
        std::fs::create_dir_all(&skills_dir).unwrap();
        assert!(
            detect_installed(&GuiCliSpec, &skills_dir),
            "skills dir present should count as installed even when the CLI binary is off PATH"
        );
    }

    #[test]
    fn detect_installed_binary_missing_does_not_accept_parent_only() {
        // The whole point of the strict (skills-dir-only) fallback: an agent
        // that shares a home root with another agent must NOT be detected just
        // because the shared config root exists. This is what keeps Antigravity's
        // `~/.gemini` from false-positiving Gemini when `gemini` isn't on PATH.
        struct SharedRootSpec;
        impl AgentSpec for SharedRootSpec {
            fn id(&self) -> &str {
                "shared-root"
            }
            fn display_name(&self) -> &str {
                "Shared Root"
            }
            fn icon(&self) -> &str {
                "shared.svg"
            }
            fn resolve_global_dir(&self, home: &Path) -> PathBuf {
                home.join(".shared-root").join("skills")
            }
            fn project_skills_rel(&self) -> Option<&str> {
                Some(".shared-root/skills")
            }
            fn binary_name(&self) -> Option<&str> {
                Some("skillstar-definitely-not-a-real-bin-xyz")
            }
        }

        let tmp = tempfile::tempdir().unwrap();
        // The shared config root exists but the skills dir does not — mimics
        // ~/.gemini (created by Antigravity) without ~/.gemini/skills.
        let config_root = tmp.path().join(".shared-root");
        std::fs::create_dir_all(&config_root).unwrap();
        let skills_dir = config_root.join("skills");
        assert!(
            !detect_installed(&SharedRootSpec, &skills_dir),
            "parent-only presence must not count as installed for a binary agent"
        );
    }

    #[test]
    fn desktop_app_for_agent_unknown_id_is_false() {
        assert!(!desktop_app_for_agent("not-a-real-agent-id"));
    }

    #[test]
    fn claude_cli_and_desktop_share_one_app_detection_mapping() {
        assert_eq!(desktop_app_name_for_agent("claude"), Some("Claude"));
    }

    #[test]
    fn cli_agent_installation_accepts_each_surface_independently() {
        assert!(cli_agent_installed_from_signals(true, false, false));
        assert!(cli_agent_installed_from_signals(false, true, false));
        assert!(cli_agent_installed_from_signals(false, false, true));
        assert!(!cli_agent_installed_from_signals(false, false, false));
    }

    #[test]
    fn alternate_cli_for_unknown_agent_is_false() {
        assert!(!alternate_cli_for_agent("not-a-real-agent-id"));
    }
}
