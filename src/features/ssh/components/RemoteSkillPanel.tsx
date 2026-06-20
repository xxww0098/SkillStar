import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { motion } from "framer-motion";
import { Plus, Upload } from "lucide-react";
import { toast } from "sonner";
import { Button } from "../../../components/ui/button";
import { ModalHeader, ModalShell } from "../../../components/ui/ModalShell";
import { LoadingLogo } from "../../../components/ui/LoadingLogo";
import { SkillGrid } from "../../my-skills/components/SkillGrid";
import { useAgentProfiles } from "../../../hooks/useAgentProfiles";
import { tauriInvoke } from "../../../lib/ipc";
import type { Skill, ViewMode } from "../../../types";
import type { RemoteSkill, SshHostListItem } from "../../../lib/ipc/commands/ssh";
import {
  useAcceptHostKey,
  useDeleteRemoteSkill,
  useDiscoverRemoteSkillsQuery,
  useMigrateRemoteSkill,
  usePushSkill,
} from "../api/remote";
import { useConnectStream } from "../hooks/useConnectStream";
import { formatRemoteSize, remoteSkillToSkill } from "../lib/remoteSkillAsSkill";
import { remoteAgentProfile } from "../lib/remoteAgentProfile";

import { RemoteSkillDrawer } from "./RemoteSkillDrawer";
import { RemoteBulkMigrateDialog } from "./RemoteBulkMigrateDialog";
import { UninstallConfirmDialog } from "../../my-skills/components/UninstallConfirmDialog";

export interface RemoteDiscoveryUiState {
  visibleCount: number;
  loading: boolean;
  isFetching: boolean;
  refetch: () => void;
  connectAttention?: boolean;
  connectLines?: import("../hooks/useConnectStream").SshProgressLine[];
  pendingHostKey?: import("../hooks/useConnectStream").PendingHostKey | null;
  connectActive?: boolean;
  acceptHostKey?: (fingerprint: string) => Promise<void>;
  rejectHostKey?: () => void;
}

interface ContentProps {
  host: SshHostListItem;
  searchQuery?: string;
  viewMode?: ViewMode;
  agentFilter?: string | null;
  pushOpen?: boolean;
  onPushOpenChange?: (open: boolean) => void;
  onDiscoveryUiChange?: (state: RemoteDiscoveryUiState) => void;
}

function hostConn(host: SshHostListItem): { id: string; defaultRemoteDir: string } {
  if (host.source === "managed") {
    return { id: host.id, defaultRemoteDir: host.default_remote_dir };
  }
  return { id: `system:${host.alias}`, defaultRemoteDir: "" };
}

