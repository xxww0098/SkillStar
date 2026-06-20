/** SSH host CRUD: query + mutations over the `ssh_hosts.toml` store.
 * All mutations follow the project convention: optimistic onMutate → toast
 * onError → invalidate onSettled. */
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useCallback } from "react";
import { toast } from "sonner";
import i18n from "../../../i18n";
import { tauriInvoke } from "../../../lib/ipc";
import type { SshHost, SshHostListItem } from "../../../lib/ipc/commands/ssh";
import { sshKeys } from "./keys";

const HOSTS_STALE_MS = 30_000;

export function useSshHostsQuery() {
  return useQuery<SshHostListItem[]>({
    queryKey: sshKeys.hosts(),
    queryFn: () => tauriInvoke("list_ssh_hosts"),
    staleTime: HOSTS_STALE_MS,
  });
}

export function useHostMutations() {
  const queryClient = useQueryClient();
  const queryKey = sshKeys.hosts();

  const invalidate = useCallback(() => {
    queryClient.invalidateQueries({ queryKey: sshKeys.all });
  }, [queryClient]);

  const addMutation = useMutation({
    mutationFn: ({ def, credential }: { def: SshHost; credential?: string }) =>
      tauriInvoke("add_ssh_host", { def, credential }),
    onSuccess: (created) => {
      if (created?.id) {
        queryClient.setQueryData<SshHost[]>(queryKey, (prev) => (prev ? [...prev, created] : [created]));
      }
      toast.success(i18n.t("ssh.toast.hostAdded"));
      invalidate();
    },
    onError: (e: unknown) => toast.error(String(e)),
  });

  const updateMutation = useMutation({
    mutationFn: ({ id, def, credential }: { id: string; def: SshHost; credential?: string }) =>
      tauriInvoke("update_ssh_host", { id, def, credential }),
    onMutate: async ({ id, def }) => {
      await queryClient.cancelQueries({ queryKey });
      const prev = queryClient.getQueryData<SshHost[]>(queryKey);
      queryClient.setQueryData<SshHost[]>(queryKey, (old) => (old ?? []).map((h) => (h.id === id ? def : h)));
      return { prev };
    },
    onError: (e: unknown, _v, ctx) => {
      if (ctx?.prev) queryClient.setQueryData(queryKey, ctx.prev);
      toast.error(String(e));
    },
    onSuccess: () => toast.success(i18n.t("ssh.toast.hostUpdated")),
    onSettled: () => invalidate(),
  });

  const deleteMutation = useMutation({
    mutationFn: (id: string) => tauriInvoke("delete_ssh_host", { id }),
    onMutate: async (id) => {
      await queryClient.cancelQueries({ queryKey });
      const prev = queryClient.getQueryData<SshHost[]>(queryKey);
      queryClient.setQueryData<SshHost[]>(queryKey, (old) => (old ?? []).filter((h) => h.id !== id));
      return { prev };
    },
    onError: (e: unknown, _v, ctx) => {
      if (ctx?.prev) queryClient.setQueryData(queryKey, ctx.prev);
      toast.error(i18n.t("ssh.toast.deleteFailed"), { description: String(e) });
    },
    onSuccess: () => toast.success(i18n.t("ssh.toast.hostDeleted")),
    onSettled: () => invalidate(),
  });

  return { addMutation, updateMutation, deleteMutation };
}
