//! Abstraction over SFTP read/metadata ops for remote skill discovery.
//!
//! [`SftpSession`] implements [`RemoteDiscoveryFs`] for production. Tests use
//! [`MockRemoteFs`] to drive the real `discover_remote_skills` / `list_remote_skills`
//! entry points without a live SSH session.

use std::collections::{HashMap, HashSet};

use russh_sftp::client::SftpSession;
use russh_sftp::protocol::FileAttributes;

/// Remote filesystem operations used by skill discovery and listing.
pub trait RemoteDiscoveryFs: Send + Sync {
    fn canonicalize_home(&self) -> impl std::future::Future<Output = String> + Send;
    fn read_dir(
        &self,
        path: &str,
    ) -> impl std::future::Future<Output = Vec<(String, FileAttributes)>> + Send;
    fn path_exists(&self, path: &str) -> impl std::future::Future<Output = bool> + Send;
}

impl RemoteDiscoveryFs for SftpSession {
    async fn canonicalize_home(&self) -> String {
        self.canonicalize(".")
            .await
            .unwrap_or_else(|_| ".".to_string())
    }

    async fn read_dir(&self, path: &str) -> Vec<(String, FileAttributes)> {
        match self.read_dir(path).await {
            Ok(entries) => entries
                .into_iter()
                .map(|e| (e.file_name(), e.metadata().clone()))
                .collect(),
            Err(_) => Vec::new(),
        }
    }

    async fn path_exists(&self, path: &str) -> bool {
        self.metadata(path).await.is_ok()
    }
}

/// Returns true when an SFTP directory entry may hold a skill (dir or symlink).
pub fn is_skill_entry(attrs: &FileAttributes) -> bool {
    attrs.is_dir() || attrs.is_symlink()
}

/// In-memory remote tree for unit tests (vps-yy style layouts).
#[derive(Debug, Clone)]
pub struct MockRemoteFs {
    pub home: String,
    dirs: HashMap<String, Vec<(String, FileAttributes)>>,
    paths: HashSet<String>,
}

impl MockRemoteFs {
    pub fn new(home: impl Into<String>) -> Self {
        Self {
            home: home.into(),
            dirs: HashMap::new(),
            paths: HashSet::new(),
        }
    }

    pub fn with_dir(mut self, path: impl Into<String>, entries: Vec<(String, FileAttributes)>) -> Self {
        self.dirs.insert(path.into(), entries);
        self
    }

    pub fn with_path(mut self, path: impl Into<String>) -> Self {
        self.paths.insert(path.into());
        self
    }

    fn dir_attrs() -> FileAttributes {
        let mut a = FileAttributes::default();
        a.set_dir(true);
        a
    }

    fn symlink_attrs() -> FileAttributes {
        let mut a = FileAttributes::empty();
        a.set_symlink(true);
        a
    }

    /// Typical vps-yy VPS: codex hub symlink + grok standalone copy.
    pub fn vps_yy_layout() -> Self {
        Self::new("/root")
            .with_dir(
                "/root",
                vec![
                    (".codex".into(), Self::dir_attrs()),
                    (".grok".into(), Self::dir_attrs()),
                    (".cache".into(), Self::dir_attrs()),
                ],
            )
            .with_dir(
                "/root/.codex/skills",
                vec![
                    ("hub-skill".into(), Self::symlink_attrs()),
                    ("stray-folder".into(), Self::dir_attrs()),
                ],
            )
            .with_dir(
                "/root/.grok/skills",
                vec![("standalone-one".into(), Self::dir_attrs())],
            )
            .with_path("~/.skillstar/hub/content/hub-skill/SKILL.md")
            .with_path("/root/.grok/skills/standalone-one/SKILL.md")
    }
}

/// Stub remote exec for unit tests (no live SSH channel).
#[derive(Debug, Default)]
pub struct MockRemoteExec;

impl crate::client::RemoteExec for MockRemoteExec {
    async fn exec_script(&mut self, script: &str) -> anyhow::Result<String> {
        if script.contains("hub_managed") && script.contains("readlink") {
            Ok(String::new())
        } else {
            Ok("standalone".into())
        }
    }
}

impl RemoteDiscoveryFs for MockRemoteFs {
    async fn canonicalize_home(&self) -> String {
        self.home.clone()
    }

    async fn read_dir(&self, path: &str) -> Vec<(String, FileAttributes)> {
        self.dirs.get(path).cloned().unwrap_or_default()
    }

    async fn path_exists(&self, path: &str) -> bool {
        self.paths.contains(path)
    }
}