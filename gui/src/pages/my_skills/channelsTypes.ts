import type { Signal } from "nsh";
import { MySkillsChannels } from "skillstar-gui/native";
import { logoutGitHub, openGitHubSignIn, refreshGitHubAuth } from "./githubSignInSlot";
import { github } from "./state";

// ---- Domain JSON shapes matching `ss-channels` / `ss-skills` ----------------

export type SharedChannelRole = "owner" | "publisher" | "subscriber";
export type SharedChannelStatus =
  | "awaiting_app_installation"
  | "awaiting_invitation_acceptance"
  | "active";

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
  created_at: string;
  updated_at: string;
}

export interface ChannelReleaseTarget {
  revision: number;
  tag_name: string;
  commit_sha: string;
}

export interface ChannelAutoUpdateState {
  enabled: boolean;
  last_run_at?: string | null;
  last_error?: string | null;
}

export interface ChannelSubscriptionRemoteState {
  status: "active" | "revoked" | "offline" | "integrity_error" | "recoverable_failure";
  checked_at?: string | null;
  message?: string | null;
}

export interface ChannelSkillPin {
  skill_id: string;
  target: ChannelReleaseTarget;
}

export type ChannelUpdateItemState =
  | "current"
  | "available"
  | "applied"
  | "blocked"
  | "failed"
  | "notification"
  | "removed_from_channel";

export interface ChannelUpdateItem {
  id: string;
  change: "added" | "updated" | "removed" | "unchanged";
  state: ChannelUpdateItemState;
  selected: boolean;
  from_content_hash: string | null;
  to_content_hash: string | null;
  block_reason: string | null;
  suggested_local_name: string | null;
  error: string | null;
  pinned_target?: ChannelReleaseTarget | null;
}

