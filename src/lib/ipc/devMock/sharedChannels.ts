import type { SharedChannelDescriptor } from "../../../types";
import type { DevMockHandlers } from "./shared";

let channels: SharedChannelDescriptor[] = [];

export const SHARED_CHANNEL_HANDLERS: DevMockHandlers = {
  list_shared_channel_organizations: () => [
    { id: 7, login: "acme", avatar_url: null, viewer_is_admin: true },
    { id: 8, login: "design-lab", avatar_url: null, viewer_is_admin: false },
  ],
  list_shared_channels: () => channels,
  create_shared_channel: (args) => {
    const request = (args?.request ?? {}) as { organization?: string; repository_name?: string };
    const owner = request.organization || "acme";
    const name = request.repository_name || "skillstar-team";
    const now = new Date().toISOString();
    const channel: SharedChannelDescriptor = {
      descriptor_version: 1,
      repository_id: 42,
      organization_id: 7,
      owner,
      name,
      html_url: `https://github.com/${owner}/${name}`,
      clone_url: `https://github.com/${owner}/${name}.git`,
      role: "owner",
      status: "active",
      authorization: { repository_selection: "selected", administration: "write", contents: "write" },
      created_at: now,
      updated_at: now,
    };
    channels = [channel];
    return channel;
  },
  resume_shared_channel: (args) => {
    const repositoryId = Number(args?.repositoryId ?? 0);
    const current = channels.find((channel) => channel.repository_id === repositoryId);
    if (!current) throw new Error("repository_not_found: Pending shared channel not found");
    const active = { ...current, status: "active" as const, updated_at: new Date().toISOString() };
    channels = channels.map((channel) => (channel.repository_id === repositoryId ? active : channel));
    return active;
  },
};
