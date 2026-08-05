import { tauriInvoke } from "../../../lib/ipc";
import type {
  ApplyChannelUpdateRequest,
  CreateChannelInvitationRequest,
  CreateSharedChannelRequest,
  ExistingChannelScanRequest,
  SubscribeChannelRequest,
  RollbackChannelSkillRequest,
} from "../../../types";

export function listSharedChannelOrganizations() {
  return tauriInvoke("list_shared_channel_organizations");
}

export function listSharedChannels() {
  return tauriInvoke("list_shared_channels");
}

export function createSharedChannel(request: CreateSharedChannelRequest) {
  return tauriInvoke("create_shared_channel", { request });
}

export function resumeSharedChannel(repositoryId: number) {
  return tauriInvoke("resume_shared_channel", { repositoryId });
}

export function listExistingChannelRepositories(organizationId: number) {
  return tauriInvoke("list_existing_channel_repositories", { organizationId });
}

export function scanExistingSharedChannel(request: ExistingChannelScanRequest, sessionId: string) {
  return tauriInvoke("scan_existing_shared_channel", { request, sessionId });
}

export function confirmExistingSharedChannel(sessionId: string) {
  return tauriInvoke("confirm_existing_shared_channel", { sessionId });
}

export function cancelExistingSharedChannelRegistration(sessionId: string) {
  return tauriInvoke("cancel_existing_shared_channel_registration", {
    sessionId,
  });
}

export function previewSharedChannelPublish(repositoryId: number, sessionId: string) {
  return tauriInvoke("preview_shared_channel_publish", {
    repositoryId,
    sessionId,
  });
}

export function publishSharedChannel(sessionId: string, title: string, notes: string) {
  return tauriInvoke("publish_shared_channel", { sessionId, title, notes });
}

export function cancelSharedChannelPublish(sessionId: string) {
  return tauriInvoke("cancel_shared_channel_publish", { sessionId });
}

export function listSharedChannelMembership(repositoryId: number) {
  return tauriInvoke("list_shared_channel_membership", { repositoryId });
}

export function inviteSharedChannelMember(request: CreateChannelInvitationRequest) {
  return tauriInvoke("invite_shared_channel_member", { request });
}

export function cancelSharedChannelInvitation(repositoryId: number, invitationId: number) {
  return tauriInvoke("cancel_shared_channel_invitation", {
    repositoryId,
    invitationId,
  });
}

export function resendSharedChannelInvitation(repositoryId: number, invitationId: number) {
  return tauriInvoke("resend_shared_channel_invitation", {
    repositoryId,
    invitationId,
  });
}

export function listSharedChannelInvitationInbox() {
  return tauriInvoke("list_shared_channel_invitation_inbox");
}

export function acceptSharedChannelInvitation(invitationId: number) {
  return tauriInvoke("accept_shared_channel_invitation", { invitationId });
}

export function declineSharedChannelInvitation(invitationId: number) {
  return tauriInvoke("decline_shared_channel_invitation", { invitationId });
}

export function resumeAcceptedSharedChannel(repositoryId: number) {
  return tauriInvoke("resume_accepted_shared_channel", { repositoryId });
}

export function listSharedChannelSubscriptions() {
  return tauriInvoke("list_shared_channel_subscriptions");
}

export function reviewSharedChannelSubscription(repositoryId: number) {
  return tauriInvoke("review_shared_channel_subscription", { repositoryId });
}

export function subscribeSharedChannel(request: SubscribeChannelRequest, sessionId: string) {
  return tauriInvoke("subscribe_shared_channel", { request, sessionId });
}

export function getSharedChannelUpdateState(repositoryId: number) {
  return tauriInvoke("get_shared_channel_update_state", { repositoryId });
}

export function checkSharedChannelUpdate(repositoryId: number) {
  return tauriInvoke("check_shared_channel_update", { repositoryId });
}

export function applySharedChannelUpdate(request: ApplyChannelUpdateRequest, sessionId: string) {
  return tauriInvoke("apply_shared_channel_update", { request, sessionId });
}

export function listSharedChannelSkillRollbackTargets(repositoryId: number, skillId: string) {
  return tauriInvoke("list_shared_channel_skill_rollback_targets", { repositoryId, skillId });
}

export function rollbackSharedChannelSkill(request: RollbackChannelSkillRequest, sessionId: string) {
  return tauriInvoke("rollback_shared_channel_skill", { request, sessionId });
}

export function resumeSharedChannelSkillFollowing(repositoryId: number, skillId: string) {
  return tauriInvoke("resume_shared_channel_skill_following", { repositoryId, skillId });
}

export function getSharedChannelAutoUpdateState(repositoryId: number) {
  return tauriInvoke("get_shared_channel_auto_update_state", { repositoryId });
}

export function setSharedChannelAutoUpdateEnabled(repositoryId: number, enabled: boolean) {
  return tauriInvoke("set_shared_channel_auto_update_enabled", { repositoryId, enabled });
}

export function runSharedChannelAutoUpdates(sessionId: string) {
  return tauriInvoke("run_shared_channel_auto_updates", { sessionId });
}
