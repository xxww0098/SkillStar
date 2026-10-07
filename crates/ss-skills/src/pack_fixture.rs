//! Offline, deterministic stand-in for `pbakaus/impeccable`: one Skill
//! published as harness-rewritten copies (same `name`, different bytes), a
//! Claude plugin wrapper, test-fixture Skills, and heavy non-Skill content.
//! The registries below are the single source for what tests assert against.

use std::ffi::OsString;
use std::path::{Path, PathBuf};

/// Env-isolated sandbox for install-flow tests: a private `HOME`/hub/data
/// tree plus a `GIT_CONFIG_GLOBAL` that [`Sandbox::map_github_url`] can point
/// at a local fixture via `insteadOf`. Shared by every test module that
/// drives the real install pipeline against a fixture repo.
pub(crate) struct Sandbox {
    previous: Vec<(&'static str, Option<OsString>)>,
    _temp: tempfile::TempDir,
    _guard: std::sync::MutexGuard<'static, ()>,
}

impl Sandbox {
    pub(crate) fn new() -> Self {
        let _guard = crate::lock_test_env();
        let temp = tempfile::tempdir().unwrap();
        let overrides = [
            ("SKILLSTAR_HUB_DIR", Some(temp.path().join("hub"))),
            ("SKILLSTAR_DATA_DIR", Some(temp.path().join("data"))),
            (
                "SKILLSTAR_TOOL_SYNC_HOME",
                Some(temp.path().join("tool-home")),
            ),
            ("HOME", Some(temp.path().join("home"))),
            ("USERPROFILE", Some(temp.path().join("home"))),
            ("GIT_CONFIG_GLOBAL", Some(temp.path().join("gitconfig"))),
            ("GIT_CONFIG_NOSYSTEM", Some(PathBuf::from("1"))),
            ("DSH_HOME", None),
            ("CODEX_HOME", None),
        ];
        let previous = overrides
            .iter()
            .map(|(key, _)| (*key, std::env::var_os(key)))
            .collect();
        unsafe {
            for (key, value) in &overrides {
                match value {
                    Some(path) => std::env::set_var(key, path),
                    None => std::env::remove_var(key),
                }
            }
        }
        crate::deployment::invalidate_profile_cache();
        Self {
            previous,
            _temp: temp,
            _guard,
        }
    }

