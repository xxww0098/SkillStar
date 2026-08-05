import { tauriInvoke } from "../../../lib/ipc";
import type { CreateSharedChannelRequest } from "../../../types";

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
