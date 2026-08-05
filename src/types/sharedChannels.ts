import type { LocalDivergenceResolution } from "./skill";

export type SharedChannelRole = "owner" | "publisher" | "subscriber";
export type SharedChannelStatus = "awaiting_app_installation" | "awaiting_invitation_acceptance" | "active";

export interface GitHubOrganization {
  id: number;
  login: string;
  avatar_url: string | null;
  viewer_is_admin: boolean;
}

export interface SharedChannelAuthorization {
  repository_selection: "selected";
  administration: "write";
  contents: "write";
}

export interface SharedChannelDescriptor {
  descriptor_version: number;
  repository_id: number;
  organization_id: number;
  owner: string;
  name: string;
  html_url: string;
  clone_url: string;
  role: SharedChannelRole;
  status: SharedChannelStatus;
  authorization: SharedChannelAuthorization;
  created_at: string;
  updated_at: string;
}

export interface CreateSharedChannelRequest {
  organization: string;
  repository_name: string;
  description: string;
}

export interface ExistingChannelRepositoryCandidate {
  repository_id: number;
  organization_id: number;
  owner: string;
  name: string;
  html_url: string;
  clone_url: string;
  role: SharedChannelRole;
  already_registered: boolean;
}

export interface ExistingChannelSkillPreview {
  id: string;
  folder_path: string;
  description: string;
}

export interface ExistingChannelExposure {
  full_repository_contents_readable: boolean;
  full_history_readable: boolean;
}

export interface ExistingChannelScanPreview {
  session_id: string;
  repository: ExistingChannelRepositoryCandidate;
  skills: ExistingChannelSkillPreview[];
  non_skill_files: string[];
  total_files: number;
  exposure: ExistingChannelExposure;
}

export interface ExistingChannelScanRequest {
  organization_id: number;
  repository_id: number;
}

export type ChannelSkillReleaseStatus = "added" | "updated" | "unchanged" | "removed";

export interface ChannelReleaseSkill {
  id: string;
  content_root: string;
  content_hash: string;
  content_hash_version: number;
  status: ChannelSkillReleaseStatus;
}

export interface ChannelPublisherIdentity {
  id: number;
  login: string;
}

export interface ChannelReleaseManifest {
  schema_version: number;
  repository_id: number;
  organization_id: number;
  revision: number;
  tag_name: string;
  commit_sha: string;
  publisher: ChannelPublisherIdentity;
  published_at: string;
  title: string;
  notes: string;
  skills: ChannelReleaseSkill[];
}

export interface ChannelPublishPreview {
  session_id: string;
  repository_id: number;
  commit_sha: string;
  next_revision: number;
  tag_name: string;
  changes: ChannelReleaseSkill[];
}

export interface RemoteChannelRelease {
  id: number;
  html_url: string;
}

export interface ChannelPublishResult {
  manifest: ChannelReleaseManifest;
  release: RemoteChannelRelease;
}

export type ChannelInviteRole = "subscriber" | "publisher";
export type ChannelMembershipStatus = "pending" | "accepted" | "failed" | "cancelled";

export interface ChannelMemberIdentity {
  id: number;
  login: string;
}

export interface ChannelMember {
  user: ChannelMemberIdentity;
  role: SharedChannelRole;
  github_role_name: string;
  status: ChannelMembershipStatus;
}

export interface ChannelInvitation {
  id: number;
  repository_id: number;
  organization_id: number;
  owner: string;
  repository_name: string;
  html_url: string;
  invitee: ChannelMemberIdentity | null;
  inviter: ChannelMemberIdentity | null;
  role: ChannelInviteRole;
  effective_role: SharedChannelRole;
  status: ChannelMembershipStatus;
  created_at: string;
}

export interface ChannelMembershipSnapshot {
  repository_id: number;
  members: ChannelMember[];
  invitations: ChannelInvitation[];
}

export type RepositoryAccessSource = "direct" | "inherited" | "unknown";
export type ChannelMemberRevocationStatus = "revoked" | "access_remains";

export interface ChannelMemberRevocationResult {
  repository_id: number;
  username: string;
  status: ChannelMemberRevocationStatus;
  effective_role: SharedChannelRole | null;
  access_source: RepositoryAccessSource | null;
}

export interface CreateChannelInvitationRequest {
  repository_id: number;
  username: string;
  role: ChannelInviteRole;
}

export interface ChannelInvitationAction {
  repository_id: number;
  invitation_id: number | null;
  username: string;
  role: ChannelInviteRole;
  status: ChannelMembershipStatus;
}

export interface ChannelReleaseTarget {
  revision: number;
  tag_name: string;
  commit_sha: string;
}

export interface ChannelSkillProvenance {
  repository_id: number;
  repository_url: string;
  git_ref: string;
  source_folder: string;
}

export interface ChannelSubscribedSkill {
  id: string;
  content_root: string;
  release_content_hash: string;
  release_content_hash_version: number;
  baseline_hash: string;
  baseline_hash_version: number;
  provenance: ChannelSkillProvenance;
}

export interface ChannelSkillPin {
  skill_id: string;
  target: ChannelReleaseTarget;
}

export type ChannelSubscriptionRemoteStatus =
  | "active"
  | "revoked"
  | "offline"
  | "integrity_error"
  | "recoverable_failure";

export interface ChannelSubscriptionRemoteState {
  status: ChannelSubscriptionRemoteStatus;
  checked_at: string | null;
  message: string | null;
}

