import type { CreateSharedChannelRequest, GitHubOrganization, SharedChannelDescriptor } from "../../../types";

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
}
