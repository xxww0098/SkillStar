//! Canonical project identity for the project-skills MCP.
//!
//! `register_project` still matches the path string exactly. This module
//! matches live directories by `std::fs::canonicalize` and never rewrites an
//! existing `projects.json` path.

use anyhow::{Context, Result};
use std::path::{Path, PathBuf};

use super::index::{load_index, save_index, unique_project_name};
use super::types::ProjectEntry;
use skillstar_core::infra::paths as fs_paths;

/// A project root the caller can read. `name` is set when one live index row
/// already canonicalizes to `root`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ObservedProject {
    pub root: PathBuf,
    pub name: Option<String>,
    /// Success always carries `false`. Two live rows for one root are an error.
    pub ambiguous: bool,
}

/// Read the index. Does not create `projects.json` or insert a row.
pub fn observe_project(path: &str) -> Result<ObservedProject> {
    let root = canonicalize_absolute_dir(path)?;
    let matches = live_matches(&root);
    match matches.as_slice() {
        [] => Ok(ObservedProject {
            root,
            name: None,
            ambiguous: false,
        }),
        [one] => Ok(ObservedProject {
            root,
            name: Some(one.name.clone()),
            ambiguous: false,
        }),
        many => {
            let names = many
                .iter()
                .map(|entry| entry.name.as_str())
                .collect::<Vec<_>>()
                .join(", ");
            anyhow::bail!("ambiguous project root {}: {names}", root.display())
        }
    }
}

/// Insert a canonical row, or reuse the one live row without rewriting its path.
///
/// Does not take the project write lock. Production callers must already hold it.
pub fn register_canonical_project(path: &str) -> Result<ProjectEntry> {
    let observed = observe_project(path)?;
    if let Some(name) = observed.name {
        return load_index()
            .projects
            .into_iter()
            .find(|entry| entry.name == name)
            .context("observed project disappeared from the index");
    }

    let mut index = load_index();
    let base = observed
        .root
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| "unknown".to_string());
    let name = unique_project_name(&index, &base);
    let stored = observed
        .root
        .to_str()
        .context("canonical project path is not utf-8")?
        .to_string();
    let entry = ProjectEntry {
        path: stored,
        name: name.clone(),
        created_at: chrono::Utc::now().to_rfc3339(),
    };
    let dir = fs_paths::project_detail_dir(&name);
    std::fs::create_dir_all(&dir)
        .with_context(|| format!("failed to create project dir: {}", dir.display()))?;
    index.projects.push(entry.clone());
    save_index(&index)?;
    Ok(entry)
}

/// The deepest existing ancestor of `target`, once canonicalized, must stay
/// inside `root`. A symlink or junction that points outside fails before any
/// directory is created.
pub fn contained_child(root: &Path, target: &Path) -> Result<()> {
    let root = std::fs::canonicalize(root)
        .with_context(|| format!("canonicalize project root {}", root.display()))?;
    let ancestor = deepest_existing_ancestor(target)
        .with_context(|| format!("no existing ancestor for {}", target.display()))?;
    let ancestor = std::fs::canonicalize(&ancestor)
        .with_context(|| format!("canonicalize {}", ancestor.display()))?;
    if ancestor == root || ancestor.starts_with(&root) {
        return Ok(());
    }
    anyhow::bail!(
        "{} escapes project root {}",
        ancestor.display(),
        root.display()
    )
}

fn canonicalize_absolute_dir(path: &str) -> Result<PathBuf> {
    let path = Path::new(path);
    if !path.is_absolute() {
        anyhow::bail!("project path must be absolute");
    }
    if !path.is_dir() {
        anyhow::bail!("project path is not a directory: {}", path.display());
    }
    std::fs::canonicalize(path)
        .with_context(|| format!("canonicalize project path {}", path.display()))
}

fn live_matches(root: &Path) -> Vec<ProjectEntry> {
    load_index()
        .projects
        .into_iter()
        .filter(|entry| {
            let path = Path::new(&entry.path);
            path.is_dir() && std::fs::canonicalize(path).ok().as_deref() == Some(root)
        })
        .collect()
}

fn deepest_existing_ancestor(path: &Path) -> Option<PathBuf> {
    let mut current = path.to_path_buf();
    loop {
        if current.symlink_metadata().is_ok() {
            return Some(current);
        }
        if !current.pop() {
            return None;
        }
    }
}

#[cfg(test)]
mod project_binding_tests {
    use super::{contained_child, observe_project, register_canonical_project};
    use crate::projects::{list_projects, register_project};
    use skillstar_core::infra::paths as fs_paths;
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::{SystemTime, UNIX_EPOCH};

    static SUFFIX: AtomicU64 = AtomicU64::new(0);

    struct EnvGuard {
        root: PathBuf,
        extras: Vec<PathBuf>,
        home: Option<std::ffi::OsString>,
        data: Option<std::ffi::OsString>,
        #[cfg(windows)]
        userprofile: Option<std::ffi::OsString>,
        _lock: std::sync::MutexGuard<'static, ()>,
    }

