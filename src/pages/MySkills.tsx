import { tauriInvoke } from "../lib/ipc";
import { AnimatePresence, motion } from "framer-motion";
import { AlertTriangle, Globe, Layers, Server, Upload } from "lucide-react";
import { lazy, Suspense, useCallback, useEffect, useMemo, useState } from "react";

import { useTranslation } from "react-i18next";
import { Toolbar } from "../components/layout/Toolbar";
import { Button } from "../components/ui/button";
import { LoadingLogo } from "../components/ui/LoadingLogo";
import { AiPickSkillsModal } from "../features/my-skills/components/AiPickSkillsModal";
import { CreateGroupModal } from "../features/my-skills/components/CreateGroupModal";
import { DeployToProjectModal } from "../features/my-skills/components/DeployToProjectModal";
import { ExportShareCodeModal } from "../features/my-skills/components/ExportShareCodeModal";
import { ImportBundleModal } from "../features/my-skills/components/ImportBundleModal";
import { ImportModal } from "../features/my-skills/components/ImportModal";
import { PublishSkillModal } from "../features/my-skills/components/PublishSkillModal";
import { MySkillsRemoteHostPicker } from "../features/my-skills/components/MySkillsRemoteHostPicker";
import { useMySkillsRemoteHosts } from "../features/my-skills/hooks/useMySkillsRemoteHosts";
import { MySkillsScopeSwitch, type MySkillsScope } from "../features/my-skills/components/MySkillsScopeSwitch";
import { SkillGrid } from "../features/my-skills/components/SkillGrid";
import { SkillSelectionBar } from "../features/my-skills/components/SkillSelectionBar";
import { UninstallConfirmDialog } from "../features/my-skills/components/UninstallConfirmDialog";
import { useSkillCards } from "../features/my-skills/hooks/useSkillCards";
import { useSkills } from "../features/my-skills/hooks/useSkills";
import { useAgentProfiles } from "../hooks/useAgentProfiles";
import { useSkillsSelectionShortcuts } from "../hooks/useSkillsSelectionShortcuts";
import { useViewMode } from "../hooks/useViewMode";
import { toast } from "../lib/toast";
import { navigateToSettingsSection } from "../lib/utils";
import type { RepoNewSkill, Skill, SortOption } from "../types";
import { EmptyState } from "../components/ui/EmptyState";
import { SshHostForm } from "../features/ssh";
import { RemoteSkillsContent, type RemoteDiscoveryUiState } from "../features/ssh/components/RemoteSkillPanel";
import { RemoteConnectionLogPopover } from "../features/ssh/components/RemoteConnectionLogPopover";
import { useDiscoverRemoteSkillsQuery } from "../features/ssh/api/remote";
import { remoteAgentProfile } from "../features/ssh/lib/remoteAgentProfile";

const DetailPanel = lazy(() =>
  import("../components/layout/DetailPanel").then((mod) => ({
    default: mod.DetailPanel,
  })),
);

interface MySkillsProps {
  initialFocusSkill?: string | null;
  onClearFocus?: () => void;
  onPackSkills?: (skills: string[]) => void;
  /** Pre-filled share code from clipboard auto-detect */
  initialShareCode?: string;
  /** Clear consumed share code */
  onClearShareCode?: () => void;
}

