//! vercel-labs/skills install semantics (D-081).
//!
//! Install = copy a skill folder into the canonical `~/.agents/skills/<name>`
//! as real files and record provenance in the vercel lock. Same-name installs
//! from any source overwrite. Agent symlinking stays in `deployment` /
//! `skillstar-app::global_deploy`; this module owns disk + lock only.

use std::path::Path;

use anyhow::{Context, Result, anyhow};
use skillstar_core::infra::fs_ops;
use skillstar_core::infra::paths;

use crate::fetch;
use crate::skill_lock::{self, SkillLockEntry};
use crate::source_resolver::Source;

/// Directory/file names never copied out of a source folder (vercel parity).
pub const COPY_EXCLUDES: &[&str] = &[".git", "__pycache__", "__pypackages__", "metadata.json"];

/// Kebab-case sanitize with traversal blocking (vercel `sanitizeName` parity).
pub fn sanitize_name(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    let mut pending_dash = false;
    for ch in raw.chars() {
        if ch.is_ascii_lowercase() || ch.is_ascii_digit() {
            if pending_dash && !out.is_empty() {
                out.push('-');
            }
            pending_dash = false;
            out.push(ch);
        } else if ch.is_ascii_uppercase() {
            if pending_dash && !out.is_empty() {
                out.push('-');
            }
            pending_dash = false;
            out.push(ch.to_ascii_lowercase());
        } else {
            // separators, dots, slashes and any traversal shape collapse to a
            // single hyphen; leading separators never start the name.
            pending_dash = true;
        }
    }
    let trimmed = out.trim_matches('-').to_string();
    if trimmed.is_empty() {
        "skill".to_string()
    } else {
        trimmed.chars().take(255).collect()
    }
}

/// One skill to install out of a fetched checkout.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstallUnit {
    /// Skill identity (frontmatter `name`).
    pub id: String,
    /// Repo-relative folder ("" = the checkout root itself).
    pub folder_path: String,
}

/// Copy `units` from `checkout` into canonical and write lock entries.
///
/// Overwrite semantics (vercel parity): an existing canonical folder with the
/// same name is removed and recreated; provenance in the lock is rewritten.
/// The batch fails closed before any disk mutation when a unit's frontmatter
/// does not pass the install gate — one invalid skill blocks the whole batch.
pub fn install_units(
    checkout: &Path,
    spec: &Source,
    units: &[InstallUnit],
) -> Result<Vec<String>> {
    if units.is_empty() {
        return Err(anyhow!("No skills selected for installation"));
    }
    crate::skill_mutation::policy().ensure_repository_mutation_allowed(&spec.repo_url)?;
    let mut prepared = Vec::new();
    let mut gate_errors = Vec::new();
    for unit in units {
        crate::skill_mutation::policy().ensure_skill_mutation_allowed(&unit.id)?;
        let source_dir = if unit.folder_path.is_empty() {
            checkout.to_path_buf()
        } else {
            checkout.join(&unit.folder_path)
        };
        if !source_dir.is_dir() {
            return Err(anyhow!(
                "Skill folder '{}' not found in the fetched source",
                unit.folder_path
            ));
        }
        if let Err(reason) = crate::validation::ensure_installable(&source_dir) {
            gate_errors.push(format!("'{}': {reason}", unit.id));
            continue;
        }
        prepared.push((unit, source_dir));
    }
    if !gate_errors.is_empty() {
        return Err(anyhow!(
            "Refusing to install invalid skill(s):\n{}",
            gate_errors.join("\n")
        ));
    }

    let root = paths::agents_skills_root();
    std::fs::create_dir_all(&root).context("Failed to create canonical skills directory")?;
    let folder_hash = |folder: &str| -> Option<String> {
        if checkout.join(".git").exists() {
            let path = if folder.is_empty() { None } else { Some(folder) };
            fetch::folder_tree_hash(checkout, path)
        } else {
            None
        }
    };
    let source_type = skill_lock::classify_source(&spec.repo_url);
    let now = chrono::Utc::now().to_rfc3339();

    // One lock transaction for the whole batch; disk copies happen first so a
    // failed copy leaves the lock untouched (a partial copy set without lock
    // entries is self-healing: the next install overwrites the same names).
    let mut entries = Vec::new();
    let mut names = Vec::new();
    for (unit, source_dir) in prepared {
        let name = sanitize_name(&unit.id);
        let dest = root.join(&name);
        if dest.symlink_metadata().is_ok() {
            // Overwrite: same-name install from ANY source replaces the
            // previous canonical copy (vercel parity — no cross-source skip).
            fs_ops::remove_link_or_copy(&dest)
                .with_context(|| format!("Failed to replace existing skill '{name}'"))?;
        }
        copy_skill_folder(&source_dir, &dest)
            .with_context(|| format!("Failed to copy skill '{name}' into canonical"))?;
        names.push(name.clone());
        entries.push((
            name,
            SkillLockEntry {
                source: spec.short.clone(),
                source_type,
                source_url: spec.repo_url.clone(),
                git_ref: spec.git_ref.clone(),
                skill_path: if unit.folder_path.is_empty() {
                    None
                } else {
                    Some(unit.folder_path.clone())
                },
                skill_folder_hash: folder_hash(&unit.folder_path),
                installed_at: now.clone(),
                updated_at: now.clone(),
            },
        ));
    }
    skill_lock::mutate(|lock| {
        for (name, entry) in entries.drain(..) {
            lock.upsert(&name, entry);
        }
    })
    .context("Failed to record installed skills in the lock")?;
    Ok(names)
}

