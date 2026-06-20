/** Query-key factory for the SSH feature. Every TanStack Query key must come
 * from here so invalidation stays consistent. */
export const sshKeys = {
  all: ["ssh"] as const,
  hosts: () => [...sshKeys.all, "hosts"] as const,
  remoteSkills: (hostId: string, remoteDir: string) => [...sshKeys.all, "remote-skills", hostId, remoteDir] as const,
};
