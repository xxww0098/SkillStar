//! Filesystem detection helpers: install status + synced-skill counting.

use std::path::{Path, PathBuf};

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
/// Detection strategy is driven by the spec's [`AgentSpec::binary_name`]:
///
/// - **CLI agents** (binary name set, e.g. `claude`, `gemini`): considered
///   installed iff the executable is reachable in the *enriched* PATH. This is
///   what disambiguates agents that share a home root — e.g. Antigravity and
///   Gemini both live under `~/.gemini`, but only the one whose CLI is on PATH
///   counts as installed. The enriched PATH covers Homebrew/cargo/snap dirs so
///   GUI-launched Tauri (whose `PATH` may omit `/opt/homebrew/bin`) still finds
///   user-installed CLIs.
///   When the binary is *not* on PATH, we fall back to directory presence — but
///   strictly on the skills dir itself (`global_skills_dir`), never its parent.
///   This matters because some CLI agents ship with a config dir even when the
///   CLI isn't on PATH: ZCode is a GUI app that never installs a `zcode` binary
///   yet lays down `~/.zcode/skills`, and Codex's npm global sometimes fails to
///   symlink into `/opt/homebrew/bin` even though `~/.codex/skills` exists.
///   Restricting the fallback to the skills dir (not the parent) preserves the
///   shared-root disambiguation: a stray `~/.gemini` from Antigravity (which
///   has no `~/.gemini/skills`) still won't false-positive Gemini.
/// - **IDE / global-only agents** (no binary name, e.g. Antigravity, OpenClaw):
///   fall back to directory presence — either the skills directory or its
///   parent (the agent's config root) existing.
///
/// Creating a missing skills directory is reserved for an explicit deploy/link
/// operation; this function never writes to the filesystem.
pub(crate) fn detect_installed(spec: &dyn AgentSpec, global_skills_dir: &Path) -> bool {
    if let Some(binary) = spec.binary_name() {
        if binary_in_enriched_path(binary) {
            return true;
        }
        // CLI not on PATH — accept a present skills dir only (not the parent),
        // to avoid re-introducing the shared-home-root false positive (see the
        // Antigravity ↔ Gemini asymmetry guarded by the mod-level tests).
        return global_skills_dir.is_dir();
    }

    global_skills_dir.is_dir()
        || global_skills_dir
            .parent()
            .is_some_and(|parent| parent.is_dir())
}

/// Resolve whether a CLI binary is reachable in the enriched PATH.
///
/// Uses `skillstar_core::infra::path_env::enriched_path()` (which prepends
/// Homebrew/snap/scoop/cargo bin dirs) instead of the raw `PATH` env var, so
/// detection works even when the app was launched from a GUI context with a
/// minimal `PATH`. `which_in` (not `which`) is what lets us feed that custom
/// path list.
fn binary_in_enriched_path(binary: &str) -> bool {
    let path_str = skillstar_core::infra::path_env::enriched_path();
    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    // `which_in` takes the PATH as a single OsStr (colon/semicolon-joined),
    // not an iterator — so we hand it the enriched PATH string directly.
    which::which_in(binary, Some(&path_str), &cwd).is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detect_installed_falls_back_to_dir_for_spec_without_binary() {
        // A spec with no binary name must use directory presence. Use a spec
        // stub over a tempdir to keep this hermetic.
        struct DirOnlySpec;
        impl AgentSpec for DirOnlySpec {
            fn id(&self) -> &str { "test" }
            fn display_name(&self) -> &str { "Test" }
            fn icon(&self) -> &str { "test.svg" }
            fn resolve_global_dir(&self, home: &Path) -> std::path::PathBuf {
                home.join("skills")
            }
            fn project_skills_rel(&self) -> Option<&str> { Some(".test/skills") }
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
            fn id(&self) -> &str { "test-cargo" }
            fn display_name(&self) -> &str { "Test Cargo" }
            fn icon(&self) -> &str { "test.svg" }
            fn resolve_global_dir(&self, home: &Path) -> std::path::PathBuf {
                home.join(".test-cargo")
            }
            fn project_skills_rel(&self) -> Option<&str> { Some(".test-cargo/skills") }
            fn binary_name(&self) -> Option<&str> { Some("cargo") }
        }

        assert!(detect_installed(&CargoSpec, Path::new("/nonexistent/should/not/matter")));
    }

    #[test]
    fn detect_installed_binary_not_present_returns_false() {
        struct FakeSpec;
        impl AgentSpec for FakeSpec {
            fn id(&self) -> &str { "fake" }
            fn display_name(&self) -> &str { "Fake" }
            fn icon(&self) -> &str { "fake.svg" }
            fn resolve_global_dir(&self, home: &Path) -> std::path::PathBuf {
                home.join(".fake")
            }
            fn project_skills_rel(&self) -> Option<&str> { Some(".fake/skills") }
            fn binary_name(&self) -> Option<&str> { Some("skillstar-definitely-not-a-real-bin-xyz") }
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
            fn id(&self) -> &str { "gui-cli" }
            fn display_name(&self) -> &str { "GUI CLI" }
            fn icon(&self) -> &str { "gui.svg" }
            fn resolve_global_dir(&self, home: &Path) -> std::path::PathBuf {
                home.join(".gui-cli").join("skills")
            }
            fn project_skills_rel(&self) -> Option<&str> { Some(".gui-cli/skills") }
            fn binary_name(&self) -> Option<&str> { Some("skillstar-definitely-not-a-real-bin-xyz") }
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
            fn id(&self) -> &str { "shared-root" }
            fn display_name(&self) -> &str { "Shared Root" }
            fn icon(&self) -> &str { "shared.svg" }
            fn resolve_global_dir(&self, home: &Path) -> std::path::PathBuf {
                home.join(".shared-root").join("skills")
            }
            fn project_skills_rel(&self) -> Option<&str> { Some(".shared-root/skills") }
            fn binary_name(&self) -> Option<&str> { Some("skillstar-definitely-not-a-real-bin-xyz") }
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
}