/// Remove the canonical folder and lock entry for one skill.
///
/// Agent/project links are removed by the caller (`deployment`, `projects`);
/// this function touches canonical + lock only.
pub fn uninstall_canonical(name: &str) -> Result<()> {
    crate::skill_mutation::policy().ensure_skill_mutation_allowed(name)?;
    let dest = paths::agents_skill_dir(name);
    if dest.symlink_metadata().is_ok() {
        fs_ops::remove_link_or_copy(&dest)
            .with_context(|| format!("Failed to remove canonical skill '{name}'"))?;
    }
    skill_lock::mutate(|lock| lock.remove(name))
        .context("Failed to remove skill from the lock")?;
    Ok(())
}

/// Copy a skill folder's payload (exclusions per [COPY_EXCLUDES]).
fn copy_skill_folder(source: &Path, dest: &Path) -> Result<()> {
    copy_dir_filtered(source, dest, 0)
}

fn copy_dir_filtered(source: &Path, dest: &Path, depth: usize) -> Result<()> {
    std::fs::create_dir_all(dest)?;
    for entry in std::fs::read_dir(source)? {
        let entry = entry?;
        let name = entry.file_name();
        let name_str = name.to_string_lossy();
        if COPY_EXCLUDES.contains(&&*name_str) {
            continue;
        }
        let from = entry.path();
        let to = dest.join(&name);
        let file_type = entry.file_type()?;
        if file_type.is_dir() {
            copy_dir_filtered(&from, &to, depth + 1)?;
        } else if file_type.is_symlink() {
            // Broken or escaping symlinks are skipped silently (vercel parity:
            // copies dereference; a broken link has nothing to copy).
            if let Ok(resolved) = std::fs::read_link(&from) {
                if resolved.is_absolute() && !resolved.starts_with(source) {
                    continue;
                }
                std::fs::copy(std::fs::canonicalize(&from).unwrap_or(from), &to)?;
            }
        } else if file_type.is_file() {
            std::fs::copy(&from, &to)?;
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                if let Ok(metadata) = std::fs::metadata(&from)
                    && let mode = metadata.permissions().mode()
                    && mode & 0o111 != 0
                {
                    let _ = std::fs::set_permissions(
                        &to,
                        std::fs::Permissions::from_mode(mode),
                    );
                }
            }
        }
    }
    Ok(())
}

