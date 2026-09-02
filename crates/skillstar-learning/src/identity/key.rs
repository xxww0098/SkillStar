//! Domain-separated SHA-256 keys for skill identity and revision.

use sha2::{Digest, Sha256};

use super::{
    ContentRevision, GitTrackingRef, SkillIdentity, SkillIdentityKey, SkillIdentitySource,
    SkillRevision, SkillRevisionKey, SkillSourceRevision,
};

const IDENTITY_DOMAIN: &[u8] = b"skillstar.skill-identity.v1\0";
const REVISION_DOMAIN: &[u8] = b"skillstar.skill-revision.v1\0";

const TAG_GIT: u8 = 1;
const TAG_LOCAL: u8 = 2;
const TAG_CHANNEL: u8 = 3;
const TAG_DEFAULT_BRANCH: u8 = 1;
const TAG_NAMED_REF: u8 = 2;

pub fn identity_key(source: &SkillIdentitySource) -> SkillIdentityKey {
    let mut hasher = Sha256::new();
    hasher.update(IDENTITY_DOMAIN);
    match source {
        SkillIdentitySource::Git {
            repository,
            tracking_ref,
            content_root,
        } => {
            hasher.update([TAG_GIT]);
            write_str(&mut hasher, repository);
            match tracking_ref {
                GitTrackingRef::DefaultBranch => hasher.update([TAG_DEFAULT_BRANCH]),
                GitTrackingRef::Named { name } => {
                    hasher.update([TAG_NAMED_REF]);
                    write_str(&mut hasher, name);
                }
            }
            write_str(&mut hasher, content_root);
        }
        SkillIdentitySource::Local { local_id } => {
            hasher.update([TAG_LOCAL]);
            hasher.update(local_id.as_bytes());
        }
        SkillIdentitySource::Channel {
            repository_id,
            content_root,
        } => {
            hasher.update([TAG_CHANNEL]);
            hasher.update(repository_id.to_be_bytes());
            write_str(&mut hasher, content_root);
        }
    }
    SkillIdentityKey::from_digest(&hasher.finalize())
}

pub fn revision_key(
    identity: &SkillIdentity,
    revision: &SkillRevisionParts<'_>,
) -> SkillRevisionKey {
    let mut hasher = Sha256::new();
    hasher.update(REVISION_DOMAIN);
    write_str(&mut hasher, identity.key.as_str());
    match revision.source {
        SkillSourceRevision::Git {
            commit_sha,
            tree_hash,
        } => {
            hasher.update([TAG_GIT]);
            write_str(&mut hasher, commit_sha);
            write_str(&mut hasher, tree_hash);
        }
        SkillSourceRevision::Local => hasher.update([TAG_LOCAL]),
        SkillSourceRevision::Channel { commit_sha, .. } => {
            hasher.update([TAG_CHANNEL]);
            write_str(&mut hasher, commit_sha);
        }
    }
    hasher.update(revision.content.hash_version.to_be_bytes());
    write_str(&mut hasher, &revision.content.content_hash);
    SkillRevisionKey::from_digest(&hasher.finalize())
}

pub struct SkillRevisionParts<'a> {
    pub content: &'a ContentRevision,
    pub source: &'a SkillSourceRevision,
}

impl<'a> From<&'a SkillRevision> for SkillRevisionParts<'a> {
    fn from(revision: &'a SkillRevision) -> Self {
        Self {
            content: &revision.content,
            source: &revision.source,
        }
    }
}

fn write_str(hasher: &mut Sha256, value: &str) {
    hasher.update((value.len() as u64).to_be_bytes());
    hasher.update(value.as_bytes());
}
