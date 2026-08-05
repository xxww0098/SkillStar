//! Explicit immutable shared-channel publication.

use super::{
    RemoteRepository, SharedChannelError, SharedChannelErrorCode, SharedChannelGateway,
    SharedChannelRegistry, SharedChannelRole, SharedChannelStatus, project_role,
    validate_remote_repository,
};
use async_trait::async_trait;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Component, Path};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

pub const CHANNEL_RELEASE_MANIFEST_VERSION: u32 = 1;
pub const CHANNEL_CONTENT_HASH_VERSION: u32 = crate::content::SNAPSHOT_HASH_VERSION;
const MAX_PUBLISH_SESSIONS: usize = 64;
const PUBLISH_PREVIEW_TTL: Duration = Duration::from_secs(30 * 60);
const MAX_CHANNEL_SKILLS: usize = 4_096;
const MAX_RELEASE_TITLE_BYTES: usize = 120;
const MAX_RELEASE_NOTES_BYTES: usize = 20_000;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ChannelPublisherIdentity {
    pub id: u64,
    pub login: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChannelSkillReleaseStatus {
    Added,
    Updated,
    Unchanged,
    Removed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ChannelReleaseSkill {
    pub id: String,
    pub content_root: String,
    pub content_hash: String,
    pub content_hash_version: u32,
    pub status: ChannelSkillReleaseStatus,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ChannelReleaseManifest {
    pub schema_version: u32,
    pub repository_id: u64,
    pub organization_id: u64,
    pub revision: u64,
    pub tag_name: String,
    pub commit_sha: String,
    pub publisher: ChannelPublisherIdentity,
    pub published_at: String,
    pub title: String,
    pub notes: String,
    pub skills: Vec<ChannelReleaseSkill>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChannelDraftSkill {
    pub id: String,
    pub content_root: String,
    pub content_hash: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChannelDraftSnapshot {
    pub commit_sha: String,
    pub skills: Vec<ChannelDraftSkill>,
}

pub trait ChannelDraftScanner: Send + Sync + 'static {
    fn scan(
        &self,
        repository: &RemoteRepository,
    ) -> Result<ChannelDraftSnapshot, SharedChannelError>;
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChannelPublishPreview {
    pub session_id: String,
    pub repository_id: u64,
    pub commit_sha: String,
    pub next_revision: u64,
    pub tag_name: String,
    pub changes: Vec<ChannelReleaseSkill>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RemoteChannelRelease {
    pub id: u64,
    pub html_url: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChannelPublishResult {
    pub manifest: ChannelReleaseManifest,
    pub release: RemoteChannelRelease,
}

#[async_trait]
pub trait ChannelPublicationGateway: Send + Sync {
    async fn publisher_identity(&self) -> Result<ChannelPublisherIdentity, SharedChannelError>;
    async fn head_commit(
        &self,
        repository: &RemoteRepository,
    ) -> Result<String, SharedChannelError>;
    async fn published_manifests(
        &self,
        repository: &RemoteRepository,
    ) -> Result<Vec<ChannelReleaseManifest>, SharedChannelError>;
    async fn highest_reserved_revision(
        &self,
        repository: &RemoteRepository,
    ) -> Result<u64, SharedChannelError> {
        Ok(self
            .published_manifests(repository)
            .await?
            .iter()
            .map(|manifest| manifest.revision)
            .max()
            .unwrap_or(0))
    }
    async fn publish_immutable(
        &self,
        repository: &RemoteRepository,
        manifest: &ChannelReleaseManifest,
    ) -> Result<RemoteChannelRelease, SharedChannelError>;
}

#[derive(Clone, Default)]
pub struct ChannelPublishSessions {
    inner: Arc<Mutex<PublishSessionStore>>,
}

#[derive(Default)]
struct PublishSessionStore {
    next_generation: u64,
    sessions: HashMap<String, PublishSessionState>,
}

enum PublishSessionState {
    Scanning {
        generation: u64,
    },
    Cancelled {
        generation: u64,
    },
    Preview {
        generation: u64,
        preview: StoredPublishPreview,
    },
    Publishing {
        generation: u64,
        preview: StoredPublishPreview,
    },
}

#[derive(Clone)]
struct StoredPublishPreview {
    public: ChannelPublishPreview,
    repository: RemoteRepository,
    touched_at: Instant,
}

impl ChannelPublishSessions {
    fn reserve(&self, session_id: &str) -> Result<u64, SharedChannelError> {
        let mut store = lock_sessions(&self.inner);
        let now = Instant::now();
        store
            .sessions
            .retain(|_, state| !preview_expired(state, now));
        if store.sessions.contains_key(session_id) || store.sessions.len() >= MAX_PUBLISH_SESSIONS {
            return Err(SharedChannelError::new(
                SharedChannelErrorCode::RegistrationSessionConflict,
                "The channel publish session is already active or the session limit was reached",
            ));
        }
        store.next_generation = store.next_generation.wrapping_add(1).max(1);
        let generation = store.next_generation;
        store.sessions.insert(
            session_id.to_string(),
            PublishSessionState::Scanning { generation },
        );
        Ok(generation)
    }

    fn is_scanning(&self, session_id: &str, generation: u64) -> bool {
        matches!(
            lock_sessions(&self.inner).sessions.get(session_id),
            Some(PublishSessionState::Scanning { generation: current }) if *current == generation
        )
    }

    fn complete(
        &self,
        session_id: &str,
        generation: u64,
        preview: StoredPublishPreview,
    ) -> Result<ChannelPublishPreview, SharedChannelError> {
        let mut store = lock_sessions(&self.inner);
        match store.sessions.remove(session_id) {
            Some(PublishSessionState::Scanning {
                generation: current,
            }) if current == generation => {
                let public = preview.public.clone();
                store.sessions.insert(
                    session_id.to_string(),
                    PublishSessionState::Preview {
                        generation,
                        preview,
                    },
                );
                Ok(public)
            }
            Some(PublishSessionState::Cancelled {
                generation: current,
            }) if current == generation => Err(cancelled_error()),
            Some(state) => {
                store.sessions.insert(session_id.to_string(), state);
                Err(cancelled_error())
            }
            None => Err(cancelled_error()),
        }
    }

    fn abandon(&self, session_id: &str, generation: u64) {
        let mut store = lock_sessions(&self.inner);
        let should_remove = matches!(
            store.sessions.get(session_id),
            Some(PublishSessionState::Scanning { generation: current }
                | PublishSessionState::Cancelled { generation: current }) if *current == generation
        );
        if should_remove {
            store.sessions.remove(session_id);
        }
    }

    fn claim(&self, session_id: &str) -> Result<(u64, StoredPublishPreview), SharedChannelError> {
        let mut store = lock_sessions(&self.inner);
        let (generation, preview) = match store.sessions.remove(session_id) {
            Some(PublishSessionState::Preview {
                generation,
                preview,
            }) if !stored_preview_expired(&preview, Instant::now()) => (generation, preview),
            Some(PublishSessionState::Preview { .. }) => return Err(publish_session_not_found()),
            Some(state) => {
                store.sessions.insert(session_id.to_string(), state);
                return Err(publish_session_not_found());
            }
            None => return Err(publish_session_not_found()),
        };
        store.sessions.insert(
            session_id.to_string(),
            PublishSessionState::Publishing {
                generation,
                preview: preview.clone(),
            },
        );
        Ok((generation, preview))
    }

    fn restore(&self, session_id: &str, generation: u64) {
        let mut store = lock_sessions(&self.inner);
        let (current, mut preview) = match store.sessions.remove(session_id) {
            Some(PublishSessionState::Publishing {
                generation,
                preview,
            }) => (generation, preview),
            Some(state) => {
                store.sessions.insert(session_id.to_string(), state);
                return;
            }
            None => return,
        };
        preview.touched_at = Instant::now();
        let state = if current == generation {
            PublishSessionState::Preview {
                generation,
                preview,
            }
        } else {
            PublishSessionState::Publishing {
                generation: current,
                preview,
            }
        };
        store.sessions.insert(session_id.to_string(), state);
    }

    fn finish(&self, session_id: &str, generation: u64) -> Result<(), SharedChannelError> {
        let mut store = lock_sessions(&self.inner);
        match store.sessions.remove(session_id) {
            Some(PublishSessionState::Publishing {
                generation: current,
                ..
            }) if current == generation => Ok(()),
            Some(state) => {
                store.sessions.insert(session_id.to_string(), state);
                Err(publish_session_not_found())
            }
            None => Err(publish_session_not_found()),
        }
    }

    pub fn cancel(&self, session_id: &str) -> bool {
        let mut store = lock_sessions(&self.inner);
        match store.sessions.remove(session_id) {
            Some(PublishSessionState::Scanning { generation }) => {
                store.sessions.insert(
                    session_id.to_string(),
                    PublishSessionState::Cancelled { generation },
                );
                true
            }
            Some(PublishSessionState::Cancelled { generation }) => {
                store.sessions.insert(
                    session_id.to_string(),
                    PublishSessionState::Cancelled { generation },
                );
                true
            }
            Some(PublishSessionState::Preview { .. }) => true,
            Some(state @ PublishSessionState::Publishing { .. }) => {
                store.sessions.insert(session_id.to_string(), state);
                false
            }
            None => false,
        }
    }

    pub fn clear(&self) {
        lock_sessions(&self.inner).sessions.clear();
    }
}

pub struct ChannelPublicationFacade<G, R> {
    gateway: G,
    registry: R,
    scanner: Option<Arc<dyn ChannelDraftScanner>>,
    sessions: ChannelPublishSessions,
}

impl<G, R> ChannelPublicationFacade<G, R>
where
    G: SharedChannelGateway + ChannelPublicationGateway,
    R: SharedChannelRegistry,
{
    pub fn new<S>(gateway: G, registry: R, scanner: S, sessions: ChannelPublishSessions) -> Self
    where
        S: ChannelDraftScanner,
    {
        Self {
            gateway,
            registry,
            scanner: Some(Arc::new(scanner)),
            sessions,
        }
    }

    pub fn without_scanner(gateway: G, registry: R, sessions: ChannelPublishSessions) -> Self {
        Self {
            gateway,
            registry,
            scanner: None,
            sessions,
        }
    }

    pub async fn preview(
        &self,
        repository_id: u64,
        session_id: String,
    ) -> Result<ChannelPublishPreview, SharedChannelError> {
        validate_session_id(&session_id)?;
        let generation = self.sessions.reserve(&session_id)?;
        let result = self
            .preview_reserved(repository_id, session_id.clone(), generation)
            .await;
        if result.is_err() {
            self.sessions.abandon(&session_id, generation);
        }
        result
    }

    async fn preview_reserved(
        &self,
        repository_id: u64,
        session_id: String,
        generation: u64,
    ) -> Result<ChannelPublishPreview, SharedChannelError> {
        let channel = self
            .registry
            .load()?
            .channels
            .into_iter()
            .find(|channel| channel.repository_id == repository_id)
            .ok_or_else(|| repository_not_found(repository_id))?;
        if channel.status != SharedChannelStatus::Active {
            return Err(SharedChannelError::new(
                SharedChannelErrorCode::PermissionDenied,
                "The shared channel must be active before it can be published",
            ));
        }
        let organization = self
            .gateway
            .list_organizations()
            .await?
            .into_iter()
            .find(|organization| organization.id == channel.organization_id)
            .ok_or_else(|| {
                SharedChannelError::new(
                    SharedChannelErrorCode::OrganizationUnavailable,
                    "The channel organization is unavailable to this GitHub identity",
                )
            })?;
        let repository = self
            .gateway
            .get_selected_repository(channel.organization_id, repository_id)
            .await?;
        validate_remote_repository(&repository, &organization, repository_id)?;
        require_publish_role(&repository)?;
        if !self.sessions.is_scanning(&session_id, generation) {
            return Err(cancelled_error());
        }

        let scanner = self.scanner.clone().ok_or_else(|| {
            SharedChannelError::new(
                SharedChannelErrorCode::Protocol,
                "The channel publication facade is not configured for draft scanning",
            )
        })?;
        let repository_for_scan = repository.clone();
        let snapshot = tokio::task::spawn_blocking(move || scanner.scan(&repository_for_scan))
            .await
            .map_err(|_| {
                SharedChannelError::new(
                    SharedChannelErrorCode::Protocol,
                    "The channel draft scan stopped unexpectedly",
                )
            })??;
        validate_draft_snapshot(&snapshot)?;
        let head = self.gateway.head_commit(&repository).await?;
        validate_commit_sha(&head)?;
        if head != snapshot.commit_sha {
            return Err(draft_changed_error());
        }
        let manifests = self.gateway.published_manifests(&repository).await?;
        let previous = latest_manifest(&manifests, repository_id, channel.organization_id)?;
        let highest_reserved = self.gateway.highest_reserved_revision(&repository).await?;
        let next_revision = next_revision(
            previous.as_ref().map_or(0, |manifest| manifest.revision),
            highest_reserved,
        )?;
        let tag_name = revision_tag(next_revision);
        let changes = build_changes(&snapshot.skills, previous.as_ref())?;
        let public = ChannelPublishPreview {
            session_id: session_id.clone(),
            repository_id,
            commit_sha: snapshot.commit_sha,
            next_revision,
            tag_name,
            changes,
        };
        self.sessions.complete(
            &session_id,
            generation,
            StoredPublishPreview {
                public,
                repository,
                touched_at: Instant::now(),
            },
        )
    }

    pub async fn publish(
        &self,
        session_id: &str,
        title: String,
        notes: String,
    ) -> Result<ChannelPublishResult, SharedChannelError> {
        validate_session_id(session_id)?;
        validate_release_text(&title, &notes)?;
        let (generation, preview) = self.sessions.claim(session_id)?;
        let result = self
            .publish_claimed(session_id, generation, &preview, title, notes)
            .await;
        if result.is_err() {
            self.sessions.restore(session_id, generation);
        }
        result
    }

    async fn publish_claimed(
        &self,
        session_id: &str,
        generation: u64,
        preview: &StoredPublishPreview,
        title: String,
        notes: String,
    ) -> Result<ChannelPublishResult, SharedChannelError> {
        let organization = self
            .gateway
            .list_organizations()
            .await?
            .into_iter()
            .find(|organization| organization.id == preview.repository.owner_id)
            .ok_or_else(|| {
                SharedChannelError::new(
                    SharedChannelErrorCode::OrganizationUnavailable,
                    "The channel organization is unavailable to this GitHub identity",
                )
            })?;
        let repository = self
            .gateway
            .get_selected_repository(preview.repository.owner_id, preview.repository.id)
            .await?;
        validate_remote_repository(&repository, &organization, preview.repository.id)?;
        require_publish_role(&repository)?;
        let head = self.gateway.head_commit(&repository).await?;
        validate_commit_sha(&head)?;
        if head != preview.public.commit_sha {
            return Err(draft_changed_error());
        }
        let manifests = self.gateway.published_manifests(&repository).await?;
        let latest = latest_manifest(&manifests, repository.id, preview.repository.owner_id)?;
        let highest_reserved = self.gateway.highest_reserved_revision(&repository).await?;
        let expected_revision = next_revision(
            latest.as_ref().map_or(0, |manifest| manifest.revision),
            highest_reserved,
        )?;
        if expected_revision != preview.public.next_revision {
            return Err(SharedChannelError::new(
                SharedChannelErrorCode::ReleaseConflict,
                "Another publisher created a channel revision; scan the draft again",
            ));
        }
        let publisher = self.gateway.publisher_identity().await?;
        validate_publisher(&publisher)?;
        let manifest = ChannelReleaseManifest {
            schema_version: CHANNEL_RELEASE_MANIFEST_VERSION,
            repository_id: repository.id,
            organization_id: repository.owner_id,
            revision: preview.public.next_revision,
            tag_name: preview.public.tag_name.clone(),
            commit_sha: preview.public.commit_sha.clone(),
            publisher,
            published_at: Utc::now().to_rfc3339(),
            title,
            notes,
            skills: preview.public.changes.clone(),
        };
        validate_manifest(&manifest, repository.id, repository.owner_id)?;
        let release = self
            .gateway
            .publish_immutable(&repository, &manifest)
            .await?;
        // The remote Release is authoritative once GitHub accepts it. Logout or
        // cancellation may concurrently clear local session state, but must not
        // turn a successful immutable publication into a reported failure.
        let _ = self.sessions.finish(session_id, generation);
        Ok(ChannelPublishResult { manifest, release })
    }

    pub fn cancel(&self, session_id: &str) -> bool {
        self.sessions.cancel(session_id)
    }
}

fn require_publish_role(repository: &RemoteRepository) -> Result<(), SharedChannelError> {
    match project_role(&repository.permissions) {
        Some(SharedChannelRole::Owner | SharedChannelRole::Publisher) => Ok(()),
        _ => Err(SharedChannelError::new(
            SharedChannelErrorCode::PermissionDenied,
            "Repository Write, Maintain, or Admin permission is required to publish a channel release",
        )),
    }
}

fn build_changes(
    draft: &[ChannelDraftSkill],
    previous: Option<&ChannelReleaseManifest>,
) -> Result<Vec<ChannelReleaseSkill>, SharedChannelError> {
    let previous_skills = previous
        .map(|manifest| {
            manifest
                .skills
                .iter()
                .filter(|skill| skill.status != ChannelSkillReleaseStatus::Removed)
                .map(|skill| (skill.id.to_lowercase(), skill))
                .collect::<BTreeMap<_, _>>()
        })
        .unwrap_or_default();
    let mut seen = HashSet::new();
    let mut changes = Vec::new();
    for skill in draft {
        let identity = skill.id.to_lowercase();
        if !seen.insert(identity.clone()) {
            return Err(SharedChannelError::new(
                SharedChannelErrorCode::Integrity,
                "The channel draft contains duplicate Skill identities",
            ));
        }
        let status = match previous_skills.get(&identity) {
            None => ChannelSkillReleaseStatus::Added,
            Some(previous)
                if previous.id == skill.id
                    && previous.content_root == skill.content_root
                    && previous.content_hash == skill.content_hash =>
            {
                ChannelSkillReleaseStatus::Unchanged
            }
            Some(_) => ChannelSkillReleaseStatus::Updated,
        };
        changes.push(ChannelReleaseSkill {
            id: skill.id.clone(),
            content_root: skill.content_root.clone(),
            content_hash: skill.content_hash.clone(),
            content_hash_version: CHANNEL_CONTENT_HASH_VERSION,
            status,
        });
    }
    for (identity, skill) in previous_skills {
        if !seen.contains(&identity) {
            let mut removed = skill.clone();
            removed.status = ChannelSkillReleaseStatus::Removed;
            changes.push(removed);
        }
    }
    changes.sort_by(|left, right| left.id.cmp(&right.id));
    Ok(changes)
}

fn latest_manifest(
    manifests: &[ChannelReleaseManifest],
    repository_id: u64,
    organization_id: u64,
) -> Result<Option<ChannelReleaseManifest>, SharedChannelError> {
    for manifest in manifests {
        validate_manifest(manifest, repository_id, organization_id)?;
    }
    Ok(manifests
        .iter()
        .max_by_key(|manifest| manifest.revision)
        .cloned())
}

pub fn validate_manifest(
    manifest: &ChannelReleaseManifest,
    repository_id: u64,
    organization_id: u64,
) -> Result<(), SharedChannelError> {
    if manifest.schema_version != CHANNEL_RELEASE_MANIFEST_VERSION
        || manifest.repository_id != repository_id
        || manifest.organization_id != organization_id
        || manifest.revision == 0
        || manifest.tag_name != revision_tag(manifest.revision)
    {
        return Err(integrity_error(
            "The channel release manifest identity is invalid",
        ));
    }
    validate_commit_sha(&manifest.commit_sha)?;
    validate_publisher(&manifest.publisher)?;
    chrono::DateTime::parse_from_rfc3339(&manifest.published_at)
        .map_err(|_| integrity_error("The channel release timestamp is invalid"))?;
    validate_release_text(&manifest.title, &manifest.notes)
        .map_err(|_| integrity_error("The channel release title or notes are invalid"))?;
    let mut ids = HashSet::new();
    if manifest.skills.len() > MAX_CHANNEL_SKILLS {
        return Err(integrity_error(
            "The channel release manifest exceeds the Skill count limit",
        ));
    }
    for skill in &manifest.skills {
        if !ids.insert(skill.id.to_lowercase())
            || !valid_skill_id(&skill.id)
            || !valid_content_root(&skill.content_root)
            || !valid_content_hash(&skill.content_hash)
            || skill.content_hash_version != CHANNEL_CONTENT_HASH_VERSION
        {
            return Err(integrity_error(
                "The channel release Skill manifest is invalid",
            ));
        }
    }
    Ok(())
}

fn validate_draft_snapshot(snapshot: &ChannelDraftSnapshot) -> Result<(), SharedChannelError> {
    validate_commit_sha(&snapshot.commit_sha)?;
    let mut ids = HashSet::new();
    if snapshot.skills.len() > MAX_CHANNEL_SKILLS {
        return Err(integrity_error(
            "The channel draft exceeds the Skill count limit",
        ));
    }
    for skill in &snapshot.skills {
        if !ids.insert(skill.id.to_lowercase())
            || !valid_skill_id(&skill.id)
            || !valid_content_root(&skill.content_root)
            || !valid_content_hash(&skill.content_hash)
        {
            return Err(integrity_error(
                "The channel draft Skill snapshot is invalid",
            ));
        }
    }
    Ok(())
}

pub(super) fn validate_commit_sha(value: &str) -> Result<(), SharedChannelError> {
    if value.len() == 40 && value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        Ok(())
    } else {
        Err(integrity_error("The channel commit SHA is invalid"))
    }
}

pub(super) fn validate_publisher(
    identity: &ChannelPublisherIdentity,
) -> Result<(), SharedChannelError> {
    let valid = identity.id > 0
        && !identity.login.is_empty()
        && identity.login.len() <= 39
        && identity
            .login
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-');
    if valid {
        Ok(())
    } else {
        Err(integrity_error("The channel publisher identity is invalid"))
    }
}

pub(super) fn validate_release_text(title: &str, notes: &str) -> Result<(), SharedChannelError> {
    if title.trim().is_empty()
        || title.len() > MAX_RELEASE_TITLE_BYTES
        || notes.len() > MAX_RELEASE_NOTES_BYTES
        || title.chars().any(char::is_control)
        || notes.contains('\0')
    {
        Err(SharedChannelError::new(
            SharedChannelErrorCode::Protocol,
            "Release title is required and release text exceeds the supported limits",
        ))
    } else {
        Ok(())
    }
}

fn valid_skill_id(value: &str) -> bool {
    value.len() <= 128 && crate::content::validate_skill_name(value).is_ok()
}

pub(super) fn valid_content_root(value: &str) -> bool {
    if value.is_empty() {
        return true;
    }
    let path = Path::new(value);
    !path.is_absolute()
        && !value.contains('\\')
        && !value.chars().any(char::is_control)
        && path
            .components()
            .all(|component| matches!(component, Component::Normal(_)))
}

fn valid_content_hash(value: &str) -> bool {
    value.strip_prefix("sha256:").is_some_and(|digest| {
        digest.len() == 64 && digest.bytes().all(|byte| byte.is_ascii_hexdigit())
    })
}

fn validate_session_id(session_id: &str) -> Result<(), SharedChannelError> {
    uuid::Uuid::parse_str(session_id).map(|_| ()).map_err(|_| {
        SharedChannelError::new(
            SharedChannelErrorCode::RegistrationSessionConflict,
            "Channel publish session id must be a UUID",
        )
    })
}

pub fn revision_tag(revision: u64) -> String {
    format!("channel-v{revision:06}")
}

pub(super) fn parse_revision_tag(tag: &str) -> Option<u64> {
    let revision = tag.strip_prefix("channel-v")?;
    if revision.len() < 6 || !revision.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    let revision = revision.parse::<u64>().ok()?;
    (revision > 0 && revision_tag(revision) == tag).then_some(revision)
}

fn next_revision(published: u64, reserved: u64) -> Result<u64, SharedChannelError> {
    published.max(reserved).checked_add(1).ok_or_else(|| {
        SharedChannelError::new(
            SharedChannelErrorCode::Integrity,
            "The channel revision counter overflowed",
        )
    })
}

fn lock_sessions(
    sessions: &Mutex<PublishSessionStore>,
) -> std::sync::MutexGuard<'_, PublishSessionStore> {
    sessions
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn preview_expired(state: &PublishSessionState, now: Instant) -> bool {
    matches!(state, PublishSessionState::Preview { preview, .. } if stored_preview_expired(preview, now))
}

fn stored_preview_expired(preview: &StoredPublishPreview, now: Instant) -> bool {
    now.saturating_duration_since(preview.touched_at) >= PUBLISH_PREVIEW_TTL
}

fn repository_not_found(repository_id: u64) -> SharedChannelError {
    SharedChannelError::new(
        SharedChannelErrorCode::RepositoryNotFound,
        format!("No shared channel is registered for repository ID {repository_id}"),
    )
}

fn publish_session_not_found() -> SharedChannelError {
    SharedChannelError::new(
        SharedChannelErrorCode::RegistrationSessionNotFound,
        "The channel publish preview expired, was cancelled, or is already publishing",
    )
}

pub(super) fn cancelled_error() -> SharedChannelError {
    SharedChannelError::new(
        SharedChannelErrorCode::Cancelled,
        "The channel draft scan was cancelled",
    )
}

fn draft_changed_error() -> SharedChannelError {
    SharedChannelError::new(
        SharedChannelErrorCode::DraftChanged,
        "The repository default branch changed after the publish preview; scan it again",
    )
}

pub(super) fn integrity_error(message: &str) -> SharedChannelError {
    SharedChannelError::new(SharedChannelErrorCode::Integrity, message)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shared_channels::RepositoryPermissions;

    #[test]
    fn expired_previews_are_evicted_before_enforcing_the_session_limit() {
        let sessions = ChannelPublishSessions::default();
        let expired = Instant::now()
            .checked_sub(PUBLISH_PREVIEW_TTL + Duration::from_secs(1))
            .unwrap();
        let mut store = lock_sessions(&sessions.inner);
        for index in 0..MAX_PUBLISH_SESSIONS {
            store.sessions.insert(
                format!("expired-{index}"),
                PublishSessionState::Preview {
                    generation: index as u64 + 1,
                    preview: stored_preview(expired),
                },
            );
        }
        drop(store);

        assert!(sessions.reserve("fresh").is_ok());
        let store = lock_sessions(&sessions.inner);
        assert_eq!(store.sessions.len(), 1);
        assert!(store.sessions.contains_key("fresh"));
    }

    fn stored_preview(touched_at: Instant) -> StoredPublishPreview {
        StoredPublishPreview {
            public: ChannelPublishPreview {
                session_id: "123e4567-e89b-12d3-a456-426614174000".into(),
                repository_id: 42,
                commit_sha: "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".into(),
                next_revision: 1,
                tag_name: "channel-v000001".into(),
                changes: Vec::new(),
            },
            repository: RemoteRepository {
                id: 42,
                owner_id: 7,
                owner_login: "acme".into(),
                owner_type: "Organization".into(),
                name: "shared".into(),
                default_branch: "main".into(),
                html_url: "https://github.com/acme/shared".into(),
                clone_url: "https://github.com/acme/shared.git".into(),
                private: true,
                permissions: RepositoryPermissions {
                    admin: true,
                    maintain: true,
                    push: true,
                    pull: true,
                },
            },
            touched_at,
        }
    }
}