/// Enumerate installed skill names from the canonical directory.
pub fn installed_names() -> Vec<String> {
    let root = paths::agents_skills_root();
    let Ok(entries) = std::fs::read_dir(&root) else {
        return Vec::new();
    };
    entries
        .flatten()
        .filter(|entry| entry.path().join("SKILL.md").is_file())
        .filter_map(|entry| entry.file_name().into_string().ok())
        .collect()
}

#[cfg(test)]
mod tests {
    use crate::skill_lock::SourceType;

    use super::*;

    struct Sandbox {
        previous: Vec<(&'static str, Option<std::ffi::OsString>)>,
        _temp: tempfile::TempDir,
        _guard: std::sync::MutexGuard<'static, ()>,
    }

    impl Sandbox {
        fn new() -> Self {
            let _guard = crate::lock_test_env();
            let temp = tempfile::tempdir().unwrap();
            let overrides = [
                ("SKILLSTAR_DATA_DIR", temp.path().join("data")),
                ("SKILLSTAR_HUB_DIR", temp.path().join("hub")),
                ("SKILLSTAR_TOOL_SYNC_HOME", temp.path().join("tool-home")),
                ("HOME", temp.path().join("home")),
                ("USERPROFILE", temp.path().join("home")),
            ];
            let previous = overrides
                .iter()
                .map(|(key, _)| (*key, std::env::var_os(key)))
                .collect();
            unsafe {
                for (key, value) in overrides {
                    std::env::set_var(key, value);
                }
            }
            Self {
                previous,
                _temp: temp,
                _guard,
            }
        }
    }