export interface ChannelUpdateSnapshot {
  target: ChannelReleaseTarget;
  title: string;
  notes: string;
  publisher: { id: number; login: string };
  published_at: string;
  checked_at: string;
  status: "up_to_date" | "update_available" | "partially_upgraded" | "blocked";
  acknowledgement_required: boolean;
  items: ChannelUpdateItem[];
  check_error?: string | null;
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

export interface LocalSubscriptionRecord {
  view: ChannelSubscriptionView;
  update_snapshot: ChannelUpdateSnapshot | null;
  pins: ChannelSkillPin[];
}

export interface LocalChannelsSnapshot {
  channels: SharedChannelDescriptor[];
  subscriptions: LocalSubscriptionRecord[];
}

export interface GitHubOrgSummary {
  id: number;
  login: string;
  viewer_is_admin: boolean;
}

export interface RegistrationCandidateRepo {
  repository_id: number;
  owner: string;
  name: string;
  role: SharedChannelRole;
  already_registered: boolean;
}

export interface RegistrationScanPreview {
  session_id: string;
  repository: {
    repository_id: number;
    organization_id: number;
    owner: string;
    name: string;
  };
  total_files: number;
  unclassified_files: string[];
  skills: { id: string; relative_dir: string; name?: string }[];
}

export interface PublishPreview {
  session_id: string;
  repository_id: number;
  tag_name: string;
  commit_sha: string;
  skills: { id: string; status: string }[];
}

export interface ChannelMemberRecord {
  user: { id: number; login: string };
  role: SharedChannelRole;
  github_role_name: string;
}

export interface ChannelInvitationRecord {
  id: number;
  invitee: { id: number; login: string } | null;
  role: "subscriber" | "publisher";
  status: string;
}

export interface ChannelMembershipSnapshot {
  members: ChannelMemberRecord[];
  invitations: ChannelInvitationRecord[];
}

export interface InboxInvitation {
  id: number;
  repository_id: number;
  owner: string;
  repository_name: string;
  inviter: { id: number; login: string } | null;
  effective_role: SharedChannelRole;
}

export interface ChannelReview {
  repository_id: number;
  target: ChannelReleaseTarget;
  title: string;
  notes: string;
  publisher: { id: number; login: string };
  skills: { id: string; status: string; relative_dir?: string; content_hash?: string }[];
}

export interface RollbackTargetItem {
  target: ChannelReleaseTarget;
  title: string;
  published_at: string;
}

export type DivergenceChoice =
  | { type: "preserve"; local_name: string }
  | { type: "discard" };

export interface SkillUpdateResolutionInput {
  skill_id: string;
  resolution: DivergenceChoice;
}

export interface ChannelMarketplaceExportResult {
  root: string;
  repository_id: number;
  revision: number;
  plugin_names: string[];
  files: number;
}

export interface ChannelsBridge {
  snapshot(): Promise<LocalChannelsSnapshot>;
  setAutoUpdate(repositoryId: number, enabled: boolean): Promise<ChannelAutoUpdateState>;
  exportMarketplace(repositoryId: number, outDir: string): Promise<ChannelMarketplaceExportResult>;
  pickExportFolder(prompt: string): Promise<string | null>;
  listOrganizations(): Promise<GitHubOrgSummary[]>;
  createChannel(organization: string, repositoryName: string, description: string): Promise<SharedChannelDescriptor>;
  resumeChannel(repositoryId: number): Promise<SharedChannelDescriptor>;
  listRegistrationCandidates(organizationId: number): Promise<RegistrationCandidateRepo[]>;
  scanExistingRepo(organizationId: number, repositoryId: number, sessionId: string): Promise<RegistrationScanPreview>;
  confirmRegistration(sessionId: string): Promise<SharedChannelDescriptor>;
  cancelRegistration(sessionId: string): Promise<boolean>;
  publishPreview(repositoryId: number, sessionId: string): Promise<PublishPreview>;
  publishRelease(sessionId: string, title: string, notes: string): Promise<{ manifest: { tag_name: string } }>;
  cancelPublish(sessionId: string): Promise<boolean>;
  listMembership(repositoryId: number): Promise<ChannelMembershipSnapshot>;
  inviteMember(repositoryId: number, username: string, role: "subscriber" | "publisher"): Promise<{ username: string; status: string }>;
  cancelInvitation(repositoryId: number, invitationId: number): Promise<{ username: string }>;
  resendInvitation(repositoryId: number, invitationId: number): Promise<{ username: string; status: string }>;
  revokeMember(repositoryId: number, username: string): Promise<{
    username: string;
    status: string;
    effective_role?: string | null;
    access_source?: string | null;
  }>;
  listInbox(): Promise<InboxInvitation[]>;
  acceptInvitation(invitationId: number): Promise<SharedChannelDescriptor>;
  resumeAcceptedChannel(repositoryId: number): Promise<SharedChannelDescriptor>;
  declineInvitation(invitationId: number): Promise<{ id?: number }>;
  reviewChannel(repositoryId: number): Promise<ChannelReview>;
  subscribeChannel(repositoryId: number, target: ChannelReleaseTarget, selectedSkillIds: string[]): Promise<{ target: ChannelReleaseTarget }>;
  checkUpdate(repositoryId: number): Promise<ChannelUpdateSnapshot>;
  applyUpdate(repositoryId: number, target: ChannelReleaseTarget, resolutions: SkillUpdateResolutionInput[]): Promise<{
    snapshot: ChannelUpdateSnapshot;
    applied_skill_ids: string[];
  }>;
  listRollbackTargets(repositoryId: number, skillId: string): Promise<RollbackTargetItem[]>;
  rollbackSkill(
    repositoryId: number,
    skillId: string,
    target: ChannelReleaseTarget,
    resolution: DivergenceChoice | null,
  ): Promise<{ pin: ChannelSkillPin; snapshot: ChannelUpdateSnapshot }>;
  resumeFollowingSkill(repositoryId: number, skillId: string): Promise<ChannelUpdateSnapshot>;
  installChannelSkill(repositoryId: number, skillId: string): Promise<{ snapshot: ChannelUpdateSnapshot }>;
  uninstallRemovedSkill(repositoryId: number, skillId: string, revoked: boolean): Promise<{ skill_id: string }>;
  convertRemovedSkillToLocal(
    repositoryId: number,
    skillId: string,
    localName: string,
    revoked: boolean,
  ): Promise<{ skill_id: string; local_name?: string | null }>;
}

export interface ChannelsAuthControl {
  auth: Signal<{ state: string; login: string | null } | null>;
  openSignIn(): void;
  logout(): Promise<void>;
  refreshAuth(): Promise<void>;
}

export const realChannelsBridge: ChannelsBridge = {
  snapshot: () => MySkillsChannels.snapshot() as Promise<LocalChannelsSnapshot>,
  setAutoUpdate: (repoId, enabled) =>
    MySkillsChannels.setAutoUpdate(repoId, enabled) as Promise<ChannelAutoUpdateState>,
  exportMarketplace: (repoId, outDir) =>
    MySkillsChannels.exportMarketplace(repoId, outDir) as Promise<ChannelMarketplaceExportResult>,
  pickExportFolder: (prompt) => MySkillsChannels.pickExportFolder(prompt),
  listOrganizations: () => MySkillsChannels.listOrganizations() as Promise<GitHubOrgSummary[]>,
  createChannel: (org, name, desc) =>
    MySkillsChannels.createChannel(org, name, desc) as Promise<SharedChannelDescriptor>,
  resumeChannel: (repoId) => MySkillsChannels.resumeChannel(repoId) as Promise<SharedChannelDescriptor>,
  listRegistrationCandidates: (orgId) =>
    MySkillsChannels.listRegistrationCandidates(orgId) as Promise<RegistrationCandidateRepo[]>,
  scanExistingRepo: (orgId, repoId, sessionId) =>
    MySkillsChannels.scanExistingRepo(orgId, repoId, sessionId) as Promise<RegistrationScanPreview>,
  confirmRegistration: (sessionId) =>
    MySkillsChannels.confirmRegistration(sessionId) as Promise<SharedChannelDescriptor>,
  cancelRegistration: (sessionId) => MySkillsChannels.cancelRegistration(sessionId),
  publishPreview: (repoId, sessionId) =>
    MySkillsChannels.publishPreview(repoId, sessionId) as Promise<PublishPreview>,
  publishRelease: (sessionId, title, notes) =>
    MySkillsChannels.publishRelease(sessionId, title, notes) as Promise<{ manifest: { tag_name: string } }>,
  cancelPublish: (sessionId) => MySkillsChannels.cancelPublish(sessionId),
  listMembership: (repoId) =>
    MySkillsChannels.listMembership(repoId) as Promise<ChannelMembershipSnapshot>,
  inviteMember: (repoId, username, role) =>
    MySkillsChannels.inviteMember(repoId, username, role) as Promise<{ username: string; status: string }>,
  cancelInvitation: (repoId, invId) =>
    MySkillsChannels.cancelInvitation(repoId, invId) as Promise<{ username: string }>,
  resendInvitation: (repoId, invId) =>
    MySkillsChannels.resendInvitation(repoId, invId) as Promise<{ username: string; status: string }>,
  revokeMember: (repoId, username) =>
    MySkillsChannels.revokeMember(repoId, username) as Promise<{
      username: string;
      status: string;
      effective_role?: string | null;
      access_source?: string | null;
    }>,
  listInbox: () => MySkillsChannels.listInbox() as Promise<InboxInvitation[]>,
  acceptInvitation: (invId) =>
    MySkillsChannels.acceptInvitation(invId) as Promise<SharedChannelDescriptor>,
  resumeAcceptedChannel: (repoId) =>
    MySkillsChannels.resumeAcceptedChannel(repoId) as Promise<SharedChannelDescriptor>,
  declineInvitation: (invId) =>
    MySkillsChannels.declineInvitation(invId) as Promise<{ id?: number }>,
  reviewChannel: (repoId) => MySkillsChannels.reviewChannel(repoId) as Promise<ChannelReview>,
  subscribeChannel: (repoId, target, ids) =>
    MySkillsChannels.subscribeChannel(repoId, target, ids) as Promise<{ target: ChannelReleaseTarget }>,
  checkUpdate: (repoId) => MySkillsChannels.checkUpdate(repoId) as Promise<ChannelUpdateSnapshot>,
  applyUpdate: (repoId, target, resolutions) =>
    MySkillsChannels.applyUpdate(repoId, target, resolutions) as Promise<{
      snapshot: ChannelUpdateSnapshot;
      applied_skill_ids: string[];
    }>,
  listRollbackTargets: (repoId, skillId) =>
    MySkillsChannels.listRollbackTargets(repoId, skillId) as Promise<RollbackTargetItem[]>,
  rollbackSkill: (repoId, skillId, target, resolution) =>
    MySkillsChannels.rollbackSkill(repoId, skillId, target, resolution) as Promise<{
      pin: ChannelSkillPin;
      snapshot: ChannelUpdateSnapshot;
    }>,
  resumeFollowingSkill: (repoId, skillId) =>
    MySkillsChannels.resumeFollowingSkill(repoId, skillId) as Promise<ChannelUpdateSnapshot>,
  installChannelSkill: (repoId, skillId) =>
    MySkillsChannels.installChannelSkill(repoId, skillId) as Promise<{ snapshot: ChannelUpdateSnapshot }>,
  uninstallRemovedSkill: (repoId, skillId, revoked) =>
    MySkillsChannels.uninstallRemovedSkill(repoId, skillId, revoked) as Promise<{ skill_id: string }>,
  convertRemovedSkillToLocal: (repoId, skillId, localName, revoked) =>
    MySkillsChannels.convertRemovedSkillToLocal(repoId, skillId, localName, revoked) as Promise<{
      skill_id: string;
      local_name?: string | null;
    }>,
};

export const realAuthControl: ChannelsAuthControl = {
  auth: github,
  openSignIn: openGitHubSignIn,
  logout: logoutGitHub,
  refreshAuth: refreshGitHubAuth,
};

export type ActiveModal =
  | null
  | { kind: "create" }
  | { kind: "register" }
  | { kind: "review"; repositoryId: number }
  | { kind: "publish"; repositoryId: number }
  | { kind: "members"; repositoryId: number }
  | { kind: "rollback"; repositoryId: number; skillId: string }
  | { kind: "convert_local"; repositoryId: number; skillId: string; revoked: boolean }
  | { kind: "export_marketplace"; repositoryId: number };

export interface ChannelListEntry {
  repositoryId: number;
  descriptor: SharedChannelDescriptor | null;
  subscription: LocalSubscriptionRecord | null;
  displayName: string;
}
