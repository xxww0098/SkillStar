import { useQuery } from "@tanstack/react-query";
import { motion } from "framer-motion";
import { Layers, Plus, Server, Upload } from "lucide-react";
import { type ReactNode, useCallback, useEffect, useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { Toolbar } from "../../../components/layout/Toolbar";
import { Button } from "../../../components/ui/button";
import { EmptyState } from "../../../components/ui/EmptyState";
import { LoadingLogo } from "../../../components/ui/LoadingLogo";
import { ModalHeader, ModalShell } from "../../../components/ui/ModalShell";
import { useAgentProfiles } from "../../../hooks/useAgentProfiles";
import { useViewMode } from "../../../hooks/useViewMode";
import { tauriInvoke } from "../../../lib/ipc";
import type { RemoteSkill, SshHostListItem } from "../../../lib/ipc/commands/ssh";
import type { Skill } from "../../../types";
// Deep imports (not the my-skills barrel) to avoid a my-skills <-> ssh barrel cycle.
import { ScopeDetailDrawer } from "../../my-skills/components/ScopeDetailDrawer";
import { SkillGrid } from "../../my-skills/components/SkillGrid";
import { UninstallConfirmDialog } from "../../my-skills/components/UninstallConfirmDialog";
import { remoteHostItemKey } from "../../my-skills/lib/remoteHostKey";
import {
  useAcceptHostKey,
  useDeleteRemoteSkill,
  useDiscoverRemoteSkillsQuery,
  useMigrateRemoteSkill,
  usePushSkill,
} from "../api/remote";
import { useConnectStream } from "../hooks/useConnectStream";
import { remoteAgentProfile } from "../lib/remoteAgentProfile";
import { formatRemoteSize, remoteSkillToSkill } from "../lib/remoteSkillAsSkill";
import { RemoteBulkMigrateDialog } from "./RemoteBulkMigrateDialog";
import { RemoteConnectionLogPopover } from "./RemoteConnectionLogPopover";

interface ContentProps {
  /** Selected host, or null while hosts load / none exist / none selected. */
  host: SshHostListItem | null;
  hostsLoading: boolean;
  hasHosts: boolean;
  onAddHost: () => void;
  /** Scope switch element built by the page; rendered inside the toolbar title. */
  scopeSwitch: ReactNode;
  /** Remote host picker element built by the page; rendered in the filters row. */
  hostPicker: ReactNode;
}

/**
 * Remote (SSH host) skill workspace. Self-contained twin of
 * {@link import("../../my-skills").LocalSkillsContent}: owns its own toolbar
 * (host picker + connection console + push, all wired to its internal discovery
 * state — no upward `onDiscoveryUiChange` relay), the host gate, the detail
 * drawer, and the push/migrate/delete dialogs.
 */
export function RemoteSkillsContent({
  host,
  hostsLoading,
  hasHosts,
  onAddHost,
  scopeSwitch,
  hostPicker,
}: ContentProps) {
  const { t } = useTranslation();
  const { profiles } = useAgentProfiles();

  const connId = host ? remoteHostItemKey(host) : "";
  const defaultRemoteDir = host && host.source === "managed" ? host.default_remote_dir : "";

  const [searchQuery, setSearchQuery] = useState("");
  const [viewMode, setViewMode] = useViewMode("grid");
  const [agentFilter, setAgentFilter] = useState<string | null>(null);
  const [pushOpen, setPushOpen] = useState(false);
  const [consoleOpen, setConsoleOpen] = useState(false);
  const [drawerSkill, setDrawerSkill] = useState<RemoteSkill | null>(null);
  const [bulkMigrateOpen, setBulkMigrateOpen] = useState(false);
  const [pendingRemoteDelete, setPendingRemoteDelete] = useState<RemoteSkill | null>(null);
  const [remoteDeleting, setRemoteDeleting] = useState(false);

  const discovery = useDiscoverRemoteSkillsQuery(connId, host != null);
  const push = usePushSkill();
  const del = useDeleteRemoteSkill();
  const migrate = useMigrateRemoteSkill();
  const acceptKey = useAcceptHostKey();
  const { lines, pendingHostKey } = useConnectStream(connId || null);

  const agents = discovery.data?.agents ?? [];
  const allSkills = discovery.data?.skills ?? [];
  const remoteDir = useMemo(() => {
    if (agentFilter) {
      const hit = agents.find((a) => a.agent === agentFilter);
      if (hit?.path) return hit.path;
    }
    return agents[0]?.path || defaultRemoteDir || "~/.claude/skills";
  }, [agentFilter, agents, defaultRemoteDir]);

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

  const remoteAgentProfiles = useMemo(
    () => agents.map((a) => remoteAgentProfile(a.agent, profiles)),
    [agents, profiles],
  );

  const active = discovery.isFetching || push.isPending || del.isPending || migrate.isPending;

  const handleAcceptHostKey = useCallback(
    async (fingerprint: string) => {
      if (!host) return;
      await acceptKey.mutateAsync({ id: connId, host: host.host, fingerprint });
      void discovery.refetch();
    },
    [acceptKey, connId, host, discovery.refetch],
  );

  const handleRejectHostKey = useCallback(() => {
    void discovery.refetch();
  }, [discovery.refetch]);

  // Surface the connection console automatically when a host-key prompt arrives.
  useEffect(() => {
    if (pendingHostKey != null) setConsoleOpen(true);
  }, [pendingHostKey]);

  const standaloneSkills = useMemo(
    () => allSkills.filter((s) => (s.layout ?? "standalone") === "standalone"),
    [allSkills],
  );

  const bulkDismissKey = `skillstar.ssh.bulkMigrateDismissed.${connId}`;

  useEffect(() => {
    if (!host || discovery.isLoading || discovery.isFetching) return;
    if (standaloneSkills.length === 0) return;
    try {
      if (localStorage.getItem(bulkDismissKey) === "1") return;
    } catch {
      /* storage unavailable */
    }
    setBulkMigrateOpen(true);
  }, [host, discovery.isLoading, discovery.isFetching, standaloneSkills.length, bulkDismissKey]);

  const handleMigrateOne = useCallback(
    async (skill: RemoteSkill, agentSkillsDir: string) => {
      await migrate.mutateAsync({
        hostId: connId,
        skillName: skill.name,
        agentSkillsDir,
        standalonePath: skill.path,
      });
    },
    [connId, migrate],
  );

  const requestDelete = useCallback((skill: RemoteSkill) => {
    setPendingRemoteDelete(skill);
  }, []);

  const confirmRemoteDelete = useCallback(async () => {
    const skill = pendingRemoteDelete;
    if (!skill) return;
    setRemoteDeleting(true);
    try {
      await del.mutateAsync({ hostId: connId, remotePath: skill.path });
      setDrawerSkill((prev) => (prev?.path === skill.path ? null : prev));
      setPendingRemoteDelete(null);
      discovery.refetch();
    } finally {
      setRemoteDeleting(false);
    }
  }, [connId, del, discovery, pendingRemoteDelete]);

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
      <div className="flex min-w-0 flex-1 flex-col overflow-hidden">
        <Toolbar
          titleNode={
            <div className="flex flex-wrap items-center gap-3">
              <h1>{t("sidebar.skills")}</h1>
              {scopeSwitch}
            </div>
          }
          searchQuery={searchQuery}
          onSearchChange={setSearchQuery}
          sortBy="updated"
          onSortChange={() => {}}
          viewMode={viewMode}
          onViewModeChange={setViewMode}
          filtersLead={hostPicker}
          countText={
            <div className="flex items-center gap-1.5 font-medium">
              <Layers className="w-3 h-3 text-muted-foreground" />
              <span>{visibleRemote.length}</span>
            </div>
          }
          hideStarsSort
          hideSortControls
          agentProfiles={remoteAgentProfiles}
          agentFilter={agentFilter}
          onAgentFilterChange={setAgentFilter}
          onRefresh={() => discovery.refetch()}
          isRefreshing={discovery.isFetching}
          actionsLead={
            host ? (
              <>
                <RemoteConnectionLogPopover
                  open={consoleOpen}
                  onOpenChange={setConsoleOpen}
                  lines={lines}
                  pendingHostKey={pendingHostKey}
                  active={active}
                  attention={pendingHostKey != null}
                  onAcceptHostKey={handleAcceptHostKey}
                  onRejectHostKey={handleRejectHostKey}
                />
                <button
                  type="button"
                  onClick={() => setPushOpen(true)}
                  className="flex h-8 items-center gap-1.5 rounded-lg border border-border/80 bg-background/50 px-3 text-xs font-medium text-foreground/80 hover:bg-accent/10 shrink-0 focus-ring"
                >
                  <Upload className="size-3.5" />
                  {t("ssh.push")}
                </button>
              </>
            ) : undefined
          }
        />

        <motion.main
          initial={{ opacity: 0 }}
          animate={{ opacity: 1 }}
          transition={{ duration: 0.2 }}
          className="ss-page-scroll"
        >
          {hostsLoading ? (
            <div className="flex items-center justify-center py-20">
              <LoadingLogo size="lg" label={t("mySkills.loading")} />
            </div>
          ) : !hasHosts ? (
            <div className="flex flex-1 flex-col items-center justify-center px-6 py-16">
              <EmptyState
                icon={<Server className="size-6 text-muted-foreground" />}
                title={t("ssh.noHosts")}
                description={t("ssh.noHostsHint")}
                action={
                  <Button type="button" onClick={onAddHost}>
                    {t("ssh.addHost")}
                  </Button>
                }
              />
            </div>
          ) : !host ? (
            <div className="flex flex-1 items-center justify-center text-sm text-muted-foreground">
              {t("ssh.selectHost")}
            </div>
          ) : discovery.isLoading ? (
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
      </div>

      <ScopeDetailDrawer
        kind="remote"
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
            await Promise.all(names.map((name) => push.mutateAsync({ hostId: connId, skillName: name, remoteDir })));
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
  const [selected, setSelected] = useState<Set<string>>(new Set());
  const { data: skills = [], isLoading } = useQuery<Skill[]>({
    queryKey: ["ssh", "push-local-skills"],
    queryFn: () => tauriInvoke("list_skills"),
    enabled: open,
    staleTime: 30_000,
  });

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
          {isLoading ? (
            <div className="flex items-center justify-center py-10">
              <LoadingLogo size="sm" />
            </div>
          ) : sorted.length === 0 ? (
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
