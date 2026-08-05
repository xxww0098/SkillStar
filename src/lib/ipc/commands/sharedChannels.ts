import type {
  CreateSharedChannelRequest,
  ChannelPublishPreview,
  ChannelPublishResult,
  ChannelInvitation,
  ChannelInvitationAction,
  ChannelMembershipSnapshot,
  CreateChannelInvitationRequest,
  ExistingChannelRepositoryCandidate,
  ExistingChannelScanPreview,
  ExistingChannelScanRequest,
  GitHubOrganization,
  SharedChannelDescriptor,
  ChannelSubscription,
  ChannelSubscriptionReview,
  ChannelSubscriptionView,
  SubscribeChannelRequest,
  ApplyChannelUpdateRequest,
  ApplyChannelUpdateResult,
  ChannelUpdateSnapshot,
  ChannelAutoUpdateExecution,
  ChannelAutoUpdateState,
} from "../../../types";

export interface SharedChannelCommands {
  list_shared_channel_organizations: {
    args: Record<string, never>;
    result: GitHubOrganization[];
  };
  list_shared_channels: {
    args: Record<string, never>;
    result: SharedChannelDescriptor[];
  };
  create_shared_channel: {
    args: { request: CreateSharedChannelRequest };
    result: SharedChannelDescriptor;
  };
  resume_shared_channel: {
    args: { repositoryId: number };
    result: SharedChannelDescriptor;
  };
  list_existing_channel_repositories: {
    args: { organizationId: number };
    result: ExistingChannelRepositoryCandidate[];
  };
  scan_existing_shared_channel: {
    args: { request: ExistingChannelScanRequest; sessionId: string };
    result: ExistingChannelScanPreview;
  };
  confirm_existing_shared_channel: {
    args: { sessionId: string };
    result: SharedChannelDescriptor;
  };
  cancel_existing_shared_channel_registration: {
    args: { sessionId: string };
    result: boolean;
  };
  preview_shared_channel_publish: {
    args: { repositoryId: number; sessionId: string };
    result: ChannelPublishPreview;
  };
  publish_shared_channel: {
    args: { sessionId: string; title: string; notes: string };
    result: ChannelPublishResult;
  };
  cancel_shared_channel_publish: {
    args: { sessionId: string };
    result: boolean;
  };
  list_shared_channel_membership: {
    args: { repositoryId: number };
    result: ChannelMembershipSnapshot;
  };
  invite_shared_channel_member: {
    args: { request: CreateChannelInvitationRequest };
    result: ChannelInvitationAction;
  };
  cancel_shared_channel_invitation: {
    args: { repositoryId: number; invitationId: number };
    result: ChannelInvitationAction;
  };
  resend_shared_channel_invitation: {
    args: { repositoryId: number; invitationId: number };
    result: ChannelInvitationAction;
  };
  list_shared_channel_invitation_inbox: {
    args: Record<string, never>;
    result: ChannelInvitation[];
  };
  accept_shared_channel_invitation: {
    args: { invitationId: number };
    result: SharedChannelDescriptor;
  };
  decline_shared_channel_invitation: {
    args: { invitationId: number };
    result: ChannelInvitationAction;
  };
  resume_accepted_shared_channel: {
    args: { repositoryId: number };
    result: SharedChannelDescriptor;
  };
  list_shared_channel_subscriptions: {
    args: Record<string, never>;
    result: ChannelSubscriptionView[];
  };
  review_shared_channel_subscription: {
    args: { repositoryId: number };
    result: ChannelSubscriptionReview;
  };
  subscribe_shared_channel: {
    args: { request: SubscribeChannelRequest; sessionId: string };
    result: ChannelSubscription;
  };
  get_shared_channel_update_state: {
    args: { repositoryId: number };
    result: ChannelUpdateSnapshot | null;
  };
  check_shared_channel_update: {
    args: { repositoryId: number };
    result: ChannelUpdateSnapshot;
  };
  apply_shared_channel_update: {
    args: { request: ApplyChannelUpdateRequest; sessionId: string };
    result: ApplyChannelUpdateResult;
  };
  get_shared_channel_auto_update_state: {
    args: { repositoryId: number };
    result: ChannelAutoUpdateState;
  };
  set_shared_channel_auto_update_enabled: {
    args: { repositoryId: number; enabled: boolean };
    result: ChannelAutoUpdateState;
  };
  run_shared_channel_auto_updates: {
    args: { sessionId: string };
    result: ChannelAutoUpdateExecution[];
  };
}
