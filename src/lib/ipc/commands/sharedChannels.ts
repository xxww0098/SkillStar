import type {
  CreateSharedChannelRequest,
  ChannelPublishPreview,
  ChannelPublishResult,
  ExistingChannelRepositoryCandidate,
  ExistingChannelScanPreview,
  ExistingChannelScanRequest,
  GitHubOrganization,
  SharedChannelDescriptor,
} from "../../../types";

export interface SharedChannelCommands {
  list_shared_channel_organizations: { args: Record<string, never>; result: GitHubOrganization[] };
  list_shared_channels: { args: Record<string, never>; result: SharedChannelDescriptor[] };
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
}