    impl Drop for Sandbox {
        fn drop(&mut self) {
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

    fn unit(id: &str, folder: &str) -> InstallUnit {
        InstallUnit {
            id: id.to_string(),
            folder_path: folder.to_string(),
        }
    }

    fn spec(url: &str) -> Source {
        Source {
            repo_url: url.to_string(),
            short: "owner/repo".to_string(),
            git_ref: None,
            subpath: None,
            skill_filter: None,
        }
    }

    fn make_skill(dir: &Path, name: &str, description: &str) {
        std::fs::create_dir_all(dir).unwrap();
        std::fs::write(
            dir.join("SKILL.md"),
            format!("---\nname: {name}\ndescription: {description}\n---\n\n# {name}\n"),
        )
        .unwrap();
    }

    #[test]
    fn sanitize_blocks_traversal_and_uppercases() {
        assert_eq!(sanitize_name("../etc/passwd"), "etc-passwd");
        assert_eq!(sanitize_name("My Skill"), "my-skill");
        assert_eq!(sanitize_name("---"), "skill");
        assert_eq!(sanitize_name("a//b"), "a-b");
        assert_eq!(sanitize_name(&"x".repeat(300)).len(), 255);
    }

    #[test]
    fn install_copies_to_canonical_and_writes_lock() {
        let _sandbox = Sandbox::new();
        let checkout = tempfile::tempdir().unwrap();
        make_skill(&checkout.path().join("skills/foo"), "foo", "does things");

        let installed = install_units(checkout.path(), &spec("https://github.com/o/r.git"), &[unit("foo", "skills/foo")]).unwrap();
        assert_eq!(installed, vec!["foo".to_string()]);

        let canonical = paths::agents_skill_dir("foo");
        assert!(canonical.join("SKILL.md").is_file());
        let lock = skill_lock::load();
        let entry = &lock.skills["foo"];
        assert_eq!(entry.source_url, "https://github.com/o/r.git");
        assert_eq!(entry.skill_path.as_deref(), Some("skills/foo"));
        assert_eq!(entry.source_type, SourceType::Github);
    }

    #[test]
    fn same_name_from_other_source_overwrites_provenance() {
        let _sandbox = Sandbox::new();
        let first = tempfile::tempdir().unwrap();
        make_skill(&first.path().join("skills/foo"), "foo", "from repo A");
        install_units(first.path(), &spec("https://github.com/a/one.git"), &[unit("foo", "skills/foo")]).unwrap();

        let second = tempfile::tempdir().unwrap();
        make_skill(&second.path().join("elsewhere/foo"), "foo", "from repo B");
        install_units(second.path(), &spec("https://github.com/b/two.git"), &[unit("foo", "elsewhere/foo")]).unwrap();

        let lock = skill_lock::load();
        assert_eq!(lock.skills.len(), 1);
        assert_eq!(lock.skills["foo"].source_url, "https://github.com/b/two.git");
        let content =
            std::fs::read_to_string(paths::agents_skill_dir("foo").join("SKILL.md")).unwrap();
        assert!(content.contains("from repo B"), "{content}");
    }

    #[test]
    fn invalid_frontmatter_fails_whole_batch_closed() {
        let _sandbox = Sandbox::new();
        let checkout = tempfile::tempdir().unwrap();
        make_skill(&checkout.path().join("skills/good"), "good", "fine");
        let bad = checkout.path().join("skills/bad");
        std::fs::create_dir_all(&bad).unwrap();
        std::fs::write(bad.join("SKILL.md"), "---\nname: bad\n---\n").unwrap();

        let error = install_units(
            checkout.path(),
            &spec("https://github.com/o/r.git"),
            &[unit("good", "skills/good"), unit("bad", "skills/bad")],
        )
        .unwrap_err()
        .to_string();
        assert!(error.contains("bad"), "{error}");
        assert!(
            !paths::agents_skill_dir("good").exists(),
            "fail-closed batch must not install anything"
        );
        assert!(skill_lock::load().skills.is_empty());
    }

    #[test]
    fn copy_excludes_git_and_caches() {
        let _sandbox = Sandbox::new();
        let checkout = tempfile::tempdir().unwrap();
        let skill = checkout.path().join("skills/foo");
        make_skill(&skill, "foo", "d");
        std::fs::create_dir_all(skill.join(".git")).unwrap();
        std::fs::write(skill.join(".git/HEAD"), "ref").unwrap();
        std::fs::create_dir_all(skill.join("__pycache__")).unwrap();
        std::fs::write(skill.join("__pycache__/x.pyc"), "bin").unwrap();
        std::fs::write(skill.join("metadata.json"), "{}").unwrap();
        std::fs::write(skill.join("helper.sh"), "#!/bin/sh\n").unwrap();

        install_units(checkout.path(), &spec("file:///nowhere"), &[unit("foo", "skills/foo")])
            .unwrap();
        let dest = paths::agents_skill_dir("foo");
        assert!(dest.join("SKILL.md").is_file());
        assert!(dest.join("helper.sh").is_file());
        assert!(!dest.join(".git").exists());
        assert!(!dest.join("__pycache__").exists());
        assert!(!dest.join("metadata.json").exists());
    }

    #[test]
    fn uninstall_removes_canonical_and_lock_entry() {
        let _sandbox = Sandbox::new();
        let checkout = tempfile::tempdir().unwrap();
        make_skill(&checkout.path().join("skills/foo"), "foo", "d");
        install_units(checkout.path(), &spec("https://github.com/o/r.git"), &[unit("foo", "skills/foo")]).unwrap();

        uninstall_canonical("foo").unwrap();
        assert!(!paths::agents_skill_dir("foo").exists());
        assert!(skill_lock::load().skills.is_empty());
        // Idempotent.
        uninstall_canonical("foo").unwrap();
    }

    #[test]
    fn local_source_records_no_folder_hash() {
        let _sandbox = Sandbox::new();
        let checkout = tempfile::tempdir().unwrap();
        make_skill(&checkout.path().join("skills/foo"), "foo", "d");

        install_units(
            checkout.path(),
            &spec(&format!("file://{}", checkout.path().display())),
            &[unit("foo", "skills/foo")],
        )
        .unwrap();
        let lock = skill_lock::load();
        assert_eq!(lock.skills["foo"].source_type, SourceType::Local);
        assert_eq!(lock.skills["foo"].skill_folder_hash, None);
    }
}