export interface ChannelSubscription {
  descriptor_version: number;
  repository_id: number;
  organization_id: number;
  repository_url_aliases?: string[];
  target: ChannelReleaseTarget;
  skills: ChannelSubscribedSkill[];
  known_skill_ids: string[];
  pins: ChannelSkillPin[];
  last_update?: ChannelUpdateSnapshot | null;
  auto_update: ChannelAutoUpdateState;
  remote_state: ChannelSubscriptionRemoteState;
  created_at: string;
  updated_at: string;
}

export interface ChannelSubscriptionView {
  schema_version: number;
  descriptor_version: number;
  repository_id: number;
  organization_id: number | null;
  target: ChannelReleaseTarget | null;
  selected_skill_ids: string[];
  auto_update: ChannelAutoUpdateState;
  remote_state: ChannelSubscriptionRemoteState;
  read_only: boolean;
}

export interface ChannelRepositoryExposure {
  private_repository: boolean;
  full_repository_contents_readable: boolean;
  full_history_readable: boolean;
}

export interface ChannelSubscriptionReviewSkill {
  id: string;
  content_root: string;
  content_hash: string;
  content_hash_version: number;
  selected: boolean;
}

export interface ChannelSubscriptionReview {
  channel: SharedChannelDescriptor;
  target: ChannelReleaseTarget;
  title: string;
  notes: string;
  publisher: ChannelPublisherIdentity;
  published_at: string;
  exposure: ChannelRepositoryExposure;
  skills: ChannelSubscriptionReviewSkill[];
  read_only: boolean;
}

export interface SubscribeChannelRequest {
  repository_id: number;
  target: ChannelReleaseTarget;
  selected_skill_ids: string[];
}

export type ChannelUpdateStatus = "up_to_date" | "update_available" | "partially_upgraded" | "blocked";
export type ChannelUpdateChange = "added" | "updated" | "removed" | "unchanged";
export type ChannelUpdateItemState =
  | "current"
  | "available"
  | "applied"
  | "blocked"
  | "failed"
  | "notification"
  | "removed_from_channel";
export type ChannelUpdateBlockReason =
  | "local_content_changed"
  | "baseline_missing"
  | "snapshot_failed"
  | "removed_upstream";

export interface ChannelUpdateItem {
  id: string;
  change: ChannelUpdateChange;
  state: ChannelUpdateItemState;
  selected: boolean;
  from_content_hash: string | null;
  to_content_hash: string | null;
  block_reason: ChannelUpdateBlockReason | null;
  suggested_local_name: string | null;
  error: string | null;
  pinned_target?: ChannelReleaseTarget | null;
  error_code?: string | null;
}

export interface ChannelSkillRollbackTarget {
  target: ChannelReleaseTarget;
  title: string;
  published_at: string;
  content_hash: string;
}

export interface RollbackChannelSkillRequest {
  repository_id: number;
  skill_id: string;
  target: ChannelReleaseTarget;
  resolution?: LocalDivergenceResolution | null;
}

export interface ChannelSkillRollbackResult {
  snapshot: ChannelUpdateSnapshot;
  pin: ChannelSkillPin;
}

export interface ChannelUpdateSnapshot {
  target: ChannelReleaseTarget;
  title: string;
  notes: string;
  publisher: ChannelPublisherIdentity;
  published_at: string;
  checked_at: string;
  status: ChannelUpdateStatus;
  acknowledgement_required: boolean;
  items: ChannelUpdateItem[];
  check_error?: string | null;
  check_error_code?: string | null;
}

export type ChannelAutoUpdateRunStatus =
  | "checking"
  | "checked"
  | "up_to_date"
  | "applied"
  | "partially_applied"
  | "paused"
  | "retryable_failure"
  | "cancelled";

export type ChannelAutoUpdatePauseReason =
  | "pinned"
  | "local_content_changed"
  | "baseline_missing"
  | "snapshot_failed"
  | "permission_changed"
  | "removed_upstream"
  | "integrity_error"
  | "unresolved_failure"
  | "new_skill_requires_review";

export interface ChannelAutoUpdatePause {
  skill_id?: string | null;
  reason: ChannelAutoUpdatePauseReason;
  detail?: string | null;
}

export interface ChannelAutoUpdateRun {
  started_at: string;
  completed_at?: string | null;
  status: ChannelAutoUpdateRunStatus;
  target?: ChannelReleaseTarget | null;
  applied_skill_ids: string[];
  pauses: ChannelAutoUpdatePause[];
  error?: string | null;
  retryable: boolean;
}

export interface ChannelAutoUpdateState {
  enabled: boolean;
  next_check_at?: string | null;
  last_run?: ChannelAutoUpdateRun | null;
}

export interface ChannelAutoUpdateExecution {
  repository_id: number;
  run: ChannelAutoUpdateRun;
}

export interface ChannelSkillUpdateResolution {
  skill_id: string;
  resolution: LocalDivergenceResolution;
}

export interface ApplyChannelUpdateRequest {
  repository_id: number;
  target: ChannelReleaseTarget;
  resolutions: ChannelSkillUpdateResolution[];
}

export interface ApplyChannelUpdateResult {
  snapshot: ChannelUpdateSnapshot;
  applied_skill_ids: string[];
}

export interface ConvertRemovedChannelSkillRequest {
  repository_id: number;
  skill_id: string;
  local_name: string;
}

export interface HandleRemovedChannelSkillResult {
  skill_id: string;
  local_name: string | null;
  snapshot: ChannelUpdateSnapshot;
}

export interface InstallChannelSkillResult {
  subscription: ChannelSubscription;
  snapshot: ChannelUpdateSnapshot;
}

export interface HandleRevokedChannelSkillResult {
  skill_id: string;
  local_name: string | null;
  subscription: ChannelSubscription;
}
