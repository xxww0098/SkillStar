import { tauriInvoke } from "../../../lib/ipc";
import type { CreateSharedChannelRequest, ExistingChannelScanRequest } from "../../../types";

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
  return tauriInvoke("cancel_existing_shared_channel_registration", { sessionId });
}
