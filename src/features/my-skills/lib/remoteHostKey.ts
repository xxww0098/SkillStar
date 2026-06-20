import type { SshHostListItem } from "../../../lib/ipc/commands/ssh";

export const REMOTE_HOST_STORAGE_KEY = "skillstar.mySkills.remoteHostKey";

export function remoteHostItemKey(item: SshHostListItem): string {
  return item.source === "managed" ? item.id : `system:${item.alias}`;
}

export function remoteHostLabel(item: SshHostListItem): string {
  if (item.source === "managed") {
    return item.display_name || `${item.username}@${item.host}`;
  }
  return item.alias;
}