    impl EnvGuard {
        fn new(label: &str) -> Self {
            let _lock = crate::lock_test_env();
            let n = SUFFIX.fetch_add(1, Ordering::Relaxed);
            let nanos = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|duration| duration.as_nanos())
                .unwrap_or(0);
            let root = std::env::temp_dir().join(format!("skillstar-bind-{label}-{nanos}-{n}"));
            fs::create_dir_all(root.join("home")).unwrap();
            fs::create_dir_all(root.join("data")).unwrap();
            let guard = Self {
                home: std::env::var_os("HOME"),
                data: std::env::var_os("SKILLSTAR_DATA_DIR"),
                #[cfg(windows)]
                userprofile: std::env::var_os("USERPROFILE"),
                extras: Vec::new(),
                root,
                _lock,
            };
            unsafe {
                std::env::set_var("HOME", guard.root.join("home"));
                std::env::set_var("SKILLSTAR_DATA_DIR", guard.root.join("data"));
                #[cfg(windows)]
                std::env::set_var("USERPROFILE", guard.root.join("home"));
            }
            guard
        }
    }

    impl Drop for EnvGuard {
        fn drop(&mut self) {
            unsafe {
                restore("HOME", self.home.take());
                restore("SKILLSTAR_DATA_DIR", self.data.take());
                #[cfg(windows)]
                restore("USERPROFILE", self.userprofile.take());
            }
            for extra in &self.extras {
                let _ = fs::remove_dir_all(extra);
            }
            let _ = fs::remove_dir_all(&self.root);
        }
    }

    unsafe fn restore(key: &str, previous: Option<std::ffi::OsString>) {
        match previous {
            Some(value) => unsafe { std::env::set_var(key, value) },
            None => unsafe { std::env::remove_var(key) },
        }
    }

    fn manifest_bytes() -> Option<Vec<u8>> {
        fs::read(fs_paths::projects_manifest_path()).ok()
    }

    #[test]
    fn observe_does_not_create_projects_json() {
        let env = EnvGuard::new("observe");
        let project = env.root.join("workspace");
        fs::create_dir_all(&project).unwrap();
        let observed = observe_project(project.to_str().unwrap()).unwrap();
        assert!(observed.name.is_none());
        assert!(!observed.ambiguous);
        assert!(!fs_paths::projects_manifest_path().exists());
    }

    #[test]
    fn observe_matches_noncanonical_registered_path_without_rewriting_it() {
        let mut env = EnvGuard::new("alias");
        let (alias, real) = symlink_pair(&mut env, "demo");
        let registered = register_project(alias.to_str().unwrap()).unwrap();
        let before = manifest_bytes().unwrap();

        let observed = observe_project(real.to_str().unwrap()).unwrap();
        assert_eq!(observed.name.as_deref(), Some(registered.name.as_str()));
        assert_eq!(observed.root, fs::canonicalize(&alias).unwrap());
        assert_eq!(manifest_bytes().as_deref(), Some(before.as_slice()));
        let stored = fs::read_to_string(fs_paths::projects_manifest_path()).unwrap();
        assert!(stored.contains(alias.to_str().unwrap()));
        assert_ne!(alias, real);
    }

    #[test]
    fn observe_rejects_two_live_entries_for_one_canonical_root() {
        let mut env = EnvGuard::new("ambiguous");
        let (alias, real) = symlink_pair(&mut env, "demo");
        register_project(alias.to_str().unwrap()).unwrap();
        register_project(real.to_str().unwrap()).unwrap();
        let before = manifest_bytes().unwrap();

        let observed = observe_project(alias.to_str().unwrap()).unwrap_err();
        assert!(observed.to_string().contains("ambiguous"), "{observed}");
        let registered = register_canonical_project(real.to_str().unwrap()).unwrap_err();
        assert!(registered.to_string().contains("ambiguous"), "{registered}");
        assert_eq!(manifest_bytes().as_deref(), Some(before.as_slice()));
    }

    #[test]
    fn register_inserts_canonical_path_once() {
        let mut env = EnvGuard::new("insert");
        let (alias, real) = symlink_pair(&mut env, "demo");
        let canonical = fs::canonicalize(&alias).unwrap();

        let first = register_canonical_project(alias.to_str().unwrap()).unwrap();
        assert_eq!(Path::new(&first.path), canonical);
        let second = register_canonical_project(real.to_str().unwrap()).unwrap();
        assert_eq!(second.name, first.name);
        assert_eq!(second.path, first.path);
        assert_eq!(list_projects().len(), 1);
        assert_eq!(
            observe_project(alias.to_str().unwrap()).unwrap().root,
            observe_project(real.to_str().unwrap()).unwrap().root
        );
    }

    #[test]
    fn contained_child_rejects_symlink_that_escapes_root() {
        let env = EnvGuard::new("escape");
        let root = env.root.join("project");
        let outside = env.root.join("outside");
        fs::create_dir_all(&root).unwrap();
        fs::create_dir_all(&outside).unwrap();
        assert!(contained_child(&root, &root.join("notes").join("today.md")).is_ok());

        let link = root.join("escape");
        link_dir(&outside, &link);
        let err = contained_child(&root, &link.join("secret.txt")).unwrap_err();
        assert!(err.to_string().contains("escapes"), "{err}");
    }

    fn symlink_pair(env: &mut EnvGuard, name: &str) -> (PathBuf, PathBuf) {
        let n = SUFFIX.fetch_add(1, Ordering::Relaxed);
        if let Ok(tmp) = fs::canonicalize("/tmp") {
            let alias = PathBuf::from("/tmp").join(format!("skillstar-bind-{name}-{n}"));
            if tmp != Path::new("/tmp") && !alias.exists() {
                fs::create_dir_all(&alias).unwrap();
                let real = fs::canonicalize(&alias).unwrap();
                env.extras.push(alias.clone());
                return (alias, real);
            }
        }
        let real = env.root.join(name);
        let alias = env.root.join(format!("{name}-alias"));
        fs::create_dir_all(&real).unwrap();
        link_dir(&real, &alias);
        (alias, real)
    }

    fn link_dir(target: &Path, link: &Path) {
        #[cfg(unix)]
        std::os::unix::fs::symlink(target, link).unwrap();
        #[cfg(windows)]
        junction::create(target, link).unwrap();
    }
}
