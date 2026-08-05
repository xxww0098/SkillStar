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

export interface ChannelSubscription {
  descriptor_version: number;
  repository_id: number;
  organization_id: number;
  target: ChannelReleaseTarget;
  skills: ChannelSubscribedSkill[];
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
