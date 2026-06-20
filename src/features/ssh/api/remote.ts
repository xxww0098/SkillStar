/** Remote skill operations: connection test, host-key acceptance, and
 * push / list / delete over the live SSH session. */
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useCallback } from "react";
import { toast } from "sonner";
import i18n from "../../../i18n";
import { tauriInvoke } from "../../../lib/ipc";
import type {
  DiscoveryResult,
  MigrateResult,
  PushResult,
  RemoteSkill,
  SshHost,
  TestConnectionOutput,
} from "../../../lib/ipc/commands/ssh";
import { sshKeys } from "./keys";

/** Scan the remote $HOME for all agent skills directories and their skills
 * (discovery-based — finds grok/.agents/claude/… without a fixed table). */
export function useDiscoverRemoteSkillsQuery(hostId: string | null, enabled = true) {
  return useQuery<DiscoveryResult>({
    queryKey: [...sshKeys.all, "discover", hostId ?? ""],
    queryFn: () => tauriInvoke("discover_remote_skills", { hostId: hostId! }),
    enabled: !!hostId && enabled,
    staleTime: 30_000,
  });
}

export function useRemoteSkillsQuery(hostId: string | null, remoteDir: string, enabled = true) {
  return useQuery<RemoteSkill[]>({
    queryKey: sshKeys.remoteSkills(hostId ?? "", remoteDir),
    queryFn: () => tauriInvoke("list_remote_skills", { hostId: hostId!, remoteDir }),
    enabled: !!hostId && enabled,
    staleTime: 10_000,
  });
}

/** Probe a host and report the host-key trust state so the UI can TOFU-prompt. */
export function useTestConnection() {
  return useMutation({
    mutationFn: (def: SshHost) => tauriInvoke("test_ssh_connection", { def }),
    onError: (e: unknown) => toast.error(i18n.t("ssh.toast.testFailed"), { description: String(e) }),
  });
}

/** Persist an accepted server fingerprint for a host (TOFU confirmation). */
export function useAcceptHostKey() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: ({ id, host, fingerprint }: { id: string; host: string; fingerprint: string }) =>
      tauriInvoke("accept_ssh_host_key", { id, host, fingerprint }),
    onSuccess: () => {
      toast.success(i18n.t("ssh.toast.hostKeyAccepted"));
      queryClient.invalidateQueries({ queryKey: sshKeys.all });
    },
  });
}

export function usePushSkill() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: ({ hostId, skillName, remoteDir }: { hostId: string; skillName: string; remoteDir: string }) =>
      tauriInvoke("push_skill_to_remote", { hostId, skillName, remoteDir }),
    onSuccess: (r: PushResult, vars) => {
      toast.success(i18n.t("ssh.toast.pushed", { name: vars.skillName, files: r.files_uploaded }));
      queryClient.invalidateQueries({ queryKey: [...sshKeys.all, "discover", vars.hostId] });
      queryClient.invalidateQueries({
        queryKey: sshKeys.remoteSkills(vars.hostId, vars.remoteDir),
      });
    },
    onError: (e: unknown) => toast.error(i18n.t("ssh.toast.pushFailed"), { description: String(e) }),
  });
}

export function useMigrateRemoteSkill() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: ({
      hostId,
      skillName,
      agentSkillsDir,
      standalonePath,
    }: {
      hostId: string;
      skillName: string;
      agentSkillsDir: string;
      standalonePath: string;
    }) =>
      tauriInvoke("migrate_remote_skill_to_hub", {
        hostId,
        skillName,
        agentSkillsDir,
        standalonePath,
      }),
    onSuccess: (_r: MigrateResult, vars) => {
      toast.success(i18n.t("ssh.toast.migrated", { name: vars.skillName }));
      queryClient.invalidateQueries({ queryKey: [...sshKeys.all, "discover", vars.hostId] });
    },
    onError: (e: unknown) => toast.error(i18n.t("ssh.toast.migrateFailed"), { description: String(e) }),
  });
}

export function useDeleteRemoteSkill() {
  const queryClient = useQueryClient();
  const invalidateAll = useCallback(
    (hostId: string) => {
      queryClient.invalidateQueries({ queryKey: [...sshKeys.all, "discover", hostId] });
      queryClient.invalidateQueries({ queryKey: [...sshKeys.all, "remote-skills"] });
    },
    [queryClient],
  );
  return useMutation({
    mutationFn: ({ hostId, remotePath }: { hostId: string; remotePath: string }) =>
      tauriInvoke("delete_remote_skill", { hostId, remotePath }),
    onSuccess: (_v, vars) => {
      const name = vars.remotePath.split("/").pop() ?? vars.remotePath;
      toast.success(i18n.t("ssh.toast.deleted", { name }));
      invalidateAll(vars.hostId);
    },
    onError: (e: unknown) => toast.error(i18n.t("ssh.toast.deleteFailed"), { description: String(e) }),
  });
}

export type { TestConnectionOutput };