    pub(crate) fn map_github_url(&self, github_url: &str, local_repo: &Path) {
        let config = std::env::var_os("GIT_CONFIG_GLOBAL").expect("GIT_CONFIG_GLOBAL");
        std::fs::write(
            config,
            format!(
                "[url \"{}\"]\n\tinsteadOf = {github_url}\n",
                crate::git::ops::local_file_url(local_repo)
            ),
        )
        .unwrap();
    }
}

impl Drop for Sandbox {
    fn drop(&mut self) {
        crate::deployment::invalidate_profile_cache();
        unsafe {
            for (key, previous) in self.previous.drain(..).rev() {
                match previous {
                    Some(value) => std::env::set_var(key, value),
                    None => std::env::remove_var(key),
                }
            }
        }
    }
}

/// Harness copies, each `<dir>/skills/impeccable`.
pub(crate) const HARNESS_DIRS: &[&str] = &[
    ".agent",
    ".agents",
    ".claude",
    ".cursor",
    ".dsh",
    ".gemini",
    ".github",
    ".grok",
    ".kiro",
    ".opencode",
];

/// Plugin-wrapped copies of the same identity.
pub(crate) const PLUGIN_COPIES: &[&str] = &[
    "plugin/skills/impeccable",
    "cursor-plugin/skills/impeccable",
];

/// Same basename, different frontmatter `name`: a distinct Skill.
pub(crate) const DECOY: &str = ".windsurf/skills/impeccable";

/// Skills that only exist as test fixtures upstream.
pub(crate) const TEST_FIXTURES: &[&str] = &[
    "tests/oracle/workspaces/ctx-pin/.agents/skills/impeccable",
    "tests/oracle/workspaces/ctx-pin/.claude/skills/impeccable",
    "tests/oracle/workspaces/ctx-pin/.cursor/skills/impeccable",
    "tests/oracle/workspaces/ctx-pin/.claude/skills/audit",
];

pub(crate) struct PackFixture {
    pub dir: tempfile::TempDir,
}

/// Every folder that publishes the `impeccable` identity.
pub(crate) fn published_copies() -> Vec<String> {
    HARNESS_DIRS
        .iter()
        .map(|dir| format!("{dir}/skills/impeccable"))
        .chain(PLUGIN_COPIES.iter().map(|dir| dir.to_string()))
        .collect()
}

/// A committed repo that also serves partial clones over `file://`.
pub(crate) fn impeccable_like() -> PackFixture {
    let dir = tempfile::tempdir().unwrap();
    build_impeccable_like(dir.path());
    PackFixture { dir }
}

pub(crate) fn build_impeccable_like(root: &Path) {
    std::fs::create_dir_all(root).unwrap();
    git(root, &["init", "--initial-branch=main"]);
    for (key, value) in [
        ("user.email", "test@example.com"),
        ("user.name", "SkillStar Tests"),
        ("core.autocrlf", "false"),
        ("core.eol", "lf"),
        // `file://` ignores `--filter` unless the serving side allows it.
        ("uploadpack.allowFilter", "true"),
        ("uploadpack.allowAnySHA1InWant", "true"),
    ] {
        git(root, &["config", key, value]);
    }
    write(root, ".gitattributes", "* -text\n");

    let mut executables = Vec::new();
    for dir in HARNESS_DIRS {
        let folder = format!("{dir}/skills/impeccable");
        let version = if *dir == ".agents" {
            "metadata:\n  version: 4.4.0\n"
        } else {
            "version: 4.4.0\n"
        };
        let invocable = if *dir == ".cursor" {
            ""
        } else {
            "user-invocable: true\n"
        };
        let skill = format!(
            "---\nname: impeccable\ndescription: Design fluency.\n{version}{invocable}---\n\n\
             Run `{folder}/scripts/impeccable context` once per session.\n"
        );
        executables.push(write_copy(root, &folder, &skill));
    }
    for folder in PLUGIN_COPIES {
        let skill = format!(
            "---\nname: impeccable\ndescription: Design fluency.\nversion: 4.4.0\n---\n\n\
             Run `\"${{CLAUDE_SKILL_DIR}}/scripts/impeccable\" context` ({folder}).\n"
        );
        executables.push(write_copy(root, folder, &skill));
    }
    executables.push(write_copy(
        root,
        DECOY,
        "---\nname: impeccable-classic\ndescription: The retired design skill.\n---\n\nClassic.\n",
    ));
    for folder in TEST_FIXTURES {
        let name = folder.rsplit('/').next().unwrap();
        write(
            root,
            &format!("{folder}/SKILL.md"),
            &format!("---\nname: {name}\ndescription: fixture\n---\n"),
        );
    }

    write(
        root,
        "skill/SKILL.src.md",
        "---\nname: impeccable\n---\n{{source}}\n",
    );
    write(
        root,
        ".claude-plugin/marketplace.json",
        r#"{"name":"impeccable","plugins":[{"name":"impeccable","source":"./plugin"}]}"#,
    );
    write(
        root,
        ".claude-plugin/plugin.json",
        r#"{"name":"impeccable","skills":"./.claude/skills/"}"#,
    );
    write(root, "plugin/hooks/hooks.json", r#"{"hooks":{}}"#);
    write(root, "plugin/agents/impeccable-reviewer.md", "# reviewer\n");
    write(root, "crates/engine/src/lib.rs", "pub fn heavy() {}\n");
    write(root, "README.md", "# impeccable-like fixture\n");

    git(root, &["add", "-A"]);
    let mut chmod = vec!["update-index", "--chmod=+x"];
    chmod.extend(executables.iter().map(String::as_str));
    git(root, &chmod);
    git(root, &["commit", "-m", "fixture", "--no-verify"]);
}

/// Shared `reference/` bytes across copies mirror upstream blob sharing.
fn write_copy(root: &Path, folder: &str, skill: &str) -> String {
    write(root, &format!("{folder}/SKILL.md"), skill);
    write(
        root,
        &format!("{folder}/reference/craft.md"),
        "# Craft floor\n",
    );
    let launcher = format!("{folder}/scripts/impeccable");
    write(root, &launcher, "#!/bin/sh\necho impeccable \"$@\"\n");
    launcher
}

fn write(root: &Path, relative: &str, content: &str) {
    let path = root.join(relative);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, content).unwrap();
}

pub(crate) fn git(repo: &Path, args: &[&str]) -> String {
    let output = ss_core::infra::path_env::command_with_path("git")
        .current_dir(repo)
        .args(args)
        .output()
        .expect("git spawns");
    assert!(
        output.status.success(),
        "git {} failed: {}",
        args.join(" "),
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn impeccable_fixture_matches_upstream_shape() {
        let fixture = impeccable_like();
        let root = fixture.dir.path();
        let tracked = git(root, &["ls-files"]);
        let skill_dirs: Vec<&str> = tracked
            .lines()
            .filter_map(|path| path.strip_suffix("/SKILL.md"))
            .collect();

        let mut expected: Vec<String> = published_copies();
        expected.push(DECOY.to_string());
        expected.extend(TEST_FIXTURES.iter().map(|dir| dir.to_string()));
        expected.sort();
        let mut actual: Vec<String> = skill_dirs.iter().map(|dir| dir.to_string()).collect();
        actual.sort();
        assert_eq!(actual, expected, "only registered folders carry SKILL.md");

        let mut shas: Vec<String> = published_copies()
            .iter()
            .map(|dir| git(root, &["rev-parse", &format!("HEAD:{dir}")]))
            .collect();
        let total = shas.len();
        shas.sort();
        shas.dedup();
        assert_eq!(shas.len(), total, "every published copy is byte-distinct");

        let launcher = git(
            root,
            &[
                "ls-files",
                "-s",
                ".cursor/skills/impeccable/scripts/impeccable",
            ],
        );
        assert!(
            launcher.starts_with("100755"),
            "launcher is executable: {launcher}"
        );
    }

    #[test]
    fn fixture_remote_serves_partial_clone() {
        let fixture = impeccable_like();
        let clone = tempfile::tempdir().unwrap();
        let dest = clone.path().join("clone");
        git(
            clone.path(),
            &[
                "clone",
                "--filter=blob:none",
                "--no-checkout",
                &crate::git::ops::local_file_url(fixture.dir.path()),
                dest.to_str().unwrap(),
            ],
        );
        assert_eq!(git(&dest, &["config", "remote.origin.promisor"]), "true");

        let blob = git(
            &dest,
            &["rev-parse", "HEAD:.agents/skills/impeccable/SKILL.md"],
        );
        let present = ss_core::infra::path_env::command_with_path("git")
            .current_dir(&dest)
            .env("GIT_NO_LAZY_FETCH", "1")
            .args(["cat-file", "-e", &blob])
            .status()
            .unwrap()
            .success();
        assert!(
            !present,
            "blobs stay on the promisor remote until asked for"
        );
    }

    /// Writes the fixture for the manual CLI probe in
    /// `specs/irregular-skill-packs/slices/00-probe-and-fixture.md`.
    #[test]
    #[ignore = "manual probe: SKILLSTAR_FIXTURE_OUT=<dir>"]
    fn write_impeccable_fixture() {
        let out = std::env::var_os("SKILLSTAR_FIXTURE_OUT").expect("SKILLSTAR_FIXTURE_OUT");
        build_impeccable_like(Path::new(&out));
    }
}