/** Remote skill grid aligned with local My Skills (`ss-page-scroll` + `SkillGrid`). */
export function RemoteSkillsContent({
  host,
  searchQuery = "",
  viewMode = "grid",
  agentFilter = null,
  pushOpen: pushOpenProp,
  onPushOpenChange,
  onDiscoveryUiChange,
}: ContentProps) {
  const { t } = useTranslation();
  const { profiles } = useAgentProfiles();
  const conn = hostConn(host);
  const [pushOpenInternal, setPushOpenInternal] = useState(false);
  const pushOpen = pushOpenProp ?? pushOpenInternal;
  const setPushOpen = onPushOpenChange ?? setPushOpenInternal;
  const [drawerSkill, setDrawerSkill] = useState<RemoteSkill | null>(null);
  const [bulkMigrateOpen, setBulkMigrateOpen] = useState(false);
  const [pendingRemoteDelete, setPendingRemoteDelete] = useState<RemoteSkill | null>(null);
  const [remoteDeleting, setRemoteDeleting] = useState(false);

  const discovery = useDiscoverRemoteSkillsQuery(conn.id, true);
  const push = usePushSkill();
  const del = useDeleteRemoteSkill();
  const migrate = useMigrateRemoteSkill();
  const acceptKey = useAcceptHostKey();
  const { lines, pendingHostKey } = useConnectStream(conn.id);

  const agents = discovery.data?.agents ?? [];
  const allSkills = discovery.data?.skills ?? [];
  const remoteDir = useMemo(() => {
    if (agentFilter) {
      const hit = agents.find((a) => a.agent === agentFilter);
      if (hit?.path) return hit.path;
    }
    return agents[0]?.path || conn.defaultRemoteDir || "~/.claude/skills";
  }, [agentFilter, agents, conn.defaultRemoteDir]);

  const visibleRemote = useMemo(() => {
    let list = agentFilter ? allSkills.filter((s) => s.agent === agentFilter) : allSkills;
    const q = searchQuery.trim().toLowerCase();
    if (q) {
      list = list.filter(
        (s) =>
          s.name.toLowerCase().includes(q) || s.path.toLowerCase().includes(q) || s.agent.toLowerCase().includes(q),
      );
    }
    return list;
  }, [allSkills, agentFilter, searchQuery]);

  const remoteByPath = useMemo(() => {
    const m = new Map<string, RemoteSkill>();
    for (const s of visibleRemote) m.set(s.path, s);
    return m;
  }, [visibleRemote]);

  const gridSkills = useMemo(
    () => visibleRemote.map((r) => remoteSkillToSkill(r, profiles)),
    [visibleRemote, profiles],
  );

  const discoveryRefetchRef = useRef(discovery.refetch);
  discoveryRefetchRef.current = discovery.refetch;

  const stableDiscoveryRefetch = useCallback(() => {
    void discoveryRefetchRef.current();
  }, []);

  const active = discovery.isFetching || push.isPending || del.isPending || migrate.isPending;

  const handleAcceptHostKey = useCallback(
    async (fingerprint: string) => {
      await acceptKey.mutateAsync({
        id: conn.id,
        host: host.host,
        fingerprint,
      });
      void discoveryRefetchRef.current();
    },
    [acceptKey, conn.id, host.host],
  );

  const handleRejectHostKey = useCallback(() => {
    void discoveryRefetchRef.current();
  }, []);

  useEffect(() => {
    onDiscoveryUiChange?.({
      visibleCount: visibleRemote.length,
      loading: discovery.isLoading,
      isFetching: discovery.isFetching,
      refetch: stableDiscoveryRefetch,
      connectAttention: pendingHostKey != null,
      connectLines: lines,
      pendingHostKey,
      connectActive: active,
      acceptHostKey: handleAcceptHostKey,
      rejectHostKey: handleRejectHostKey,
    });
  }, [
    onDiscoveryUiChange,
    visibleRemote.length,
    discovery.isLoading,
    discovery.isFetching,
    stableDiscoveryRefetch,
    pendingHostKey,
    lines,
    active,
    handleAcceptHostKey,
    handleRejectHostKey,
  ]);

  const standaloneSkills = useMemo(
    () => allSkills.filter((s) => (s.layout ?? "standalone") === "standalone"),
    [allSkills],
  );

  const bulkDismissKey = `skillstar.ssh.bulkMigrateDismissed.${conn.id}`;

  useEffect(() => {
    if (discovery.isLoading || discovery.isFetching) return;
    if (standaloneSkills.length === 0) return;
    try {
      if (localStorage.getItem(bulkDismissKey) === "1") return;
    } catch {
      /* storage unavailable */
    }
    setBulkMigrateOpen(true);
  }, [discovery.isLoading, discovery.isFetching, standaloneSkills.length, bulkDismissKey]);

  const handleMigrateOne = useCallback(
    async (skill: RemoteSkill, agentSkillsDir: string) => {
      await migrate.mutateAsync({
        hostId: conn.id,
        skillName: skill.name,
        agentSkillsDir,
        standalonePath: skill.path,
      });
    },
    [conn.id, migrate],
  );

  const requestDelete = useCallback((skill: RemoteSkill) => {
    setPendingRemoteDelete(skill);
  }, []);

  const confirmRemoteDelete = useCallback(async () => {
    const skill = pendingRemoteDelete;
    if (!skill) return;
    setRemoteDeleting(true);
    try {
      await del.mutateAsync({ hostId: conn.id, remotePath: skill.path });
      setDrawerSkill((prev) => (prev?.path === skill.path ? null : prev));
      setPendingRemoteDelete(null);
      discovery.refetch();
    } finally {
      setRemoteDeleting(false);
    }
  }, [conn.id, del, discovery, pendingRemoteDelete]);

  const getRemoteCardProps = useCallback(
    (skill: Skill) => {
      const remote = remoteByPath.get(skill.git_url);
      if (!remote) return undefined;
      return {
        agentProfile: remoteAgentProfile(remote.agent, profiles),
        sizeLabel: formatRemoteSize(remote.size),
      };
    },
    [remoteByPath, profiles],
  );

  return (
    <>
      <motion.main
        initial={{ opacity: 0 }}
        animate={{ opacity: 1 }}
        transition={{ duration: 0.2 }}
        className="ss-page-scroll"
      >
        {discovery.isLoading ? (
          <div className="flex items-center justify-center py-20">
            <LoadingLogo size="lg" label={t("mySkills.loading")} />
          </div>
        ) : (
          <SkillGrid
            skills={gridSkills}
            viewMode={viewMode}
            columnStrategy="auto-fill"
            minColumnWidth={320}
            onSkillClick={(skill) => {
              const remote = remoteByPath.get(skill.git_url);
              if (remote) setDrawerSkill(remote);
            }}
            onInstall={() => {}}
            onUpdate={() => {}}
            emptyMessage={t("ssh.noRemoteSkills")}
            emptyAction={
              <Button size="sm" variant="outline" onClick={() => setPushOpen(true)}>
                <Plus className="size-4" />
                {t("ssh.push")}
              </Button>
            }
            getRemoteCardProps={getRemoteCardProps}
          />
        )}
      </motion.main>

      <RemoteSkillDrawer
        skill={drawerSkill}
        onClose={() => setDrawerSkill(null)}
        onDelete={(s) => requestDelete(s)}
        deleting={remoteDeleting}
        builtinProfiles={profiles}
      />

      <UninstallConfirmDialog
        open={pendingRemoteDelete != null}
        skillNames={pendingRemoteDelete ? [pendingRemoteDelete.name] : []}
        uninstalling={remoteDeleting}
        onClose={() => setPendingRemoteDelete(null)}
        onConfirm={() => void confirmRemoteDelete()}
      />

      <RemoteBulkMigrateDialog
        open={bulkMigrateOpen}
        onOpenChange={(open) => {
          setBulkMigrateOpen(open);
          if (!open) {
            try {
              localStorage.setItem(bulkDismissKey, "1");
            } catch {
              /* ignore */
            }
          }
        }}
        skills={allSkills}
        agents={agents}
        onMigrateOne={handleMigrateOne}
        onComplete={() => discovery.refetch()}
      />

      <PushSkillDialog
        open={pushOpen}
        onOpenChange={setPushOpen}
        remoteDir={remoteDir}
        pending={push.isPending}
        onPush={async (names) => {
          try {
            await Promise.all(names.map((name) => push.mutateAsync({ hostId: conn.id, skillName: name, remoteDir })));
            setPushOpen(false);
            discovery.refetch();
          } catch (e) {
            toast.error(t("ssh.toast.pushFailed"), { description: String(e) });
          }
        }}
      />
    </>
  );
}

