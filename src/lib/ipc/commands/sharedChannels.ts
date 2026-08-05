import type {
  CreateSharedChannelRequest,
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
}
