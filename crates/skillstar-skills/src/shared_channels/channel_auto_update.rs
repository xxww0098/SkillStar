use super::{
    ApplyChannelUpdateRequest, ChannelReleaseTarget, ChannelSubscriptionFacade,
    ChannelSubscriptionGateway, ChannelSubscriptionInstaller, ChannelSubscriptionRegistry,
    ChannelSubscriptionUpdater, ChannelUpdateBlockReason, ChannelUpdateItem,
    ChannelUpdateItemState, SharedChannelError, SharedChannelErrorCode, SharedChannelRegistry,
};
use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::sync::{LazyLock, Mutex};

pub const CHANNEL_AUTO_UPDATE_INTERVAL_SECS: i64 = 60 * 60;
const CHANNEL_AUTO_UPDATE_RETRY_SECS: i64 = 5 * 60;
type ActiveAutoUpdateKey = (String, u64, String);
static ACTIVE_AUTO_UPDATE_CLAIMS: LazyLock<Mutex<BTreeSet<ActiveAutoUpdateKey>>> =
    LazyLock::new(|| Mutex::new(BTreeSet::new()));

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct ChannelAutoUpdateState {
    pub enabled: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next_check_at: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_run: Option<ChannelAutoUpdateRun>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChannelAutoUpdateRunStatus {
    Checking,
    Checked,
    UpToDate,
    Applied,
    PartiallyApplied,
    Paused,
    RetryableFailure,
    Cancelled,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChannelAutoUpdatePauseReason {
    Pinned,
    LocalContentChanged,
    BaselineMissing,
    SnapshotFailed,
    PermissionChanged,
    RemovedUpstream,
    IntegrityError,
    UnresolvedFailure,
    NewSkillRequiresReview,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ChannelAutoUpdatePause {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub skill_id: Option<String>,
    pub reason: ChannelAutoUpdatePauseReason,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ChannelAutoUpdateRun {
    pub started_at: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub completed_at: Option<String>,
    pub status: ChannelAutoUpdateRunStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target: Option<ChannelReleaseTarget>,
    #[serde(default)]
    pub applied_skill_ids: Vec<String>,
    #[serde(default)]
    pub pauses: Vec<ChannelAutoUpdatePause>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(default)]
    pub retryable: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ChannelAutoUpdateExecution {
    pub repository_id: u64,
    pub run: ChannelAutoUpdateRun,
}

pub(super) struct ActiveAutoUpdateClaim {
    key: ActiveAutoUpdateKey,
    _cross_process_lease: Box<dyn super::SharedChannelMutationLease>,
}

impl ActiveAutoUpdateClaim {
    fn repository_id(&self) -> u64 {
        self.key.1
    }

    fn started_at(&self) -> &str {
        &self.key.2
    }
}

impl Drop for ActiveAutoUpdateClaim {
    fn drop(&mut self) {
        ACTIVE_AUTO_UPDATE_CLAIMS
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .remove(&self.key);
    }
}

impl ChannelAutoUpdateRun {
    pub(super) fn checking(now: DateTime<Utc>) -> Self {
        Self {
            started_at: now.to_rfc3339(),
            completed_at: None,
            status: ChannelAutoUpdateRunStatus::Checking,
            target: None,
            applied_skill_ids: Vec::new(),
            pauses: Vec::new(),
            error: None,
            retryable: false,
        }
    }

    fn completed(
        started_at: &str,
        now: DateTime<Utc>,
        status: ChannelAutoUpdateRunStatus,
        target: Option<ChannelReleaseTarget>,
        applied_skill_ids: Vec<String>,
        pauses: Vec<ChannelAutoUpdatePause>,
        error: Option<String>,
    ) -> Self {
        Self {
            started_at: started_at.to_string(),
            completed_at: Some(now.to_rfc3339()),
            status,
            target,
            applied_skill_ids,
            pauses,
            error,
            retryable: false,
        }
    }
}

impl<G, C, S, I> ChannelSubscriptionFacade<G, C, S, I>
where
    G: ChannelSubscriptionGateway,
    C: SharedChannelRegistry,
    S: ChannelSubscriptionRegistry,
    I: ChannelSubscriptionInstaller + ChannelSubscriptionUpdater,
{
    pub fn auto_update_state(
        &self,
        repository_id: u64,
    ) -> Result<ChannelAutoUpdateState, SharedChannelError> {
        channel_auto_update_state(&self.subscriptions, repository_id)
    }

    pub async fn set_auto_update_enabled(
        &self,
        repository_id: u64,
        enabled: bool,
    ) -> Result<ChannelAutoUpdateState, SharedChannelError> {
        set_channel_auto_update_enabled(&self.subscriptions, repository_id, enabled).await
    }

    pub async fn run_due_auto_updates(
        &self,
    ) -> Result<Vec<ChannelAutoUpdateExecution>, SharedChannelError> {
        self.run_due_auto_updates_with(Utc::now).await
    }

    pub async fn run_due_auto_updates_at(
        &self,
        now: DateTime<Utc>,
    ) -> Result<Vec<ChannelAutoUpdateExecution>, SharedChannelError> {
        self.run_due_auto_updates_with(move || now).await
    }

    pub(super) async fn run_due_auto_updates_with<F>(
        &self,
        clock: F,
    ) -> Result<Vec<ChannelAutoUpdateExecution>, SharedChannelError>
    where
        F: Fn() -> DateTime<Utc> + Send + Sync,
    {
        let claimed = self.claim_due_auto_updates(clock()).await?;
        let mut executions = Vec::with_capacity(claimed.len());
        for claim in claimed {
            let repository_id = claim.repository_id();
            let started_at = claim.started_at().to_string();
            let run = self
                .execute_auto_update(repository_id, &started_at, &clock)
                .await;
            self.persist_auto_update_run(repository_id, &run, clock())
                .await?;
            executions.push(ChannelAutoUpdateExecution { repository_id, run });
        }
        Ok(executions)
    }

    pub(super) async fn claim_due_auto_updates(
        &self,
        now: DateTime<Utc>,
    ) -> Result<Vec<ActiveAutoUpdateClaim>, SharedChannelError> {
        let _mutation_guard = super::SHARED_CHANNEL_MUTATION_GATE.lock().await;
        let _subscription_lease = self.subscriptions.acquire_mutation_lease().await?;
        let mut store = self.subscriptions.load_mutable()?;
        let scope = self.subscriptions.auto_update_scope_key();
        let mut claimed_runs = Vec::new();
        for subscription in &mut store.subscriptions {
            if subscription
                .auto_update
                .last_run
                .as_ref()
                .is_some_and(|run| {
                    run.status == ChannelAutoUpdateRunStatus::Checking
                        && active_claim_exists(&scope, subscription.repository_id, &run.started_at)
                })
            {
                continue;
            }
            if !auto_update_due(&subscription.auto_update, now) {
                continue;
            }
            let Some(run_lease) = self
                .subscriptions
                .try_acquire_auto_update_run_lease(subscription.repository_id)?
            else {
                continue;
            };
            let mut run = ChannelAutoUpdateRun::checking(now);
            if let Some(previous) = &subscription.auto_update.last_run {
                run.pauses = previous.pauses.clone();
            }
            subscription.auto_update.next_check_at =
                Some((now + Duration::seconds(CHANNEL_AUTO_UPDATE_INTERVAL_SECS)).to_rfc3339());
            subscription.auto_update.last_run = Some(run.clone());
            subscription.updated_at = now.to_rfc3339();
            claimed_runs.push((subscription.repository_id, run.started_at, run_lease));
        }
        if !claimed_runs.is_empty() {
            self.subscriptions.save(&store)?;
        }
        Ok(claimed_runs
            .into_iter()
            .map(|(repository_id, started_at, run_lease)| {
                register_active_claim(scope.clone(), repository_id, started_at, run_lease)
            })
            .collect())
    }

    async fn execute_auto_update<F>(
        &self,
        repository_id: u64,
        started_at: &str,
        clock: &F,
    ) -> ChannelAutoUpdateRun
    where
        F: Fn() -> DateTime<Utc> + Send + Sync,
    {
        let previous = match self.subscription_for_auto_update(repository_id) {
            Ok(subscription) => subscription,
            Err(error) => return failure_run(started_at, clock(), None, error),
        };
        let mut blockers = previous
            .last_update
            .as_ref()
            .into_iter()
            .flat_map(|snapshot| &snapshot.items)
            .filter(|item| item.state == ChannelUpdateItemState::Failed)
            .filter_map(|item| {
                pause_reason(item).map(|reason| (item.id.to_ascii_lowercase(), reason))
            })
            .collect::<BTreeMap<_, _>>();
        let manually_resolved = previous
            .last_update
            .as_ref()
            .into_iter()
            .flat_map(|snapshot| &snapshot.items)
            .filter(|item| {
                matches!(
                    item.state,
                    ChannelUpdateItemState::Applied | ChannelUpdateItemState::Current
                )
            })
            .map(|item| item.id.to_ascii_lowercase())
            .collect::<BTreeSet<_>>();
        let persisted_blockers = previous
            .auto_update
            .last_run
            .as_ref()
            .into_iter()
            .flat_map(|run| &run.pauses)
            .filter_map(|pause| {
                let skill_id = pause.skill_id.as_deref()?.to_ascii_lowercase();
                matches!(
                    pause.reason,
                    ChannelAutoUpdatePauseReason::PermissionChanged
                        | ChannelAutoUpdatePauseReason::IntegrityError
                        | ChannelAutoUpdatePauseReason::UnresolvedFailure
                )
                .then_some((skill_id, pause.reason))
            })
            .filter(|(skill_id, _)| !manually_resolved.contains(skill_id))
            .collect::<BTreeMap<_, _>>();
        blockers.extend(persisted_blockers);

        let snapshot = match self.check_update(repository_id).await {
            Ok(snapshot) => snapshot,
            Err(error) => return failure_run(started_at, clock(), None, error),
        };
        let current = match self.subscription_for_auto_update(repository_id) {
            Ok(subscription) => subscription,
            _ => {
                return ChannelAutoUpdateRun::completed(
                    started_at,
                    clock(),
                    ChannelAutoUpdateRunStatus::Cancelled,
                    Some(snapshot.target),
                    Vec::new(),
                    Vec::new(),
                    None,
                );
            }
        };
        if let Some(error) = snapshot.check_error.clone() {
            let code = snapshot
                .check_error_code
                .unwrap_or(SharedChannelErrorCode::Protocol);
            return failure_run(
                started_at,
                clock(),
                Some(snapshot.target.clone()),
                SharedChannelError::new(code, error),
            );
        }
        let pinned = current
            .pins
            .iter()
            .map(|pin| pin.skill_id.to_ascii_lowercase())
            .collect::<BTreeSet<_>>();
        let (safe, pauses) = automatic_plan(&snapshot, &pinned, &blockers);
        if !current.auto_update.enabled {
            return ChannelAutoUpdateRun::completed(
                started_at,
                clock(),
                ChannelAutoUpdateRunStatus::Checked,
                Some(snapshot.target),
                Vec::new(),
                pauses,
                None,
            );
        }
        let has_new_skill = pauses
            .iter()
            .any(|pause| pause.reason == ChannelAutoUpdatePauseReason::NewSkillRequiresReview);
        let acknowledge_release = !has_new_skill;
        let should_apply = !safe.is_empty()
            || (snapshot.acknowledgement_required
                && pauses.is_empty()
                && snapshot
                    .items
                    .iter()
                    .all(|item| item.state == ChannelUpdateItemState::Current));

        if !should_apply {
            let status = if pauses.is_empty() {
                ChannelAutoUpdateRunStatus::UpToDate
            } else {
                ChannelAutoUpdateRunStatus::Paused
            };
            return ChannelAutoUpdateRun::completed(
                started_at,
                clock(),
                status,
                Some(snapshot.target),
                Vec::new(),
                pauses,
                None,
            );
        }

        let result = self
            .apply_update_selected(
                ApplyChannelUpdateRequest {
                    repository_id,
                    target: snapshot.target.clone(),
                    resolutions: Vec::new(),
                },
                Some(&safe),
                acknowledge_release,
                Some(started_at),
            )
            .await;
        match result {
            Ok(result) => {
                let current_pins = self
                    .subscription_for_auto_update(repository_id)
                    .map(|subscription| {
                        subscription
                            .pins
                            .into_iter()
                            .map(|pin| pin.skill_id.to_ascii_lowercase())
                            .collect::<BTreeSet<_>>()
                    })
                    .unwrap_or(pinned);
                let (_, final_pauses) = automatic_plan(&result.snapshot, &current_pins, &blockers);
                let retryable_errors = result
                    .snapshot
                    .items
                    .iter()
                    .filter(|item| {
                        item.state == ChannelUpdateItemState::Failed
                            && is_retryable_error_code(item.error_code)
                    })
                    .filter_map(|item| item.error.clone())
                    .collect::<Vec<_>>();
                let retryable = !retryable_errors.is_empty();
                let status = if retryable && result.applied_skill_ids.is_empty() {
                    ChannelAutoUpdateRunStatus::RetryableFailure
                } else if retryable {
                    ChannelAutoUpdateRunStatus::PartiallyApplied
                } else if final_pauses.is_empty() {
                    if result.applied_skill_ids.is_empty() {
                        ChannelAutoUpdateRunStatus::UpToDate
                    } else {
                        ChannelAutoUpdateRunStatus::Applied
                    }
                } else if result.applied_skill_ids.is_empty() {
                    ChannelAutoUpdateRunStatus::Paused
                } else {
                    ChannelAutoUpdateRunStatus::PartiallyApplied
                };
                let mut run = ChannelAutoUpdateRun::completed(
                    started_at,
                    clock(),
                    status,
                    Some(result.snapshot.target),
                    result.applied_skill_ids,
                    final_pauses,
                    retryable.then(|| retryable_errors.join("; ")),
                );
                run.retryable = retryable;
                run
            }
            Err(error) if error.code == SharedChannelErrorCode::ReleaseConflict => {
                ChannelAutoUpdateRun::completed(
                    started_at,
                    clock(),
                    ChannelAutoUpdateRunStatus::Cancelled,
                    Some(snapshot.target),
                    Vec::new(),
                    Vec::new(),
                    Some(error.message),
                )
            }
            Err(error) => failure_run(started_at, clock(), Some(snapshot.target), error),
        }
    }

    fn subscription_for_auto_update(
        &self,
        repository_id: u64,
    ) -> Result<super::ChannelSubscription, SharedChannelError> {
        self.subscriptions
            .load_mutable()?
            .subscriptions
            .into_iter()
            .find(|subscription| subscription.repository_id == repository_id)
            .ok_or_else(|| subscription_not_found(repository_id))
    }

    async fn persist_auto_update_run(
        &self,
        repository_id: u64,
        run: &ChannelAutoUpdateRun,
        now: DateTime<Utc>,
    ) -> Result<(), SharedChannelError> {
        let _mutation_guard = super::SHARED_CHANNEL_MUTATION_GATE.lock().await;
        let _subscription_lease = self.subscriptions.acquire_mutation_lease().await?;
        let mut store = self.subscriptions.load_mutable()?;
        let subscription = store
            .subscriptions
            .iter_mut()
            .find(|subscription| subscription.repository_id == repository_id)
            .ok_or_else(|| subscription_not_found(repository_id))?;
        let current_started_at = subscription
            .auto_update
            .last_run
            .as_ref()
            .map(|current| current.started_at.as_str());
        let run_started_at = DateTime::parse_from_rfc3339(&run.started_at).map_err(|_| {
            SharedChannelError::new(
                SharedChannelErrorCode::Storage,
                "Invalid automatic update run timestamp",
            )
        })?;
        if current_started_at
            .and_then(|current| DateTime::parse_from_rfc3339(current).ok())
            .is_some_and(|current| current > run_started_at)
        {
            return Ok(());
        }
        subscription.auto_update.last_run = Some(run.clone());
        subscription.auto_update.next_check_at = if run_is_hard_paused(run) {
            None
        } else {
            let delay = if run.retryable {
                CHANNEL_AUTO_UPDATE_RETRY_SECS
            } else {
                CHANNEL_AUTO_UPDATE_INTERVAL_SECS
            };
            Some((now + Duration::seconds(delay)).to_rfc3339())
        };
        subscription.updated_at = now.to_rfc3339();
        self.subscriptions.save(&store)
    }
}

pub fn channel_auto_update_state<S: ChannelSubscriptionRegistry>(
    subscriptions: &S,
    repository_id: u64,
) -> Result<ChannelAutoUpdateState, SharedChannelError> {
    subscriptions
        .load_mutable()?
        .subscriptions
        .into_iter()
        .find(|subscription| subscription.repository_id == repository_id)
        .map(|subscription| subscription.auto_update)
        .ok_or_else(|| subscription_not_found(repository_id))
}

pub async fn set_channel_auto_update_enabled<S: ChannelSubscriptionRegistry>(
    subscriptions: &S,
    repository_id: u64,
    enabled: bool,
) -> Result<ChannelAutoUpdateState, SharedChannelError> {
    set_channel_auto_update_enabled_at(subscriptions, repository_id, enabled, Utc::now()).await
}

async fn set_channel_auto_update_enabled_at<S: ChannelSubscriptionRegistry>(
    subscriptions: &S,
    repository_id: u64,
    enabled: bool,
    now: DateTime<Utc>,
) -> Result<ChannelAutoUpdateState, SharedChannelError> {
    let _mutation_guard = super::SHARED_CHANNEL_MUTATION_GATE.lock().await;
    let _subscription_lease = subscriptions.acquire_mutation_lease().await?;
    let mut store = subscriptions.load_mutable()?;
    let subscription = store
        .subscriptions
        .iter_mut()
        .find(|subscription| subscription.repository_id == repository_id)
        .ok_or_else(|| subscription_not_found(repository_id))?;
    subscription.auto_update.enabled = enabled;
    let checking = subscription
        .auto_update
        .last_run
        .as_ref()
        .is_some_and(|run| run.status == ChannelAutoUpdateRunStatus::Checking);
    if !checking {
        subscription.auto_update.next_check_at = if enabled {
            Some(now.to_rfc3339())
        } else if subscription
            .auto_update
            .last_run
            .as_ref()
            .is_some_and(run_is_hard_paused)
        {
            None
        } else {
            Some((now + Duration::seconds(CHANNEL_AUTO_UPDATE_INTERVAL_SECS)).to_rfc3339())
        };
    }
    subscription.updated_at = now.to_rfc3339();
    let state = subscription.auto_update.clone();
    subscriptions.save(&store)?;
    Ok(state)
}

fn auto_update_due(state: &ChannelAutoUpdateState, now: DateTime<Utc>) -> bool {
    if state.last_run.as_ref().is_some_and(|run| {
        run.status == ChannelAutoUpdateRunStatus::Checking
            && DateTime::parse_from_rfc3339(&run.started_at)
                .map(|started| started + Duration::seconds(CHANNEL_AUTO_UPDATE_RETRY_SECS) <= now)
                .unwrap_or(true)
    }) {
        return true;
    }
    if state.next_check_at.is_none() && state.last_run.as_ref().is_some_and(run_is_hard_paused) {
        return false;
    }
    state.next_check_at.as_ref().is_none_or(|value| {
        DateTime::parse_from_rfc3339(value)
            .map(|due| due <= now)
            .unwrap_or(true)
    })
}

fn active_claim_exists(scope: &str, repository_id: u64, started_at: &str) -> bool {
    ACTIVE_AUTO_UPDATE_CLAIMS
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .contains(&(scope.to_string(), repository_id, started_at.to_string()))
}

fn register_active_claim(
    scope: String,
    repository_id: u64,
    started_at: String,
    cross_process_lease: Box<dyn super::SharedChannelMutationLease>,
) -> ActiveAutoUpdateClaim {
    let key = (scope, repository_id, started_at);
    ACTIVE_AUTO_UPDATE_CLAIMS
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .insert(key.clone());
    ActiveAutoUpdateClaim {
        key,
        _cross_process_lease: cross_process_lease,
    }
}

fn pause_reason(item: &ChannelUpdateItem) -> Option<ChannelAutoUpdatePauseReason> {
    if item.state == ChannelUpdateItemState::Notification {
        return Some(ChannelAutoUpdatePauseReason::NewSkillRequiresReview);
    }
    if item.state == ChannelUpdateItemState::Failed {
        return match item.error_code {
            Some(
                SharedChannelErrorCode::Network
                | SharedChannelErrorCode::InvitationRateLimited
                | SharedChannelErrorCode::NotAuthenticated
                | SharedChannelErrorCode::Protocol
                | SharedChannelErrorCode::Cancelled,
            ) => None,
            Some(
                SharedChannelErrorCode::PermissionDenied
                | SharedChannelErrorCode::AppNotInstalled
                | SharedChannelErrorCode::AppRepositorySelectionRequired
                | SharedChannelErrorCode::AppRepositoryAccessRequired
                | SharedChannelErrorCode::RepositoryNotFound,
            ) => Some(ChannelAutoUpdatePauseReason::PermissionChanged),
            Some(SharedChannelErrorCode::Integrity | SharedChannelErrorCode::ReleaseConflict) => {
                Some(ChannelAutoUpdatePauseReason::IntegrityError)
            }
            _ => Some(ChannelAutoUpdatePauseReason::UnresolvedFailure),
        };
    }
    match item.block_reason {
        Some(ChannelUpdateBlockReason::LocalContentChanged) => {
            Some(ChannelAutoUpdatePauseReason::LocalContentChanged)
        }
        Some(ChannelUpdateBlockReason::BaselineMissing) => {
            Some(ChannelAutoUpdatePauseReason::BaselineMissing)
        }
        Some(ChannelUpdateBlockReason::SnapshotFailed) => {
            Some(ChannelAutoUpdatePauseReason::SnapshotFailed)
        }
        Some(ChannelUpdateBlockReason::RemovedUpstream) => {
            Some(ChannelAutoUpdatePauseReason::RemovedUpstream)
        }
        None => None,
    }
}

fn automatic_plan(
    snapshot: &super::ChannelUpdateSnapshot,
    pinned: &BTreeSet<String>,
    blockers: &BTreeMap<String, ChannelAutoUpdatePauseReason>,
) -> (BTreeSet<String>, Vec<ChannelAutoUpdatePause>) {
    let mut safe = BTreeSet::new();
    let mut pauses = Vec::new();
    for item in &snapshot.items {
        let key = item.id.to_ascii_lowercase();
        if item.state == ChannelUpdateItemState::Available {
            if pinned.contains(&key) {
                pauses.push(item_pause(item, ChannelAutoUpdatePauseReason::Pinned));
            } else if let Some(reason) = blockers.get(&key) {
                pauses.push(item_pause(item, *reason));
            } else {
                safe.insert(key);
            }
        } else if let Some(reason) = pause_reason(item) {
            pauses.push(item_pause(item, reason));
        }
    }
    (safe, pauses)
}

fn run_is_hard_paused(run: &ChannelAutoUpdateRun) -> bool {
    run.status == ChannelAutoUpdateRunStatus::Paused
        && run.pauses.iter().any(|pause| {
            pause.skill_id.is_none()
                && matches!(
                    pause.reason,
                    ChannelAutoUpdatePauseReason::IntegrityError
                        | ChannelAutoUpdatePauseReason::UnresolvedFailure
                )
        })
}

fn item_pause(
    item: &ChannelUpdateItem,
    reason: ChannelAutoUpdatePauseReason,
) -> ChannelAutoUpdatePause {
    ChannelAutoUpdatePause {
        skill_id: Some(item.id.clone()),
        reason,
        detail: item.error.clone(),
    }
}

fn failure_run(
    started_at: &str,
    now: DateTime<Utc>,
    target: Option<ChannelReleaseTarget>,
    error: SharedChannelError,
) -> ChannelAutoUpdateRun {
    let (status, pauses) = match error.code {
        SharedChannelErrorCode::Cancelled => (ChannelAutoUpdateRunStatus::Cancelled, Vec::new()),
        SharedChannelErrorCode::Network
        | SharedChannelErrorCode::InvitationRateLimited
        | SharedChannelErrorCode::NotAuthenticated
        | SharedChannelErrorCode::Protocol => {
            (ChannelAutoUpdateRunStatus::RetryableFailure, Vec::new())
        }
        SharedChannelErrorCode::PermissionDenied
        | SharedChannelErrorCode::SubscriptionAccessRevoked
        | SharedChannelErrorCode::AppNotInstalled
        | SharedChannelErrorCode::AppRepositorySelectionRequired
        | SharedChannelErrorCode::AppRepositoryAccessRequired
        | SharedChannelErrorCode::RepositoryNotFound => (
            ChannelAutoUpdateRunStatus::Paused,
            vec![ChannelAutoUpdatePause {
                skill_id: None,
                reason: ChannelAutoUpdatePauseReason::PermissionChanged,
                detail: Some(error.message.clone()),
            }],
        ),
        SharedChannelErrorCode::Integrity | SharedChannelErrorCode::ReleaseConflict => (
            ChannelAutoUpdateRunStatus::Paused,
            vec![ChannelAutoUpdatePause {
                skill_id: None,
                reason: ChannelAutoUpdatePauseReason::IntegrityError,
                detail: Some(error.message.clone()),
            }],
        ),
        _ => (
            ChannelAutoUpdateRunStatus::Paused,
            vec![ChannelAutoUpdatePause {
                skill_id: None,
                reason: ChannelAutoUpdatePauseReason::UnresolvedFailure,
                detail: Some(error.message.clone()),
            }],
        ),
    };
    let retryable = status == ChannelAutoUpdateRunStatus::RetryableFailure;
    let mut run = ChannelAutoUpdateRun::completed(
        started_at,
        now,
        status,
        target,
        Vec::new(),
        pauses,
        Some(error.message),
    );
    run.retryable = retryable;
    run
}

fn is_retryable_error_code(code: Option<SharedChannelErrorCode>) -> bool {
    matches!(
        code,
        Some(
            SharedChannelErrorCode::Network
                | SharedChannelErrorCode::InvitationRateLimited
                | SharedChannelErrorCode::NotAuthenticated
                | SharedChannelErrorCode::Protocol
                | SharedChannelErrorCode::Cancelled
        )
    )
}

fn subscription_not_found(repository_id: u64) -> SharedChannelError {
    SharedChannelError::new(
        SharedChannelErrorCode::SubscriptionNotFound,
        format!("No shared channel subscription exists for repository ID {repository_id}"),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn due_policy_uses_injected_time() {
        let now = DateTime::parse_from_rfc3339("2026-08-05T10:00:00Z")
            .unwrap()
            .with_timezone(&Utc);
        assert!(auto_update_due(&ChannelAutoUpdateState::default(), now));
        assert!(auto_update_due(
            &ChannelAutoUpdateState {
                enabled: true,
                next_check_at: Some("2026-08-05T10:00:00Z".into()),
                last_run: None,
            },
            now,
        ));
        assert!(!auto_update_due(
            &ChannelAutoUpdateState {
                enabled: true,
                next_check_at: Some("2026-08-05T10:00:01Z".into()),
                last_run: None,
            },
            now,
        ));
        let interrupted = ChannelAutoUpdateState {
            enabled: true,
            next_check_at: Some("2026-08-05T11:00:00Z".into()),
            last_run: Some(ChannelAutoUpdateRun::checking(now)),
        };
        assert!(!auto_update_due(&interrupted, now + Duration::minutes(4)));
        assert!(auto_update_due(&interrupted, now + Duration::minutes(5)));
    }

    #[test]
    fn remote_failures_have_distinct_pause_semantics() {
        let now = Utc::now();
        let permission = failure_run(
            "2026-08-05T00:00:00Z",
            now,
            None,
            SharedChannelError::new(SharedChannelErrorCode::PermissionDenied, "permission"),
        );
        assert_eq!(permission.status, ChannelAutoUpdateRunStatus::Paused);
        assert_eq!(
            permission.pauses[0].reason,
            ChannelAutoUpdatePauseReason::PermissionChanged
        );

        let integrity = failure_run(
            "2026-08-05T00:00:00Z",
            now,
            None,
            SharedChannelError::new(SharedChannelErrorCode::Integrity, "tampered"),
        );
        assert_eq!(
            integrity.pauses[0].reason,
            ChannelAutoUpdatePauseReason::IntegrityError
        );

        let network = failure_run(
            "2026-08-05T00:00:00Z",
            now,
            None,
            SharedChannelError::new(SharedChannelErrorCode::Network, "offline"),
        );
        assert_eq!(network.status, ChannelAutoUpdateRunStatus::RetryableFailure);
        assert!(network.pauses.is_empty());

        let rate_limited = failure_run(
            "2026-08-05T00:00:00Z",
            now,
            None,
            SharedChannelError::new(
                SharedChannelErrorCode::InvitationRateLimited,
                "rate limited",
            ),
        );
        assert_eq!(
            rate_limited.status,
            ChannelAutoUpdateRunStatus::RetryableFailure
        );
        assert!(rate_limited.retryable);
    }
}
