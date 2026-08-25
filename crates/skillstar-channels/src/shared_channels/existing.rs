//! Preview-first registration of existing organization-private repositories.

use super::{
    CHANNEL_DESCRIPTOR_VERSION, GitHubOrganization, RemoteRepository, SHARED_CHANNEL_MUTATION_GATE,
    SharedChannelAuthorization, SharedChannelDescriptor, SharedChannelError,
    SharedChannelErrorCode, SharedChannelGateway, SharedChannelRegistry, SharedChannelRole,
    SharedChannelStatus, project_role, validate_remote_repository,
};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use skillstar_skills::git::transport::GitOperationPhase;
use skillstar_skills::git_skill::GitSkillFacade;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

const MAX_REGISTRATION_SESSIONS: usize = 64;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExistingChannelScanRequest {
    pub organization_id: u64,
    pub repository_id: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExistingChannelRepositoryCandidate {
    pub repository_id: u64,
    pub organization_id: u64,
    pub owner: String,
    pub name: String,
    pub html_url: String,
    pub clone_url: String,
    pub role: SharedChannelRole,
    pub already_registered: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExistingChannelSkillPreview {
    pub id: String,
    pub folder_path: String,
    pub description: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExistingRepositoryInventory {
    pub skills: Vec<ExistingChannelSkillPreview>,
    pub non_skill_files: Vec<String>,
    pub total_files: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExistingChannelExposure {
    pub full_repository_contents_readable: bool,
    pub full_history_readable: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExistingChannelScanPreview {
    pub session_id: String,
    pub repository: ExistingChannelRepositoryCandidate,
    pub skills: Vec<ExistingChannelSkillPreview>,
    pub non_skill_files: Vec<String>,
    pub total_files: u64,
    pub exposure: ExistingChannelExposure,
}

pub trait ExistingRepositoryScanner: Send + Sync + 'static {
    fn scan(
        &self,
        repository: &RemoteRepository,
    ) -> Result<ExistingRepositoryInventory, SharedChannelError>;
}

#[derive(Clone)]
pub struct GitExistingRepositoryScanner {
    facade: GitSkillFacade,
}

impl GitExistingRepositoryScanner {
    pub fn new(facade: GitSkillFacade) -> Self {
        Self { facade }
    }
}

impl ExistingRepositoryScanner for GitExistingRepositoryScanner {
    fn scan(
        &self,
        repository: &RemoteRepository,
    ) -> Result<ExistingRepositoryInventory, SharedChannelError> {
        let session = self.facade.session();
        if session.is_cancelled() {
            return Err(cancelled_error());
        }
        let _transaction_guard = skillstar_skills::skill_update::acquire_update_transaction_lock()
            .map_err(|_| {
                SharedChannelError::new(
                    SharedChannelErrorCode::Storage,
                    "Unable to lock the repository cache for channel registration",
                )
            })?;
        let (_, _, repository_dir, _) = self
            .facade
            .fetch_repo_scanned_detailed(&repository.clone_url, true)
            .map_err(|_| {
                if session.is_cancelled() {
                    cancelled_error()
                } else {
                    SharedChannelError::new(
                        SharedChannelErrorCode::Protocol,
                        "Unable to scan the selected repository with the secure Git session",
                    )
                }
            })?;
        session.emit(GitOperationPhase::Running, &repository.clone_url);
        let files = match collect_repository_files(&repository_dir, session) {
            Ok(files) => files,
            Err(error) => {
                if error.code == SharedChannelErrorCode::Cancelled {
                    session.emit(GitOperationPhase::Cancelled, &repository.clone_url);
                }
                return Err(error);
            }
        };
        if let Err(error) = materialize_all_skill_directories(&repository_dir, &files, session) {
            if error.code == SharedChannelErrorCode::Cancelled {
                session.emit(GitOperationPhase::Cancelled, &repository.clone_url);
            }
            return Err(error);
        }
        let discovered = skillstar_skills::discovery::discover_skills(&repository_dir, true);
        let skills = discovered
            .into_iter()
            .map(|skill| ExistingChannelSkillPreview {
                id: skill.id,
                folder_path: skill.folder_path,
                description: skill.description,
            })
            .collect::<Vec<_>>();
        let non_skill_files = files
            .iter()
            .filter(|path| {
                !skills
                    .iter()
                    .any(|skill| path_belongs_to_skill(path, &skill.folder_path))
            })
            .cloned()
            .collect();
        if session.is_cancelled() {
            session.emit(GitOperationPhase::Cancelled, &repository.clone_url);
            return Err(cancelled_error());
        }
        session.emit(GitOperationPhase::Completed, &repository.clone_url);
        Ok(ExistingRepositoryInventory {
            skills,
            non_skill_files,
            total_files: files.len() as u64,
        })
    }
}

#[derive(Clone, Default)]
pub struct ExistingChannelRegistrationSessions {
    inner: Arc<Mutex<RegistrationSessionStore>>,
}

#[derive(Default)]
struct RegistrationSessionStore {
    next_generation: u64,
    sessions: HashMap<String, RegistrationSessionState>,
}

enum RegistrationSessionState {
    Scanning {
        generation: u64,
    },
    Cancelled {
        generation: u64,
    },
    Preview {
        generation: u64,
        preview: ExistingChannelScanPreview,
    },
    Confirming {
        generation: u64,
        preview: ExistingChannelScanPreview,
    },
}

impl ExistingChannelRegistrationSessions {
    fn reserve_scan(&self, session_id: &str) -> Result<u64, SharedChannelError> {
        let mut store = lock_sessions(&self.inner);
        if store.sessions.contains_key(session_id)
            || store.sessions.len() >= MAX_REGISTRATION_SESSIONS
        {
            return Err(SharedChannelError::new(
                SharedChannelErrorCode::RegistrationSessionConflict,
                "The registration session is already active or the session limit was reached",
            ));
        }
        store.next_generation = store.next_generation.wrapping_add(1).max(1);
        let generation = store.next_generation;
        store.sessions.insert(
            session_id.to_string(),
            RegistrationSessionState::Scanning { generation },
        );
        Ok(generation)
    }

    fn scan_is_cancelled(&self, session_id: &str, generation: u64) -> bool {
        !matches!(
            lock_sessions(&self.inner).sessions.get(session_id),
            Some(RegistrationSessionState::Scanning { generation: current }) if *current == generation
        )
    }

    fn complete_scan(
        &self,
        session_id: &str,
        generation: u64,
        preview: ExistingChannelScanPreview,
    ) -> Result<ExistingChannelScanPreview, SharedChannelError> {
        let mut store = lock_sessions(&self.inner);
        match store.sessions.remove(session_id) {
            Some(RegistrationSessionState::Scanning {
                generation: current,
            }) if current == generation => {
                store.sessions.insert(
                    session_id.to_string(),
                    RegistrationSessionState::Preview {
                        generation,
                        preview: preview.clone(),
                    },
                );
                Ok(preview)
            }
            Some(RegistrationSessionState::Cancelled {
                generation: current,
            }) if current == generation => Err(cancelled_error()),
            Some(state) => {
                store.sessions.insert(session_id.to_string(), state);
                Err(cancelled_error())
            }
            None => Err(cancelled_error()),
        }
    }

    fn abandon_scan(&self, session_id: &str, generation: u64) {
        let mut store = lock_sessions(&self.inner);
        let should_remove = matches!(
            store.sessions.get(session_id),
            Some(RegistrationSessionState::Scanning { generation: current }
                | RegistrationSessionState::Cancelled { generation: current }) if *current == generation
        );
        if should_remove {
            store.sessions.remove(session_id);
        }
    }

    fn claim_preview(
        &self,
        session_id: &str,
    ) -> Result<(u64, ExistingChannelScanPreview), SharedChannelError> {
        let mut store = lock_sessions(&self.inner);
        let (generation, preview) = match store.sessions.remove(session_id) {
            Some(RegistrationSessionState::Preview {
                generation,
                preview,
            }) => (generation, preview),
            Some(state) => {
                store.sessions.insert(session_id.to_string(), state);
                return Err(registration_session_not_found());
            }
            None => return Err(registration_session_not_found()),
        };
        store.sessions.insert(
            session_id.to_string(),
            RegistrationSessionState::Confirming {
                generation,
                preview: preview.clone(),
            },
        );
        Ok((generation, preview))
    }

    fn restore_claim(&self, session_id: &str, generation: u64) {
        let mut store = lock_sessions(&self.inner);
        let (current, preview) = match store.sessions.remove(session_id) {
            Some(RegistrationSessionState::Confirming {
                generation,
                preview,
            }) => (generation, preview),
            Some(state) => {
                store.sessions.insert(session_id.to_string(), state);
                return;
            }
            None => return,
        };
        if current == generation {
            store.sessions.insert(
                session_id.to_string(),
                RegistrationSessionState::Preview {
                    generation,
                    preview,
                },
            );
        } else {
            store.sessions.insert(
                session_id.to_string(),
                RegistrationSessionState::Confirming {
                    generation: current,
                    preview,
                },
            );
        }
    }

    fn save_claim(
        &self,
        session_id: &str,
        generation: u64,
        save: impl FnOnce() -> Result<(), SharedChannelError>,
    ) -> Result<(), SharedChannelError> {
        let mut store = lock_sessions(&self.inner);
        let (current, preview) = match store.sessions.remove(session_id) {
            Some(RegistrationSessionState::Confirming {
                generation,
                preview,
            }) => (generation, preview),
            Some(state) => {
                store.sessions.insert(session_id.to_string(), state);
                return Err(registration_session_not_found());
            }
            None => return Err(registration_session_not_found()),
        };
        if current != generation {
            store.sessions.insert(
                session_id.to_string(),
                RegistrationSessionState::Confirming {
                    generation: current,
                    preview,
                },
            );
            return Err(registration_session_not_found());
        }
        if let Err(error) = save() {
            store.sessions.insert(
                session_id.to_string(),
                RegistrationSessionState::Preview {
                    generation,
                    preview,
                },
            );
            return Err(error);
        }
        Ok(())
    }

    pub fn cancel(&self, session_id: &str) -> bool {
        let mut store = lock_sessions(&self.inner);
        match store.sessions.remove(session_id) {
            Some(RegistrationSessionState::Scanning { generation }) => {
                store.sessions.insert(
                    session_id.to_string(),
                    RegistrationSessionState::Cancelled { generation },
                );
                true
            }
            Some(RegistrationSessionState::Cancelled { generation }) => {
                store.sessions.insert(
                    session_id.to_string(),
                    RegistrationSessionState::Cancelled { generation },
                );
                true
            }
            Some(RegistrationSessionState::Preview { .. }) => true,
            Some(state @ RegistrationSessionState::Confirming { .. }) => {
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

pub struct ExistingChannelRegistrationFacade<G, R> {
    gateway: G,
    registry: R,
    scanner: Option<Arc<dyn ExistingRepositoryScanner>>,
    sessions: ExistingChannelRegistrationSessions,
}

impl<G, R> ExistingChannelRegistrationFacade<G, R>
where
    G: SharedChannelGateway,
    R: SharedChannelRegistry,
{
    pub fn new<S>(
        gateway: G,
        registry: R,
        scanner: S,
        sessions: ExistingChannelRegistrationSessions,
    ) -> Self
    where
        S: ExistingRepositoryScanner,
    {
        Self {
            gateway,
            registry,
            scanner: Some(Arc::new(scanner)),
            sessions,
        }
    }

    pub fn without_scanner(
        gateway: G,
        registry: R,
        sessions: ExistingChannelRegistrationSessions,
    ) -> Self {
        Self {
            gateway,
            registry,
            scanner: None,
            sessions,
        }
    }

    pub async fn list_candidates(
        &self,
        organization_id: u64,
    ) -> Result<Vec<ExistingChannelRepositoryCandidate>, SharedChannelError> {
        let organization = self.require_owner_organization(organization_id).await?;
        let registered = self.registry.load()?;
        let mut candidates = self
            .gateway
            .list_selected_repositories(organization_id)
            .await?
            .into_iter()
            .filter_map(|repository| {
                validate_remote_repository(&repository, &organization, repository.id)
                    .ok()
                    .and_then(|()| {
                        (project_role(&repository.permissions) == Some(SharedChannelRole::Owner))
                            .then(|| {
                                candidate(
                                    &repository,
                                    registered
                                        .channels
                                        .iter()
                                        .any(|channel| channel.repository_id == repository.id),
                                )
                            })
                    })
            })
            .collect::<Vec<_>>();
        candidates.sort_by(|left, right| {
            left.owner
                .to_ascii_lowercase()
                .cmp(&right.owner.to_ascii_lowercase())
                .then_with(|| {
                    left.name
                        .to_ascii_lowercase()
                        .cmp(&right.name.to_ascii_lowercase())
                })
        });
        Ok(candidates)
    }

    pub async fn scan(
        &self,
        request: ExistingChannelScanRequest,
        session_id: String,
    ) -> Result<ExistingChannelScanPreview, SharedChannelError> {
        validate_session_id(&session_id)?;
        let generation = self.sessions.reserve_scan(&session_id)?;
        let result = self
            .scan_reserved(request, session_id.clone(), generation)
            .await;
        if result.is_err() {
            self.sessions.abandon_scan(&session_id, generation);
        }
        result
    }

    async fn scan_reserved(
        &self,
        request: ExistingChannelScanRequest,
        session_id: String,
        generation: u64,
    ) -> Result<ExistingChannelScanPreview, SharedChannelError> {
        let organization = self
            .require_owner_organization(request.organization_id)
            .await?;
        let repository = self
            .gateway
            .get_selected_repository(request.organization_id, request.repository_id)
            .await?;
        validate_registration_repository(&repository, &organization, request.repository_id)?;
        ensure_not_registered(&self.registry.load()?, request.repository_id)?;
        if self.sessions.scan_is_cancelled(&session_id, generation) {
            return Err(cancelled_error());
        }

        let scanner = self.scanner.clone().ok_or_else(|| {
            SharedChannelError::new(
                SharedChannelErrorCode::Protocol,
                "This registration facade cannot start repository scans",
            )
        })?;
        let repository_for_scan = repository.clone();
        let inventory = tokio::task::spawn_blocking(move || scanner.scan(&repository_for_scan))
            .await
            .map_err(|_| {
                SharedChannelError::new(
                    SharedChannelErrorCode::Protocol,
                    "The existing repository scan task stopped unexpectedly",
                )
            })??;
        validate_inventory(&inventory)?;
        let preview = ExistingChannelScanPreview {
            session_id,
            repository: candidate(&repository, false),
            skills: inventory.skills,
            non_skill_files: inventory.non_skill_files,
            total_files: inventory.total_files,
            exposure: ExistingChannelExposure {
                full_repository_contents_readable: true,
                full_history_readable: true,
            },
        };
        let preview_session_id = preview.session_id.clone();
        self.sessions
            .complete_scan(&preview_session_id, generation, preview)
    }

    pub async fn confirm(
        &self,
        session_id: &str,
    ) -> Result<SharedChannelDescriptor, SharedChannelError> {
        validate_session_id(session_id)?;
        let (generation, preview) = self.sessions.claim_preview(session_id)?;
        let result = self
            .confirm_claimed(session_id, generation, preview.clone())
            .await;
        if result.is_err() {
            self.sessions.restore_claim(session_id, generation);
        }
        result
    }

    async fn confirm_claimed(
        &self,
        session_id: &str,
        generation: u64,
        preview: ExistingChannelScanPreview,
    ) -> Result<SharedChannelDescriptor, SharedChannelError> {
        let _mutation_guard = SHARED_CHANNEL_MUTATION_GATE.lock().await;
        let _registry_lease = self.registry.acquire_mutation_lease().await?;
        let store = self.registry.load()?;
        ensure_not_registered(&store, preview.repository.repository_id)?;

        let organization = self
            .require_owner_organization(preview.repository.organization_id)
            .await?;
        let repository = self
            .gateway
            .get_selected_repository(
                preview.repository.organization_id,
                preview.repository.repository_id,
            )
            .await?;
        validate_registration_repository(
            &repository,
            &organization,
            preview.repository.repository_id,
        )?;

        let now = Utc::now().to_rfc3339();
        let descriptor = SharedChannelDescriptor {
            descriptor_version: CHANNEL_DESCRIPTOR_VERSION,
            repository_id: repository.id,
            organization_id: organization.id,
            owner: repository.owner_login,
            name: repository.name,
            html_url: repository.html_url,
            clone_url: repository.clone_url,
            role: SharedChannelRole::Owner,
            status: SharedChannelStatus::Active,
            authorization: SharedChannelAuthorization::default(),
            created_at: now.clone(),
            updated_at: now,
        };
        let mut updated = store;
        updated.channels.push(descriptor.clone());
        self.sessions
            .save_claim(session_id, generation, || self.registry.save(&updated))?;
        Ok(descriptor)
    }

    pub fn cancel(&self, session_id: &str) -> bool {
        self.sessions.cancel(session_id)
    }

    async fn require_owner_organization(
        &self,
        organization_id: u64,
    ) -> Result<GitHubOrganization, SharedChannelError> {
        let organization = self
            .gateway
            .list_organizations()
            .await?
            .into_iter()
            .find(|organization| organization.id == organization_id)
            .ok_or_else(|| {
                SharedChannelError::new(
                    SharedChannelErrorCode::OrganizationUnavailable,
                    "The selected organization is unavailable to this GitHub identity",
                )
            })?;
        if !organization.viewer_is_admin {
            return Err(SharedChannelError::new(
                SharedChannelErrorCode::PermissionDenied,
                "Organization owner access is required to register an existing shared channel",
            ));
        }
        Ok(organization)
    }
}

fn candidate(
    repository: &RemoteRepository,
    already_registered: bool,
) -> ExistingChannelRepositoryCandidate {
    ExistingChannelRepositoryCandidate {
        repository_id: repository.id,
        organization_id: repository.owner_id,
        owner: repository.owner_login.clone(),
        name: repository.name.clone(),
        html_url: repository.html_url.clone(),
        clone_url: repository.clone_url.clone(),
        role: project_role(&repository.permissions).unwrap_or(SharedChannelRole::Subscriber),
        already_registered,
    }
}

fn validate_registration_repository(
    repository: &RemoteRepository,
    organization: &GitHubOrganization,
    expected_repository_id: u64,
) -> Result<(), SharedChannelError> {
    validate_remote_repository(repository, organization, expected_repository_id)?;
    if project_role(&repository.permissions) != Some(SharedChannelRole::Owner) {
        return Err(SharedChannelError::new(
            SharedChannelErrorCode::PermissionDenied,
            "Repository Admin access is required to register an existing shared channel",
        ));
    }
    Ok(())
}

fn ensure_not_registered(
    store: &super::SharedChannelStore,
    repository_id: u64,
) -> Result<(), SharedChannelError> {
    if store
        .channels
        .iter()
        .any(|channel| channel.repository_id == repository_id)
    {
        Err(SharedChannelError::new(
            SharedChannelErrorCode::RepositoryAlreadyBound,
            "This GitHub repository is already bound to a shared channel",
        ))
    } else {
        Ok(())
    }
}

fn validate_session_id(session_id: &str) -> Result<(), SharedChannelError> {
    uuid::Uuid::parse_str(session_id).map(|_| ()).map_err(|_| {
        SharedChannelError::new(
            SharedChannelErrorCode::RegistrationSessionConflict,
            "Registration session id must be a UUID",
        )
    })
}

fn validate_inventory(inventory: &ExistingRepositoryInventory) -> Result<(), SharedChannelError> {
    if inventory.total_files < inventory.non_skill_files.len() as u64
        || inventory
            .non_skill_files
            .iter()
            .any(|path| !is_safe_relative_path(path))
        || inventory.skills.iter().any(|skill| {
            !skill.folder_path.is_empty() && !is_safe_relative_path(&skill.folder_path)
        })
    {
        return Err(SharedChannelError::new(
            SharedChannelErrorCode::Protocol,
            "The repository scanner returned an invalid inventory",
        ));
    }
    Ok(())
}

fn collect_repository_files(
    root: &Path,
    session: &skillstar_skills::git::transport::GitOperationSession,
) -> Result<Vec<String>, SharedChannelError> {
    if session.is_cancelled() {
        return Err(cancelled_error());
    }
    let mut files = skillstar_skills::git::ops::list_tree_paths(root).map_err(|_| {
        if session.is_cancelled() {
            cancelled_error()
        } else {
            inventory_io_error()
        }
    })?;
    if session.is_cancelled() {
        return Err(cancelled_error());
    }
    if files.iter().any(|path| !is_safe_relative_path(path)) {
        return Err(inventory_io_error());
    }
    files.sort();
    files.dedup();
    Ok(files)
}

fn materialize_all_skill_directories(
    repository_dir: &Path,
    files: &[String],
    session: &skillstar_skills::git::transport::GitOperationSession,
) -> Result<(), SharedChannelError> {
    if session.is_cancelled() {
        return Err(cancelled_error());
    }
    let mut directories = files
        .iter()
        .filter(|path| path.as_str() == "SKILL.md" || path.ends_with("/SKILL.md"))
        .filter_map(|path| Path::new(path).parent())
        .filter_map(|directory| directory.to_str())
        .filter(|directory| !directory.is_empty())
        .collect::<Vec<_>>();
    directories.sort_unstable();
    directories.dedup();
    if !directories.is_empty() {
        skillstar_skills::git::ops::apply_sparse_checkout_in_session(
            repository_dir,
            &directories,
            session,
        )
        .map_err(|_| {
            if session.is_cancelled() {
                cancelled_error()
            } else {
                inventory_io_error()
            }
        })?;
    }
    if session.is_cancelled() {
        return Err(cancelled_error());
    }
    Ok(())
}

fn is_safe_relative_path(value: &str) -> bool {
    let path = PathBuf::from(value);
    !value.is_empty()
        && !path.is_absolute()
        && path.components().all(|component| {
            matches!(
                component,
                std::path::Component::Normal(_) | std::path::Component::CurDir
            )
        })
}

fn path_belongs_to_skill(path: &str, folder_path: &str) -> bool {
    let folder = folder_path.trim_matches('/');
    folder.is_empty()
        || folder == "."
        || path == folder
        || path
            .strip_prefix(folder)
            .is_some_and(|suffix| suffix.starts_with('/'))
}

fn lock_sessions(
    sessions: &Mutex<RegistrationSessionStore>,
) -> std::sync::MutexGuard<'_, RegistrationSessionStore> {
    sessions
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn cancelled_error() -> SharedChannelError {
    SharedChannelError::new(
        SharedChannelErrorCode::Cancelled,
        "The existing repository scan was cancelled",
    )
}

fn inventory_io_error() -> SharedChannelError {
    SharedChannelError::new(
        SharedChannelErrorCode::Protocol,
        "Unable to inventory the checked-out repository safely",
    )
}

fn registration_session_not_found() -> SharedChannelError {
    SharedChannelError::new(
        SharedChannelErrorCode::RegistrationSessionNotFound,
        "The registration preview expired, was cancelled, or is already being confirmed; scan the repository again",
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command;

    #[test]
    fn sparse_checkout_inventory_uses_the_tracked_tree_and_materializes_every_skill() {
        let temp = tempfile::tempdir().unwrap();
        let source = temp.path().join("source");
        let checkout = temp.path().join("checkout");
        std::fs::create_dir_all(source.join("skills/nested")).unwrap();
        std::fs::create_dir_all(source.join(".github/workflows")).unwrap();
        std::fs::write(source.join("SKILL.md"), "---\nname: root\n---\n").unwrap();
        std::fs::write(
            source.join("skills/nested/SKILL.md"),
            "---\nname: nested\n---\n",
        )
        .unwrap();
        std::fs::write(source.join(".github/workflows/ci.yml"), "name: CI\n").unwrap();
        run_git(&source, &["init"]);
        run_git(&source, &["config", "user.email", "test@example.com"]);
        run_git(&source, &["config", "user.name", "SkillStar Test"]);
        run_git(&source, &["add", "."]);
        run_git(&source, &["commit", "-m", "fixture"]);

        let source_url = format!("file://{}", source.display());
        run_git(
            temp.path(),
            &[
                "clone",
                "--filter=blob:none",
                "--depth",
                "1",
                "--no-checkout",
                "--sparse",
                &source_url,
                checkout.to_str().unwrap(),
            ],
        );
        run_git(&checkout, &["checkout"]);
        std::fs::write(checkout.join("untracked-cache-noise.txt"), "local only").unwrap();

        let session = skillstar_skills::git::transport::GitOperationSession::public();
        let files = collect_repository_files(&checkout, &session).unwrap();
        assert!(files.iter().any(|path| path == ".github/workflows/ci.yml"));
        assert!(!files.iter().any(|path| path == "untracked-cache-noise.txt"));

        materialize_all_skill_directories(&checkout, &files, &session).unwrap();
        let skills = skillstar_skills::discovery::discover_skills(&checkout, true);
        assert!(skills.iter().any(|skill| skill.id == "root"));
        assert!(skills.iter().any(|skill| skill.id == "nested"));
    }

    fn run_git(directory: &Path, args: &[&str]) {
        let output = Command::new("git")
            .current_dir(directory)
            .args(args)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "git {:?} failed: {}",
            args,
            String::from_utf8_lossy(&output.stderr)
        );
    }
}
