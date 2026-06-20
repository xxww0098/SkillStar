import { useEffect, useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { tauriInvoke } from "../../../lib/ipc";
import type { SshHost, SshHostListItem } from "../../../lib/ipc/commands/ssh";
import type { SshHostFormValues } from "../../ssh";
import { useHostMutations, useSshHostsQuery } from "../../ssh";
import { REMOTE_HOST_STORAGE_KEY, remoteHostItemKey } from "../lib/remoteHostKey";

export function useMySkillsRemoteHosts() {
  const { t } = useTranslation();
  const { data: hosts, isLoading } = useSshHostsQuery();
  const { addMutation, updateMutation, deleteMutation } = useHostMutations();

  const [selectedKey, setSelectedKey] = useState<string | null>(() => {
    if (typeof localStorage === "undefined") return null;
    return localStorage.getItem(REMOTE_HOST_STORAGE_KEY);
  });
  const [formOpen, setFormOpen] = useState(false);
  const [editing, setEditing] = useState<SshHost | null>(null);

  const selectedHost = useMemo(
    () => hosts?.find((h) => remoteHostItemKey(h) === selectedKey) ?? null,
    [hosts, selectedKey],
  );

  useEffect(() => {
    if (!hosts?.length) return;
    const keys = new Set(hosts.map(remoteHostItemKey));
    if (selectedKey && keys.has(selectedKey)) return;
    const key = remoteHostItemKey(hosts[0]);
    setSelectedKey(key);
    localStorage.setItem(REMOTE_HOST_STORAGE_KEY, key);
  }, [hosts, selectedKey]);

  const selectHost = (item: SshHostListItem) => {
    const key = remoteHostItemKey(item);
    setSelectedKey(key);
    localStorage.setItem(REMOTE_HOST_STORAGE_KEY, key);
  };

  const openAddHost = () => {
    setEditing(null);
    setFormOpen(true);
  };

  const openEditHost = (host: SshHost) => {
    setEditing(host);
    setFormOpen(true);
  };

  const handleImportSystemHost = async (alias: string) => {
    try {
      const created = await tauriInvoke("import_system_host", { alias });
      setSelectedKey(created.id);
      localStorage.setItem(REMOTE_HOST_STORAGE_KEY, created.id);
      toast.success(t("ssh.toast.hostAdded"));
    } catch (e) {
      toast.error(String(e));
    }
  };

  const handleHostFormSubmit = (values: SshHostFormValues) => {
    const def: SshHost = {
      id: values.id ?? "",
      display_name: values.displayName.trim() || values.host,
      host: values.host.trim(),
      port: Number(values.port) || 22,
      username: values.username.trim(),
      auth_method:
        values.authMethod.kind === "password" ? { kind: "password" } : { kind: "key", key_path: values.keyPath.trim() },
      default_remote_dir: values.defaultRemoteDir.trim(),
    };
    const credential = values.credential?.trim() || undefined;
    if (editing) {
      updateMutation.mutate({ id: editing.id, def, credential }, { onSuccess: () => setFormOpen(false) });
    } else {
      addMutation.mutate(
        { def, credential },
        {
          onSuccess: (created) => {
            setSelectedKey(created.id);
            localStorage.setItem(REMOTE_HOST_STORAGE_KEY, created.id);
            setFormOpen(false);
          },
        },
      );
    }
  };

  return {
    hosts,
    isLoadingHosts: isLoading,
    selectedKey,
    selectedHost,
    selectHost,
    formOpen,
    setFormOpen,
    editing,
    openAddHost,
    openEditHost,
    deleteHost: (id: string) => deleteMutation.mutate(id),
    handleImportSystemHost,
    handleHostFormSubmit,
  };
}