export function MySkills({
  initialFocusSkill,
  onClearFocus,
  onPackSkills,
  initialShareCode,
  onClearShareCode,
}: MySkillsProps = {}) {
  const { t } = useTranslation();
  const {
    skills,
    loading,
    refresh,
    installSkill,
    uninstallSkill,
    updateSkill,
    pendingUpdateNames,
    toggleSkillForAgent,
    pendingAgentToggleKeys,
    readSkillContent,
    updateSkillContent,
    batchRemoveSkillsFromAllAgents,
    batchAiProcessSkills,
    ghostSkills,
    dismissGhostSkill,
    dismissGhostRepo,
    installGhostSkill,
  } = useSkills();
  const { profiles, deploySkillsToProject } = useAgentProfiles();
  const { createGroup, groups } = useSkillCards();
  const remoteHosts = useMySkillsRemoteHosts();
  const SCOPE_STORAGE_KEY = "skillstar.mySkills.scope";
  const [skillsScope, setSkillsScope] = useState<MySkillsScope>(() => {
    if (typeof localStorage === "undefined") return "local";
    return localStorage.getItem(SCOPE_STORAGE_KEY) === "remote" ? "remote" : "local";
  });
  useEffect(() => {
    if (typeof window === "undefined") return;
    const hash = window.location.hash.slice(1);
    if (hash === "ssh") {
      setSkillsScope("remote");
      localStorage.setItem(SCOPE_STORAGE_KEY, "remote");
      window.location.hash = "skills";
    }
  }, []);

  const [searchQuery, setSearchQuery] = useState("");
  const [sortBy, setSortBy] = useState<SortOption>("updated");
  const [showUpdateOnly, setShowUpdateOnly] = useState(false);
  const [viewMode, setViewMode] = useViewMode("grid");
  const [agentFilter, setAgentFilter] = useState<string | null>(null);
  const [remotePushOpen, setRemotePushOpen] = useState(false);
  const [remoteConsoleOpen, setRemoteConsoleOpen] = useState(false);
  const [remoteUi, setRemoteUi] = useState<RemoteDiscoveryUiState | null>(null);

  useEffect(() => {
    if (skillsScope === "remote" && remoteUi?.connectAttention) {
      setRemoteConsoleOpen(true);
    }
  }, [skillsScope, remoteUi?.connectAttention]);
  const [selectedSkill, setSelectedSkill] = useState<Skill | null>(null);
  const [selectedSkillNames, setSelectedSkillNames] = useState<Set<string>>(new Set());
  const [quickPackSkills, setQuickPackSkills] = useState<string[]>([]);
  const [deployModalOpen, setDeployModalOpen] = useState(false);
  const [groupModalOpen, setGroupModalOpen] = useState(false);
  const [uninstallDialogOpen, setUninstallDialogOpen] = useState(false);
  const [pendingUninstallNames, setPendingUninstallNames] = useState<string[]>([]);
  const [uninstalling, setUninstalling] = useState(false);
  const [uninstallError, setUninstallError] = useState<string | null>(null);
  const [importModalOpen, setImportModalOpen] = useState(false);
  const [importBundleOpen, setImportBundleOpen] = useState(false);
  const [publishTarget, setPublishTarget] = useState<string | null>(null);
  const [aiPickModalOpen, setAiPickModalOpen] = useState(false);
  const [brokenCount, setBrokenCount] = useState(0);
  const [sourceFilter, setSourceFilter] = useState<"all" | "hub" | "local">("all");
  const [repoFilter, setRepoFilter] = useState<string | null>(null);
  const [shareCardSkills, setShareCardSkills] = useState<string[] | null>(null);
  const localCount = useMemo(() => skills.filter((s) => s.skill_type === "local").length, [skills]);
  const pendingUpdateCount = useMemo(() => skills.filter((skill) => skill.update_available).length, [skills]);

  /** Sorted unique repo source strings for the repo filter popover */
  const repoSources = useMemo(() => {
    const set = new Set<string>();
    for (const skill of skills) {
      if (skill.source) set.add(skill.source);
    }
    return Array.from(set).sort((a, b) => a.localeCompare(b));
  }, [skills]);
  const [isUpdatingAll, setIsUpdatingAll] = useState(false);

  // Convert a ghost skill into a synthetic Skill for DetailPanel
  const handleGhostClick = useCallback((ghost: RepoNewSkill) => {
    const syntheticSkill: Skill = {
      name: ghost.skill_id,
      description: ghost.description,
      skill_type: "hub",
      stars: 0,
      installed: false,
      update_available: false,
      last_updated: new Date().toISOString(),
      git_url: ghost.repo_url,
      tree_hash: null,
      category: "None",
      author: null,
      topics: [],
      source: ghost.repo_source,
    };
    setSelectedSkill((prev) => (prev?.name === syntheticSkill.name ? null : syntheticSkill));
  }, []);

  // Fetch broken skill count after skills load (lightweight, one extra field from StorageOverview)
  useEffect(() => {
    if (!loading) {
      let cancelled = false;
      tauriInvoke("get_storage_overview")
        .then((overview) => {
          if (!cancelled) setBrokenCount(overview.broken_count);
        })
        .catch((e) => {
          if (import.meta.env.DEV) console.warn("[MySkills] Failed to get storage overview:", e);
        });
      return () => {
        cancelled = true;
      };
    }
  }, [loading]);

  // Auto-focus a skill when navigating from Projects page
  useEffect(() => {
    if (initialFocusSkill && skills.length > 0) {
      const skill = skills.find((s) => s.name === initialFocusSkill);
      if (skill) setSelectedSkill(skill);
      onClearFocus?.();
    }
  }, [initialFocusSkill, skills, onClearFocus]);

  // Auto-open import modal when clipboard share code is detected
  useEffect(() => {
    if (initialShareCode) {
      setImportModalOpen(true);
    }
  }, [initialShareCode]);

  const remoteConnId = useMemo(() => {
    const h = remoteHosts.selectedHost;
    if (!h) return null;
    return h.source === "managed" ? h.id : `system:${h.alias}`;
  }, [remoteHosts.selectedHost]);

  const remoteDiscovery = useDiscoverRemoteSkillsQuery(
    remoteConnId ?? "",
    skillsScope === "remote" && remoteConnId != null,
  );

  const remoteAgentProfiles = useMemo(() => {
    const agents = remoteDiscovery.data?.agents ?? [];
    return agents.map((a) => remoteAgentProfile(a.agent, profiles));
  }, [remoteDiscovery.data?.agents, profiles]);

  const filteredSkills = useMemo(() => {
    let visibleSkills = [...skills];

    if (searchQuery) {
      const normalizedQuery = searchQuery.toLowerCase();
      visibleSkills = visibleSkills.filter(
        (skill) =>
          skill.name.toLowerCase().includes(normalizedQuery) ||
          skill.description.toLowerCase().includes(normalizedQuery) ||
          (skill.localized_description && skill.localized_description.toLowerCase().includes(normalizedQuery)),
      );
    }

    // Agent filter: only show skills linked to the selected agent
    if (agentFilter) {
      const agentProfile = profiles.find((p) => p.id === agentFilter);
      if (agentProfile) {
        visibleSkills = visibleSkills.filter((skill) => skill.agent_links?.includes(agentProfile.display_name));
      }
    }

    // Source type filter: hub / local
    if (sourceFilter === "hub") {
      visibleSkills = visibleSkills.filter((skill) => skill.skill_type !== "local");
    } else if (sourceFilter === "local") {
      visibleSkills = visibleSkills.filter((skill) => skill.skill_type === "local");
    }

    if (showUpdateOnly) {
      visibleSkills = visibleSkills.filter((skill) => skill.update_available);
    }

    // Repo source filter
    if (repoFilter) {
      visibleSkills = visibleSkills.filter((skill) => skill.source === repoFilter);
    }

    visibleSkills.sort((a, b) => {
      switch (sortBy) {
        case "stars-desc":
          return b.stars - a.stars || a.name.localeCompare(b.name);
        case "updated":
          return (
            new Date(b.last_updated).getTime() - new Date(a.last_updated).getTime() || a.name.localeCompare(b.name)
          );
        default:
          return a.name.localeCompare(b.name);
      }
    });

    return visibleSkills;
  }, [skills, searchQuery, sortBy, agentFilter, profiles, showUpdateOnly, sourceFilter, repoFilter]);

  const compatibleSelectionProfiles = useMemo(
    () => profiles.filter((profile) => profile.enabled && profile.id !== "openclaw"),
    [profiles],
  );

  // Stable reference for the per-card agent toggles — a fresh `profiles.filter(...)`
  // array in JSX would defeat SkillCard's React.memo and re-render every card.
  const enabledProfiles = useMemo(() => profiles.filter((profile) => profile.enabled), [profiles]);

  const handleInstall = async (url: string) => {
    try {
      await installSkill(url);
    } catch (e) {
      if (import.meta.env.DEV) console.error("[MySkills] installSkill failed:", e);
      toast.error(t("mySkills.installFailed"));
      throw e;
    }
  };

  const handleUpdate = async (name: string) => {
    try {
      const updated = await updateSkill(name);
      if (selectedSkill?.name === name) {
        setSelectedSkill(updated);
      }
    } catch (e) {
      const reason = e instanceof Error ? e.message : String(e);
      toast.error(reason ? `${t("mySkills.updateFailed")}: ${reason}` : t("mySkills.updateFailed"));
    }
  };

  const handleSelectSkill = useCallback((name: string) => {
    setSelectedSkillNames((prev) => {
      const next = new Set(prev);
      if (next.has(name)) {
        next.delete(name);
      } else {
        next.add(name);
      }
      return next;
    });
  }, []);

  const clearSelection = () => setSelectedSkillNames(new Set());

  const handleSelectAll = useCallback(() => {
    setSelectedSkillNames(new Set(filteredSkills.map((skill) => skill.name)));
  }, [filteredSkills]);

  const hasSelection = selectedSkillNames.size > 0;
  const [batchLoading, setBatchLoading] = useState(false);
  const [linkMenuOpen, setLinkMenuOpen] = useState(false);

  const removeSkillFromUi = useCallback((name: string) => {
    setSelectedSkill((current) => (current?.name === name ? null : current));
    setSelectedSkillNames((prev) => {
      const next = new Set(prev);
      next.delete(name);
      return next;
    });
  }, []);

  const openUninstallDialog = useCallback((names: Iterable<string>) => {
    const nextNames = Array.from(new Set(names));
    if (nextNames.length === 0) return;
    setPendingUninstallNames(nextNames);
    setUninstallError(null);
    setUninstallDialogOpen(true);
  }, []);

  const closeUninstallDialog = useCallback(() => {
    if (uninstalling) return;
    setPendingUninstallNames([]);
    setUninstallError(null);
    setUninstallDialogOpen(false);
  }, [uninstalling]);

  const handleUninstall = useCallback(
    (name: string) => {
      openUninstallDialog([name]);
    },
    [openUninstallDialog],
  );

  const handleBatchUninstall = useCallback(() => {
    openUninstallDialog(selectedSkillNames);
  }, [openUninstallDialog, selectedSkillNames]);

  const confirmUninstall = useCallback(async () => {
    if (pendingUninstallNames.length === 0) return;

    setUninstalling(true);
    const failedNames: string[] = [];

    for (const name of pendingUninstallNames) {
      try {
        await uninstallSkill(name);
        removeSkillFromUi(name);
      } catch (e) {
        failedNames.push(name);
        toast.error(t("mySkills.batchUninstallFailed", { name, count: 1 }));
      }
    }

    setUninstalling(false);

    if (failedNames.length === 0) {
      closeUninstallDialog();
      return;
    }

    setPendingUninstallNames(failedNames);
    setUninstallError(
      failedNames.length === 1
        ? t("mySkills.batchUninstallFailed", { name: failedNames[0], count: 1 })
        : t("mySkills.batchUninstallFailed", { name: failedNames[0], count: failedNames.length }),
    );
  }, [closeUninstallDialog, pendingUninstallNames, removeSkillFromUi, uninstallSkill, t]);

  const handleBatchUpdate = async () => {
    // Only update skills that actually have updates available (skip local skills)
    const updatableNames = Array.from(selectedSkillNames).filter((name) => {
      const skill = skills.find((s) => s.name === name);
      return skill?.update_available && skill.skill_type !== "local";
    });

    if (updatableNames.length === 0) {
      toast.info(t("mySkills.noUpdates"));
      return;
    }

    setBatchLoading(true);
    let successCount = 0;
    const failedNames: string[] = [];
    const errors: string[] = [];

    for (const name of updatableNames) {
      try {
        await updateSkill(name);
        successCount++;
      } catch (e) {
        failedNames.push(name);
        errors.push(e instanceof Error ? e.message : String(e));
      }
    }

    clearSelection();
    setBatchLoading(false);

    // Summary toast
    if (failedNames.length === 0) {
      toast.success(
        t("mySkills.batchUpdateSuccess", { count: successCount, defaultValue: `${successCount} skill(s) updated` }),
      );
    } else if (successCount > 0) {
      toast.warning(
        t("mySkills.batchUpdatePartial", {
          success: successCount,
          failed: failedNames.length,
          defaultValue: `${successCount} updated, ${failedNames.length} failed`,
        }),
      );
    } else {
      const reason = errors[0];
      toast.error(reason ? `${t("mySkills.updateFailed")}: ${reason}` : t("mySkills.updateFailed"));
    }
  };

  const handleUpdateAll = async () => {
    const allUpdatable = skills.filter((s) => s.update_available && s.skill_type !== "local");
    if (allUpdatable.length === 0) {
      toast.info(t("mySkills.noUpdates"));
      return;
    }

    setIsUpdatingAll(true);

    // Group by git_url — same repo only needs one update (backend clears siblings)
    const repoGroups = new Map<string, typeof allUpdatable>();
    for (const skill of allUpdatable) {
      const key = skill.git_url || skill.name; // fallback for non-repo skills
      const group = repoGroups.get(key) ?? [];
      group.push(skill);
      repoGroups.set(key, group);
    }

    // Update one representative per repo concurrently
    const tasks = Array.from(repoGroups.values()).map(async (group) => {
      const representative = group[0];
      try {
        const updated = await updateSkill(
          representative.name,
          group.map((s) => s.name),
        );
        if (selectedSkill?.name === representative.name) {
          setSelectedSkill(updated);
        }
        return { success: group.length, failed: [] as string[], error: null as string | null };
      } catch (e) {
        return { success: 0, failed: group.map((s) => s.name), error: e instanceof Error ? e.message : String(e) };
      }
    });

    const results = await Promise.allSettled(tasks);

    let successCount = 0;
    const failedNames: string[] = [];
    const errors: string[] = [];
    for (const r of results) {
      if (r.status === "fulfilled") {
        successCount += r.value.success;
        failedNames.push(...r.value.failed);
        if (r.value.error) errors.push(r.value.error);
      }
    }

    setIsUpdatingAll(false);

    if (failedNames.length === 0) {
      toast.success(
        t("mySkills.batchUpdateSuccess", { count: successCount, defaultValue: `${successCount} skill(s) updated` }),
      );
    } else if (successCount > 0) {
      toast.warning(
        t("mySkills.batchUpdatePartial", {
          success: successCount,
          failed: failedNames.length,
          defaultValue: `${successCount} updated, ${failedNames.length} failed`,
        }),
      );
    } else {
      const reason = errors[0];
      toast.error(reason ? `${t("mySkills.updateFailed")}: ${reason}` : t("mySkills.updateFailed"));
    }
  };

  const handleBatchLink = useCallback(
    async (agentId: string) => {
      setBatchLoading(true);
      try {
        const linked = await tauriInvoke("batch_link_skills_to_agent", {
          skillNames: Array.from(selectedSkillNames),
          agentId,
        });
        clearSelection();
        await refresh(true, true);
        if (linked === 0) {
          toast.warning(
            t("mySkills.batchLinkNone", {
              count: selectedSkillNames.size,
              defaultValue: `No skills were linked (${selectedSkillNames.size} skill(s) not found in hub)`,
            }),
          );
        } else {
          toast.success(
            t("mySkills.batchLinkSuccess", {
              count: linked,
              defaultValue: `${linked} skill(s) linked`,
            }),
          );
        }
      } catch (e) {
        toast.error(String(e) || t("mySkills.batchLinkFailed"));
      } finally {
        setBatchLoading(false);
      }
    },
    [selectedSkillNames, clearSelection, refresh, t],
  );

  const handleBatchUnlinkAll = useCallback(async () => {
    setBatchLoading(true);
    try {
      await batchRemoveSkillsFromAllAgents(Array.from(selectedSkillNames));
      clearSelection();
      toast.success(t("mySkills.batchUnlinkedAll", { defaultValue: "Unlinked from all agents" }));
    } catch (e) {
      toast.error(t("mySkills.batchUnlinkFailed", { defaultValue: "Failed to unlink skills" }));
    } finally {
      setBatchLoading(false);
    }
  }, [selectedSkillNames, batchRemoveSkillsFromAllAgents, clearSelection, t]);

  // Contextual single-letter shortcuts active only while skills are selected.
  useSkillsSelectionShortcuts({
    hasSelection,
    disabled: batchLoading || uninstalling,
    linkMenuOpen,
    onClear: clearSelection,
    onSelectAll: handleSelectAll,
    onToggleLinkMenu: () => setLinkMenuOpen((v) => !v),
    onCloseLinkMenu: () => setLinkMenuOpen(false),
    onUnlinkAll: handleBatchUnlinkAll,
    onDeploy: () => setDeployModalOpen(true),
    onUninstall: handleBatchUninstall,
  });

  // One-time hint when the user first enters selection mode.
  useEffect(() => {
    if (!hasSelection) return;
    if (typeof localStorage === "undefined") return;
    if (localStorage.getItem("skillstar.selectionShortcutsHinted")) return;
    localStorage.setItem("skillstar.selectionShortcutsHinted", "1");
    toast.info(
      t("mySkills.selectionShortcutsHint", {
        defaultValue: "Selection mode: A select all · L link · U unlink · Enter deploy · Esc clear",
      }),
    );
  }, [hasSelection, t]);

  const getEmptyMessage = () => {
    if (skills.length === 0) return t("emptyState.mySkillsDesc");
    if (showUpdateOnly) return t("mySkills.noUpdates");
    return t("mySkills.noMatching");
  };

  const getEmptyAction = () => {
    if (skills.length === 0) {
      return (
        <Button
          onClick={() => {
            window.dispatchEvent(new CustomEvent("skillstar:navigate", { detail: { page: "marketplace" } }));
          }}
          className="gap-2"
        >
          <Globe className="w-4 h-4" />
          {t("emptyState.mySkillsCta")}
        </Button>
      );
    }
    return undefined;
  };

  return (
    <div className="relative flex min-h-0 min-w-0 flex-1 flex-col overflow-hidden">
      <div className="flex min-w-0 flex-1 flex-col overflow-hidden">
        <Toolbar
          titleNode={
            <div className="flex flex-wrap items-center gap-3">
              <h1>{t("sidebar.skills")}</h1>
              <MySkillsScopeSwitch
                scope={skillsScope}
                onScopeChange={(next) => {
                  setSkillsScope(next);
                  localStorage.setItem(SCOPE_STORAGE_KEY, next);
                  if (next === "remote") {
                    setSelectedSkill(null);
                    setSelectedSkillNames(new Set());
                    setAgentFilter(null);
                  } else {
                    setAgentFilter(null);
                    setRemoteUi(null);
                  }
                }}
              />
            </div>
          }
          searchQuery={searchQuery}
          onSearchChange={setSearchQuery}
          sortBy={sortBy}
          onSortChange={setSortBy}
          viewMode={viewMode}
          onViewModeChange={setViewMode}
          countText={
            skillsScope === "local" ? (
              <div className="flex items-center gap-1.5 font-medium">
                <Layers className="w-3 h-3 hover:text-muted-foreground/90 transition-colors" />
                <span>{filteredSkills.length}</span>
              </div>
            ) : (
              <div className="flex items-center gap-1.5 font-medium">
                <Layers className="w-3 h-3 text-muted-foreground" />
                <span>{remoteUi?.visibleCount ?? 0}</span>
              </div>
            )
          }
          hideStarsSort={true}
          onRepoFilterChange={skillsScope === "local" ? setRepoFilter : undefined}
          hideSortControls={skillsScope === "remote"}
          hideViewToggle={false}
          filtersLead={
            skillsScope === "remote" ? (
              <MySkillsRemoteHostPicker
                hosts={remoteHosts.hosts}
                isLoading={remoteHosts.isLoadingHosts}
                selectedKey={remoteHosts.selectedKey}
                onSelect={remoteHosts.selectHost}
                onAdd={remoteHosts.openAddHost}
                onEdit={remoteHosts.openEditHost}
                onDelete={remoteHosts.deleteHost}
                onImport={remoteHosts.handleImportSystemHost}
              />
            ) : undefined
          }
          actionsLead={
            skillsScope === "remote" ? (
              <>
                <RemoteConnectionLogPopover
                  open={remoteConsoleOpen}
                  onOpenChange={setRemoteConsoleOpen}
                  lines={remoteUi?.connectLines ?? []}
                  pendingHostKey={remoteUi?.pendingHostKey ?? null}
                  active={Boolean(remoteUi?.connectActive)}
                  attention={Boolean(remoteUi?.connectAttention)}
                  onAcceptHostKey={async (fp) => {
                    await remoteUi?.acceptHostKey?.(fp);
                  }}
                  onRejectHostKey={() => remoteUi?.rejectHostKey?.()}
                />
                <button
                  type="button"
                  onClick={() => setRemotePushOpen(true)}
                  className="flex h-8 items-center gap-1.5 rounded-lg border border-border/80 bg-background/50 px-3 text-xs font-medium text-foreground/80 hover:bg-accent/10 shrink-0 focus-ring"
                >
                  <Upload className="size-3.5" />
                  {t("ssh.push")}
                </button>
              </>
            ) : undefined
          }
          agentProfiles={skillsScope === "remote" ? remoteAgentProfiles : profiles}
          agentFilter={agentFilter}
          onAgentFilterChange={setAgentFilter}
          onImport={skillsScope === "local" ? () => setImportModalOpen(true) : undefined}
          onRefresh={skillsScope === "remote" ? () => remoteUi?.refetch() : () => refresh(false, true)}
          isRefreshing={skillsScope === "remote" ? Boolean(remoteUi?.isFetching) : loading}
          onAiPick={skillsScope === "local" ? () => setAiPickModalOpen(true) : undefined}
          sourceFilter={skillsScope === "local" ? sourceFilter : undefined}
          onSourceFilterChange={
            skillsScope === "local"
              ? (f) => {
                  setSourceFilter(f);
                  if (f === "local") setRepoFilter(null);
                }
              : undefined
          }
          localCount={skillsScope === "local" ? localCount : undefined}
          onUpdateAll={skillsScope === "local" ? handleUpdateAll : undefined}
          isUpdatingAll={skillsScope === "local" ? isUpdatingAll : undefined}
          repoSources={skillsScope === "local" ? repoSources : undefined}
          repoFilter={skillsScope === "local" ? repoFilter : undefined}
          showUpdateOnly={skillsScope === "local" ? showUpdateOnly : undefined}
          onToggleUpdateOnly={skillsScope === "local" ? () => setShowUpdateOnly((prev) => !prev) : undefined}
          pendingUpdateCount={skillsScope === "local" ? pendingUpdateCount : undefined}
        />

        {/* Selection bar */}
        <AnimatePresence>
          {skillsScope === "local" && hasSelection && (
            <SkillSelectionBar
              selectedCount={selectedSkillNames.size}
              totalCount={filteredSkills.length}
              disabled={batchLoading || uninstalling}
              onDeploy={() => setDeployModalOpen(true)}
              onSaveGroup={onPackSkills ? undefined : () => setGroupModalOpen(true)}
              onPackSkills={onPackSkills ? () => onPackSkills(Array.from(selectedSkillNames)) : undefined}
              onShare={() => setShareCardSkills(Array.from(selectedSkillNames))}
              onUpdate={handleBatchUpdate}
              onUninstall={handleBatchUninstall}
              onSelectAll={handleSelectAll}
              onClear={clearSelection}
              linkMenuOpen={linkMenuOpen}
              onLinkMenuOpenChange={setLinkMenuOpen}
              agentProfiles={compatibleSelectionProfiles}
              onBatchLink={handleBatchLink}
              onBatchUnlinkAll={handleBatchUnlinkAll}
              onBatchAiProcess={async () => {
                try {
                  setBatchLoading(true);
                  await batchAiProcessSkills(Array.from(selectedSkillNames));
                  clearSelection();
                } catch (e) {
                  toast.error(t("mySkills.batchAiError", { defaultValue: "Failed to start AI processing" }));
                } finally {
                  setBatchLoading(false);
                }
              }}
            />
          )}
        </AnimatePresence>

        <ExportShareCodeModal
          open={!!shareCardSkills && shareCardSkills.length > 0}
          onClose={() => setShareCardSkills(null)}
          skillNames={shareCardSkills || undefined}
          hubSkills={skills}
          onPublishSkill={(name) => setPublishTarget(name)}
        />

        {/* Broken skills banner */}
        <AnimatePresence>
          {skillsScope === "local" && brokenCount > 0 && (
            <motion.div
              initial={{ height: 0, opacity: 0 }}
              animate={{ height: "auto", opacity: 1 }}
              exit={{ height: 0, opacity: 0 }}
              transition={{ duration: 0.2 }}
              className="overflow-hidden"
            >
              <div className="flex items-center gap-2.5 px-6 py-2 bg-amber-500/8 border-b border-amber-500/20">
                <AlertTriangle className="w-3.5 h-3.5 text-amber-400 shrink-0" />
                <span className="text-caption text-amber-300/90">
                  {t("mySkills.brokenBanner", { count: brokenCount })}
                </span>
                <button
                  type="button"
                  onClick={() => {
                    navigateToSettingsSection("storage");
                  }}
                  className="text-caption text-amber-400 hover:text-amber-300 font-medium ml-auto cursor-pointer transition-colors"
                >
                  {t("mySkills.brokenBannerAction")} →
                </button>
              </div>
            </motion.div>
          )}
        </AnimatePresence>

        {skillsScope === "remote" ? (
          remoteHosts.isLoadingHosts ? (
            <div className="flex flex-1 items-center justify-center py-20">
              <LoadingLogo size="lg" label={t("mySkills.loading")} />
            </div>
          ) : !remoteHosts.hosts?.length ? (
            <div className="flex flex-1 flex-col items-center justify-center px-6 py-16">
              <EmptyState
                icon={<Server className="size-6 text-muted-foreground" />}
                title={t("ssh.noHosts")}
                description={t("ssh.noHostsHint")}
                action={
                  <Button type="button" onClick={remoteHosts.openAddHost}>
                    {t("ssh.addHost")}
                  </Button>
                }
              />
              <SshHostForm
                open={remoteHosts.formOpen}
                onOpenChange={remoteHosts.setFormOpen}
                initial={remoteHosts.editing}
                onSubmit={remoteHosts.handleHostFormSubmit}
              />
            </div>
          ) : remoteHosts.selectedHost ? (
            <RemoteSkillsContent
              host={remoteHosts.selectedHost}
              searchQuery={searchQuery}
              viewMode={viewMode}
              agentFilter={agentFilter}
              pushOpen={remotePushOpen}
              onPushOpenChange={setRemotePushOpen}
              onDiscoveryUiChange={setRemoteUi}
            />
          ) : (
            <div className="flex flex-1 items-center justify-center text-sm text-muted-foreground">
              {t("ssh.selectHost")}
            </div>
          )
        ) : (
          <motion.main
            initial={{ opacity: 0 }}
            animate={{ opacity: 1 }}
            transition={{ duration: 0.2 }}
            className="ss-page-scroll"
          >
            {loading ? (
              <div className="flex items-center justify-center py-20">
                <LoadingLogo size="lg" label={t("mySkills.loading")} />
              </div>
            ) : (
              <SkillGrid
                skills={filteredSkills}
                viewMode={viewMode}
                columnStrategy="auto-fill"
                minColumnWidth={320}
                onSkillClick={(skill) => setSelectedSkill((prev) => (prev?.name === skill.name ? null : skill))}
                onInstall={handleInstall}
                onUpdate={handleUpdate}
                emptyMessage={getEmptyMessage()}
                emptyAction={getEmptyAction()}
                selectable
                selectedSkills={selectedSkillNames}
                onSelectSkill={handleSelectSkill}
                profiles={enabledProfiles}
                onToggleAgent={toggleSkillForAgent}
                pendingUpdateNames={pendingUpdateNames}
                pendingAgentToggleKeys={pendingAgentToggleKeys}
                ghostSkills={
                  !showUpdateOnly && !searchQuery && !agentFilter && sourceFilter === "all" && !repoFilter
                    ? ghostSkills
                    : undefined
                }
                onInstallGhost={installGhostSkill}
                onDismissGhost={dismissGhostSkill}
                onDismissGhostRepo={dismissGhostRepo}
                onGhostClick={handleGhostClick}
              />
            )}
          </motion.main>
        )}
      </div>

      {skillsScope === "local" && selectedSkill && (
        <Suspense
          fallback={
            <div className="absolute right-0 top-0 bottom-0 z-50 flex h-full w-full max-w-md items-center justify-center overflow-y-auto border-l border-border/45 bg-background/30 shadow-[0_24px_80px_-52px_var(--color-shadow)] backdrop-blur-xl">
              <LoadingLogo size="sm" />
            </div>
          }
        >
          <DetailPanel
            skill={selectedSkill}
            onClose={() => setSelectedSkill(null)}
            onInstall={handleInstall}
            onUpdate={handleUpdate}
            onUninstall={handleUninstall}
            uninstalling={uninstalling && pendingUninstallNames.includes(selectedSkill.name)}
            onReadContent={readSkillContent}
            onSaveContent={updateSkillContent}
            onPublish={(name) => setPublishTarget(name)}
          />
        </Suspense>
      )}

      <DeployToProjectModal
        open={deployModalOpen}
        onClose={() => setDeployModalOpen(false)}
        selectedSkills={Array.from(selectedSkillNames)}
        profiles={compatibleSelectionProfiles}
        onDeploy={deploySkillsToProject}
      />

      <CreateGroupModal
        open={groupModalOpen}
        onClose={() => {
          setGroupModalOpen(false);
          setQuickPackSkills([]);
        }}
        availableSkills={skills}
        existingNames={groups.map((g) => g.name)}
        initialSkills={quickPackSkills.length > 0 ? quickPackSkills : Array.from(selectedSkillNames)}
        onSave={async (name, description, icon, skillList) => {
          await createGroup(name, description, icon, skillList);
          clearSelection();
          setQuickPackSkills([]);
        }}
      />

      <UninstallConfirmDialog
        open={uninstallDialogOpen}
        skillNames={pendingUninstallNames}
        uninstalling={uninstalling}
        error={uninstallError}
        onClose={closeUninstallDialog}
        onConfirm={confirmUninstall}
      />

      <ImportModal
        open={importModalOpen}
        onClose={() => setImportModalOpen(false)}
        onInstalled={() => {
          void refresh(false, true);
        }}
        onPickLocalFile={() => {
          setImportModalOpen(false);
          setImportBundleOpen(true);
        }}
        onPackGroup={(names: string[]) => {
          setImportModalOpen(false);
          setQuickPackSkills(names);
          setGroupModalOpen(true);
        }}
        initialShareCode={initialShareCode}
        onClearShareCode={onClearShareCode}
      />

      <ImportBundleModal
        open={importBundleOpen}
        onClose={() => setImportBundleOpen(false)}
        onImported={() => {
          void refresh(false, true);
        }}
      />

      <PublishSkillModal
        open={!!publishTarget}
        onClose={() => setPublishTarget(null)}
        skillName={publishTarget || ""}
        skillDescription={skills.find((s) => s.name === publishTarget)?.description || ""}
        onPublished={() => {
          setPublishTarget(null);
          refresh(false, true);
        }}
      />

      {skillsScope === "remote" && (remoteHosts.hosts?.length ?? 0) > 0 ? (
        <SshHostForm
          open={remoteHosts.formOpen}
          onOpenChange={remoteHosts.setFormOpen}
          initial={remoteHosts.editing}
          onSubmit={remoteHosts.handleHostFormSubmit}
        />
      ) : null}

      <AiPickSkillsModal
        open={aiPickModalOpen}
        onClose={() => setAiPickModalOpen(false)}
        skills={skills}
        onResult={(names) => {
          setSelectedSkillNames(new Set(names));
        }}
      />
    </div>
  );
}
