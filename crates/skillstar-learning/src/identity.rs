//! Source-compound skill identity and exact content revision.

mod key;

use serde::{Deserialize, Serialize};
use skillstar_core::infra::error::AppError;
use uuid::Uuid;

use self::key::SkillRevisionParts;

const SNAPSHOT_HASH_VERSION: u32 = 2;

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct SkillIdentityKey(String);

impl SkillIdentityKey {
    fn from_digest(digest: &[u8]) -> Self {
        Self(format!("ski:v1:{}", hex_digest(digest)))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn storage_segment(&self) -> String {
        self.0.replace(':', "-")
    }
}

impl AsRef<str> for SkillIdentityKey {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct SkillRevisionKey(String);

impl SkillRevisionKey {
    fn from_digest(digest: &[u8]) -> Self {
        Self(format!("skr:v1:{}", hex_digest(digest)))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillIdentity {
    pub key: SkillIdentityKey,
    pub source: SkillIdentitySource,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum SkillIdentitySource {
    #[serde(rename_all = "camelCase")]
    Git {
        repository: String,
        tracking_ref: GitTrackingRef,
        content_root: String,
    },
    #[serde(rename_all = "camelCase")]
    Local { local_id: Uuid },
    #[serde(rename_all = "camelCase")]
    Channel {
        repository_id: u64,
        content_root: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum GitTrackingRef {
    DefaultBranch,
    Named { name: String },
}

impl GitTrackingRef {
    pub fn from_lock_ref(git_ref: Option<&str>) -> Result<Self, AppError> {
        match git_ref.map(str::trim).filter(|value| !value.is_empty()) {
            None => Ok(Self::DefaultBranch),
            Some(name) => {
                if name.contains('\0') {
                    return Err(AppError::Other(
                        "Git tracking ref cannot contain a NUL byte".to_string(),
                    ));
                }
                Ok(Self::Named {
                    name: name.to_string(),
                })
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillRevision {
    pub key: SkillRevisionKey,
    pub skill_key: SkillIdentityKey,
    pub content: ContentRevision,
    pub source: SkillSourceRevision,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ContentRevision {
    pub hash_version: u32,
    pub content_hash: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum SkillSourceRevision {
    #[serde(rename_all = "camelCase")]
    Git {
        commit_sha: String,
        tree_hash: String,
    },
    Local,
    #[serde(rename_all = "camelCase")]
    Channel {
        commit_sha: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        release: Option<ChannelReleaseRef>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChannelReleaseRef {
    pub revision: u64,
    pub tag_name: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResolvedSkill {
    pub identity: SkillIdentity,
    pub revision: SkillRevision,
    pub display_name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub installed_name: Option<String>,
}

impl SkillIdentity {
    pub fn git(
        repository: impl Into<String>,
        tracking_ref: GitTrackingRef,
        content_root: impl Into<String>,
    ) -> Result<Self, AppError> {
        let source = SkillIdentitySource::Git {
            repository: normalize_repository(&repository.into())?,
            tracking_ref: normalize_tracking_ref(tracking_ref)?,
            content_root: normalize_content_root(&content_root.into())?,
        };
        Ok(Self {
            key: key::identity_key(&source),
            source,
        })
    }

    pub fn local(local_id: Uuid) -> Result<Self, AppError> {
        if local_id.is_nil() {
            return Err(AppError::Other(
                "Local Skill identity cannot be a nil UUID".to_string(),
            ));
        }
        let source = SkillIdentitySource::Local { local_id };
        Ok(Self {
            key: key::identity_key(&source),
            source,
        })
    }

    pub fn channel(repository_id: u64, content_root: impl Into<String>) -> Result<Self, AppError> {
        if repository_id == 0 {
            return Err(AppError::Other(
                "Channel Skill identity requires a non-zero GitHub repository ID".to_string(),
            ));
        }
        let source = SkillIdentitySource::Channel {
            repository_id,
            content_root: normalize_content_root(&content_root.into())?,
        };
        Ok(Self {
            key: key::identity_key(&source),
            source,
        })
    }

    pub fn verified(self) -> Result<Self, AppError> {
        let expected = key::identity_key(&self.source);
        if expected != self.key {
            return Err(AppError::Other(
                "Stored Skill identity key does not match its source fields".to_string(),
            ));
        }
        match &self.source {
            SkillIdentitySource::Git {
                repository,
                tracking_ref,
                content_root,
            } => {
                normalize_repository(repository)?;
                normalize_tracking_ref(tracking_ref.clone())?;
                normalize_content_root(content_root)?;
            }
            SkillIdentitySource::Local { local_id } => {
                if local_id.is_nil() {
                    return Err(AppError::Other(
                        "Local Skill identity cannot be a nil UUID".to_string(),
                    ));
                }
            }
            SkillIdentitySource::Channel {
                repository_id,
                content_root,
            } => {
                if *repository_id == 0 {
                    return Err(AppError::Other(
                        "Channel Skill identity requires a non-zero GitHub repository ID"
                            .to_string(),
                    ));
                }
                normalize_content_root(content_root)?;
            }
        }
        Ok(self)
    }
}

impl SkillRevision {
    pub fn git(
        identity: &SkillIdentity,
        commit_sha: impl Into<String>,
        tree_hash: impl Into<String>,
        content: ContentRevision,
    ) -> Result<Self, AppError> {
        let SkillIdentitySource::Git { .. } = identity.source else {
            return Err(AppError::Other(
                "Git Skill revision requires a Git identity".to_string(),
            ));
        };
        Self::build(
            identity,
            content,
            SkillSourceRevision::Git {
                commit_sha: normalize_git_oid(&commit_sha.into(), "commit")?,
                tree_hash: normalize_git_oid(&tree_hash.into(), "tree")?,
            },
        )
    }

    pub fn local(identity: &SkillIdentity, content: ContentRevision) -> Result<Self, AppError> {
        let SkillIdentitySource::Local { .. } = identity.source else {
            return Err(AppError::Other(
                "Local Skill revision requires a local identity".to_string(),
            ));
        };
        Self::build(identity, content, SkillSourceRevision::Local)
    }

    pub fn channel(
        identity: &SkillIdentity,
        commit_sha: impl Into<String>,
        release: Option<ChannelReleaseRef>,
        content: ContentRevision,
    ) -> Result<Self, AppError> {
        let SkillIdentitySource::Channel { .. } = identity.source else {
            return Err(AppError::Other(
                "Channel Skill revision requires a channel identity".to_string(),
            ));
        };
        Self::build(
            identity,
            content,
            SkillSourceRevision::Channel {
                commit_sha: normalize_git_oid(&commit_sha.into(), "commit")?,
                release,
            },
        )
    }

    fn build(
        identity: &SkillIdentity,
        content: ContentRevision,
        source: SkillSourceRevision,
    ) -> Result<Self, AppError> {
        let content = content.verified()?;
        let revision = Self {
            key: key::revision_key(
                identity,
                &SkillRevisionParts {
                    content: &content,
                    source: &source,
                },
            ),
            skill_key: identity.key.clone(),
            content,
            source,
        };
        revision.verified(identity)
    }

    pub fn verified(self, identity: &SkillIdentity) -> Result<Self, AppError> {
        if self.skill_key != identity.key {
            return Err(AppError::Other(
                "Skill revision is bound to a different identity key".to_string(),
            ));
        }
        match (&identity.source, &self.source) {
            (SkillIdentitySource::Git { .. }, SkillSourceRevision::Git { .. })
            | (SkillIdentitySource::Local { .. }, SkillSourceRevision::Local)
            | (SkillIdentitySource::Channel { .. }, SkillSourceRevision::Channel { .. }) => {}
            _ => {
                return Err(AppError::Other(
                    "Skill identity source variant does not match revision source variant"
                        .to_string(),
                ));
            }
        }
        let expected = key::revision_key(identity, &SkillRevisionParts::from(&self));
        if expected != self.key {
            return Err(AppError::Other(
                "Stored Skill revision key does not match its fields".to_string(),
            ));
        }
        self.content.clone().verified()?;
        match &self.source {
            SkillSourceRevision::Git {
                commit_sha,
                tree_hash,
            } => {
                normalize_git_oid(commit_sha, "commit")?;
                normalize_git_oid(tree_hash, "tree")?;
            }
            SkillSourceRevision::Local => {}
            SkillSourceRevision::Channel { commit_sha, .. } => {
                normalize_git_oid(commit_sha, "commit")?;
            }
        }
        Ok(self)
    }
}

impl ContentRevision {
    pub fn new(hash_version: u32, content_hash: impl Into<String>) -> Result<Self, AppError> {
        Self {
            hash_version,
            content_hash: content_hash.into(),
        }
        .verified()
    }

    pub fn verified(self) -> Result<Self, AppError> {
        if self.hash_version != SNAPSHOT_HASH_VERSION {
            return Err(AppError::Other(format!(
                "Skill content hash version must be {SNAPSHOT_HASH_VERSION}, found {}",
                self.hash_version
            )));
        }
        let hash = self.content_hash.trim();
        let Some(hex) = hash.strip_prefix("sha256:") else {
            return Err(AppError::Other(format!(
                "Skill content hash must be sha256-prefixed: {:?}",
                self.content_hash
            )));
        };
        if hex.len() != 64
            || hex
                .chars()
                .any(|ch| !ch.is_ascii_hexdigit() || ch.is_ascii_uppercase())
        {
            return Err(AppError::Other(format!(
                "Skill content hash must be sha256:<64 lowercase hex>: {:?}",
                self.content_hash
            )));
        }
        Ok(Self {
            hash_version: self.hash_version,
            content_hash: hash.to_string(),
        })
    }
}

impl ResolvedSkill {
    pub fn new(
        identity: SkillIdentity,
        revision: SkillRevision,
        display_name: impl Into<String>,
        installed_name: Option<String>,
    ) -> Result<Self, AppError> {
        let identity = identity.verified()?;
        let revision = revision.verified(&identity)?;
        let display_name = display_name.into();
        if display_name.trim().is_empty() {
            return Err(AppError::Other(
                "Resolved Skill display name cannot be empty".to_string(),
            ));
        }
        Ok(Self {
            identity,
            revision,
            display_name,
            installed_name,
        })
    }
}

pub fn normalize_content_root(raw: &str) -> Result<String, AppError> {
    let raw = raw.replace('\\', "/");
    let raw = raw.trim();
    if raw.is_empty() || raw == "/" {
        return Ok(String::new());
    }
    if raw.starts_with('/') || raw.contains('\0') {
        return Err(AppError::Other(format!(
            "Skill content root must be a relative path: {raw:?}"
        )));
    }
    let mut parts = Vec::new();
    for part in raw.split('/') {
        if part.is_empty() {
            continue;
        }
        if part == "." || part == ".." {
            return Err(AppError::Other(format!(
                "Skill content root cannot contain '.' or '..': {raw:?}"
            )));
        }
        parts.push(part);
    }
    Ok(parts.join("/"))
}

fn normalize_repository(raw: &str) -> Result<String, AppError> {
    let value = raw.trim();
    if value.is_empty() {
        return Err(AppError::Other(
            "Git repository identity cannot be empty".to_string(),
        ));
    }
    if value.contains(['?', '#', '\0']) {
        return Err(AppError::Other(
            "Git repository identity cannot include query, fragment, or NUL".to_string(),
        ));
    }
    if let Some((_, rest)) = value.split_once("://")
        && let Some(authority) = rest.split('/').next()
        && authority.contains('@')
        && !value.to_ascii_lowercase().starts_with("file://")
    {
        return Err(AppError::Other(
            "Git repository identity cannot include userinfo".to_string(),
        ));
    }
    Ok(value.to_string())
}

fn normalize_tracking_ref(tracking_ref: GitTrackingRef) -> Result<GitTrackingRef, AppError> {
    match tracking_ref {
        GitTrackingRef::DefaultBranch => Ok(GitTrackingRef::DefaultBranch),
        GitTrackingRef::Named { name } => {
            let name = name.trim();
            if name.is_empty() {
                return Err(AppError::Other(
                    "Named Git tracking ref cannot be empty".to_string(),
                ));
            }
            if name.contains('\0') {
                return Err(AppError::Other(
                    "Git tracking ref cannot contain a NUL byte".to_string(),
                ));
            }
            Ok(GitTrackingRef::Named {
                name: name.to_string(),
            })
        }
    }
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

fn hex_digest(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        output.push(HEX[(byte >> 4) as usize] as char);
        output.push(HEX[(byte & 0x0f) as usize] as char);
    }
    output
}

#[cfg(test)]
mod tests;