/** @deprecated Use {@link RemoteSkillsContent} inside My Skills remote scope. */
export function RemoteSkillPanel(props: ContentProps) {
  return <RemoteSkillsContent {...props} />;
}

function PushSkillDialog({
  open,
  onOpenChange,
  remoteDir,
  pending,
  onPush,
}: {
  open: boolean;
  onOpenChange: (v: boolean) => void;
  remoteDir: string;
  pending: boolean;
  onPush: (names: string[]) => Promise<void>;
}) {
  const { t } = useTranslation();
  const [skills, setSkills] = useState<Skill[]>([]);
  const [selected, setSelected] = useState<Set<string>>(new Set());

  useEffect(() => {
    if (!open) return;
    let cancelled = false;
    tauriInvoke("list_skills")
      .then((list) => {
        if (!cancelled) setSkills(list);
      })
      .catch(() => {
        if (!cancelled) setSkills([]);
      });
    return () => {
      cancelled = true;
    };
  }, [open]);

  const sorted = useMemo(() => [...skills].sort((a, b) => a.name.localeCompare(b.name)), [skills]);

  const toggle = (name: string) =>
    setSelected((prev) => {
      const next = new Set(prev);
      if (next.has(name)) next.delete(name);
      else next.add(name);
      return next;
    });

  return (
    <ModalShell
      open={open}
      onClose={() => onOpenChange(false)}
      ariaLabel={t("ssh.push")}
      panelClassName="max-w-[480px]"
    >
      <ModalHeader title={t("ssh.push")} onClose={() => onOpenChange(false)} />
      <div className="flex max-h-[60vh] flex-col gap-3 p-5">
        <p className="text-xs text-muted-foreground">
          {t("ssh.hubPushNote")} → <span className="font-mono">{remoteDir}</span>
        </p>
        <div className="flex-1 overflow-y-auto rounded-lg border border-border/40">
          {sorted.length === 0 ? (
            <div className="p-4 text-center text-sm text-muted-foreground">{t("ssh.noLocalSkills")}</div>
          ) : (
            <ul className="divide-y divide-border/30">
              {sorted.map((s) => {
                const checked = selected.has(s.name);
                return (
                  <li key={s.name}>
                    <label className="flex cursor-pointer items-center gap-3 px-3 py-2 text-sm hover:bg-accent/5">
                      <input
                        type="checkbox"
                        checked={checked}
                        onChange={() => toggle(s.name)}
                        className="accent-primary"
                      />
                      <span className="min-w-0 flex-1 truncate">{s.name}</span>
                    </label>
                  </li>
                );
              })}
            </ul>
          )}
        </div>
        <div className="flex justify-end gap-2">
          <Button variant="ghost" onClick={() => onOpenChange(false)}>
            {t("common.cancel")}
          </Button>
          <Button disabled={selected.size === 0 || pending} onClick={() => onPush([...selected])}>
            <Upload className="size-4" />
            {t("ssh.push")} {selected.size > 0 ? `(${selected.size})` : ""}
          </Button>
        </div>
      </div>
    </ModalShell>
  );
}
