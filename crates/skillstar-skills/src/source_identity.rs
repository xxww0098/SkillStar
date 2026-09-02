//! Read-only provenance and snapshot facts for installed Skills.
//!
//! This module does not construct learning-domain identity types. Callers in
//! `skillstar-app` combine these facts with channel subscription records.

use serde::{Deserialize, Serialize};
use skillstar_core::infra::error::AppError;
use uuid::Uuid;

use crate::content::{self, SNAPSHOT_HASH_VERSION, SkillSnapshot};
use crate::{local_identity, local_skill, lockfile, repo_link};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SnapshotFacts {
    pub content_hash: String,
    pub hash_version: u32,
    pub file_count: usize,
    pub total_bytes: u64,
    pub source_files: Vec<String>,
}

impl SnapshotFacts {
    pub fn from_snapshot(snapshot: &SkillSnapshot) -> Self {
        Self {
            content_hash: snapshot.content_hash.clone(),
            hash_version: SNAPSHOT_HASH_VERSION,
            file_count: snapshot.files.len(),
            total_bytes: snapshot.total_bytes,
            source_files: snapshot
                .files
                .iter()
                .map(|file| file.relative_path.clone())
                .collect(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LockProvenance {
    pub git_url: String,
    pub canonical_repository: String,
    pub git_ref: Option<String>,
    pub source_folder: Option<String>,
    pub tree_hash: String,
    pub content_hash: Option<String>,
    pub content_hash_version: Option<u32>,
    pub baseline_unknown: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GitHeadFacts {
    pub commit_sha: String,
    pub tree_hash: String,
    pub content_root_tree: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstalledSkillFacts {
    pub installed_name: String,
    pub is_local: bool,
    pub local_id: Option<Uuid>,
    pub lock: Option<LockProvenance>,
    pub git_head: Option<GitHeadFacts>,
    pub snapshot: SnapshotFacts,
}

/// Collect current disk/lock/git facts for an installed Skill.
///
/// `name` is only a hub lookup handle. Missing, duplicate, or contradictory
/// facts fail closed; this helper never invents a name-only identity.
pub fn inspect_installed(name: &str) -> Result<InstalledSkillFacts, AppError> {
    content::validate_skill_name(name)?;
    let snapshot = content::snapshot_materialized(name)?;
    let is_local = local_skill::is_local_skill(name);
    let local_id = if is_local {
        let dir = skillstar_core::infra::paths::local_skills_dir().join(name);
        Some(local_identity::ensure_local_identity(&dir)?)
    } else {
        None
    };
    let lock = lock_provenance(name)?;
    let git_head = if is_local {
        None
    } else {
        match lock.as_ref() {
            Some(entry) => Some(git_head_facts(name, entry)?),
            None => None,
        }
    };
    if !is_local && lock.is_none() {
        return Err(AppError::Other(format!(
            "Installed Skill '{name}' has no local identity sidecar and no Git lock provenance"
        )));
    }
    Ok(InstalledSkillFacts {
        installed_name: name.to_string(),
        is_local,
        local_id,
        lock,
        git_head,
        snapshot: SnapshotFacts::from_snapshot(&snapshot),
    })
}

pub fn canonical_git_repository(git_url: &str) -> Result<String, AppError> {
    let source = crate::source_resolver::Source::parse(git_url).map_err(|error| {
        AppError::Other(format!("Skill Git URL is not a usable repository: {error}"))
    })?;
    credential_free_repository(&source.repo_url)
}

pub fn credential_free_repository(repo_url: &str) -> Result<String, AppError> {
    let trimmed = repo_url.trim();
    if trimmed.is_empty() {
        return Err(AppError::Other(
            "Skill Git repository URL is empty".to_string(),
        ));
    }
    if trimmed.contains(['?', '#']) {
        return Err(AppError::Other(
            "Skill Git repository identity cannot include a query or fragment".to_string(),
        ));
    }
    if let Some(path) = trimmed.strip_prefix("file://") {
        let path = path.trim_end_matches('/');
        return Ok(format!("file://{path}").to_ascii_lowercase());
    }
    if let Some((scheme, rest)) = trimmed.split_once("://") {
        let (authority, path) = rest.split_once('/').ok_or_else(|| {
            AppError::Other("Skill Git repository URL is missing a path".to_string())
        })?;
        if authority.contains('@') && !scheme.eq_ignore_ascii_case("ssh") {
            let hostport = authority
                .rsplit_once('@')
                .map(|(_, host)| host)
                .ok_or_else(|| {
                    AppError::Other(
                        "Skill Git repository identity cannot include userinfo".to_string(),
                    )
                })?;
            return Ok(
                format!("{}://{}/{}", scheme, hostport, path.trim_end_matches('/'))
                    .to_ascii_lowercase(),
            );
        }
        if scheme.eq_ignore_ascii_case("ssh") {
            return Err(AppError::Other(
                "Skill Git repository identity cannot keep an ssh:// userinfo form; canonicalize first"
                    .to_string(),
            ));
        }
        return Ok(
            format!("{}://{}/{}", scheme, authority, path.trim_end_matches('/'))
                .to_ascii_lowercase(),
        );
    }
    Ok(crate::source_resolver::normalize_remote_url(trimmed))
}

fn lock_provenance(name: &str) -> Result<Option<LockProvenance>, AppError> {
    let lockfile = lockfile::Lockfile::load(&lockfile::lockfile_path())
        .map_err(|error| AppError::Other(format!("Failed to read Skill lockfile: {error}")))?;
    let matches = lockfile
        .skills
        .iter()
        .filter(|entry| {
            entry.name == name || cfg!(windows) && entry.name.eq_ignore_ascii_case(name)
        })
        .collect::<Vec<_>>();
    if matches.len() > 1 {
        return Err(AppError::Other(format!(
            "Installed Skill '{name}' has duplicate lockfile entries"
        )));
    }
    let Some(entry) = matches.into_iter().next() else {
        return Ok(None);
    };
    if entry.git_url.trim().is_empty() {
        return Ok(None);
    }
    let canonical_repository = canonical_git_repository(&entry.git_url)?;
    Ok(Some(LockProvenance {
        git_url: entry.git_url.clone(),
        canonical_repository,
        git_ref: entry.git_ref.clone(),
        source_folder: entry.source_folder.clone(),
        tree_hash: entry.tree_hash.clone(),
        content_hash: entry.content_hash.clone(),
        content_hash_version: entry.content_hash_version,
        baseline_unknown: entry.content_hash.is_none(),
    }))
}

fn git_head_facts(name: &str, lock: &LockProvenance) -> Result<GitHeadFacts, AppError> {
    let skill_entry = skillstar_core::infra::paths::hub_skills_dir().join(name);
    let repo_root = repo_link::repo_root_of(&skill_entry)
        .or_else(|| crate::git::ops::find_repo_root(&skill_entry))
        .ok_or_else(|| {
            AppError::Other(format!(
                "Installed Skill '{name}' is Git-backed but has no repository root"
            ))
        })?;
    let commit_sha = crate::git::ops::head_revision(&repo_root).map_err(|error| {
        AppError::Other(format!(
            "Failed to read HEAD commit for Skill '{name}': {error}"
        ))
    })?;
    let commit_sha = normalize_git_oid(&commit_sha, "commit")?;
    let tree_hash = crate::git::ops::compute_tree_hash(&repo_root).map_err(|error| {
        AppError::Other(format!(
            "Failed to read HEAD tree for Skill '{name}': {error}"
        ))
    })?;
    let tree_hash = normalize_git_oid(&tree_hash, "tree")?;
    let content_root = lock.source_folder.as_deref().unwrap_or("");
    let content_root_tree = if content_root.is_empty() {
        tree_hash.clone()
    } else {
        normalize_git_oid(
            &crate::git::ops::compute_subtree_hash(&repo_root, content_root).map_err(|error| {
                AppError::Other(format!(
                    "Failed to read content-root tree for Skill '{name}': {error}"
                ))
            })?,
            "tree",
        )?
    };
    Ok(GitHeadFacts {
        commit_sha,
        tree_hash,
        content_root_tree,
    })
}

fn normalize_git_oid(value: &str, kind: &str) -> Result<String, AppError> {
    let value = value.trim().to_ascii_lowercase();
    if value.len() != 40 || !value.chars().all(|ch| ch.is_ascii_hexdigit()) {
        return Err(AppError::Other(format!(
            "Skill Git {kind} is not a 40-hex object id: {value:?}"
        )));
    }
    Ok(value)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn github_userinfo_is_stripped_from_canonical_repository() {
        let canonical = credential_free_repository("https://github.com/Owner/Repo.git").unwrap();
        assert_eq!(canonical, "https://github.com/owner/repo.git");
        let with_user =
            credential_free_repository("https://token:x@github.com/Owner/Repo.git").unwrap();
        assert_eq!(with_user, "https://github.com/owner/repo.git");
        assert!(credential_free_repository("https://github.com/o/r.git?token=1").is_err());
    }
}
