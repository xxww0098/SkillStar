//! Shared path resolution for hub skills (symlinks, lockfile `source_folder`, nested SKILL.md).

use crate::core::lockfile;

pub(crate) fn resolve_skill_dir(skill_dir: &std::path::Path) -> std::path::PathBuf {
    if !crate::core::infra::fs_ops::is_link(skill_dir) {
        return skill_dir.to_path_buf();
    }

    crate::core::infra::fs_ops::read_link_resolved(skill_dir)
        .unwrap_or_else(|_| skill_dir.to_path_buf())
}

pub(crate) fn lockfile_source_folder(skill_name: &str) -> Option<String> {
    let lock_path = lockfile::lockfile_path();
    let lockfile = lockfile::Lockfile::load(&lock_path).ok()?;
    lockfile
        .skills
        .into_iter()
        .find(|entry| entry.name == skill_name)
        .and_then(|entry| entry.source_folder)
}

fn find_nested_skill_dir_by_name(
    root: &std::path::Path,
    skill_name: &str,
) -> Option<std::path::PathBuf> {
    const SKIP_DIRS: &[&str] = &[".git", "node_modules", "target", "dist", "build", ".next"];

    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let entries = match std::fs::read_dir(&dir) {
            Ok(entries) => entries,
            Err(_) => continue,
        };

        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }

            let dir_name = entry.file_name().to_string_lossy().to_string();
            if SKIP_DIRS.iter().any(|skip| *skip == dir_name) {
                continue;
            }

            if dir_name == skill_name && path.join("SKILL.md").exists() {
                return Some(path);
            }

            stack.push(path);
        }
    }

    None
}

pub(crate) fn resolve_skill_content_dir(name: &str) -> Option<std::path::PathBuf> {
    let skills_dir = crate::core::infra::paths::hub_skills_dir();
    let skill_dir = skills_dir.join(name);
    if !skill_dir.exists() {
        return None;
    }

    let effective_dir = resolve_skill_dir(&skill_dir);
    if effective_dir.join("SKILL.md").exists() {
        return Some(effective_dir);
    }

    if let Some(source_folder) = lockfile_source_folder(name) {
        let nested = effective_dir.join(source_folder);
        if nested.join("SKILL.md").exists() {
            return Some(nested);
        }
    }

    find_nested_skill_dir_by_name(&effective_dir, name)
}
