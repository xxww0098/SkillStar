import { motion } from "framer-motion";
import { Globe, Layers } from "lucide-react";
import { type ReactNode, useCallback, useEffect, useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { Button } from "../../../components/ui/button";
import { LoadingLogo } from "../../../components/ui/LoadingLogo";
import { useAgentProfiles } from "../../../hooks/useAgentProfiles";
import { useSkillsSelectionShortcuts } from "../../../hooks/useSkillsSelectionShortcuts";
import { useViewMode } from "../../../hooks/useViewMode";
import { selectTargetableAgentProfiles, supportsGlobalDeploy, supportsProjectDeploy } from "../../../lib/agentProfiles";
import { tauriInvoke } from "../../../lib/ipc";
import { toast } from "../../../lib/toast";
import { Toolbar } from "../../../components/layout/Toolbar";
import type { Skill, SkillUpdateReport, SortOption } from "../../../types";
import { useSkillCards } from "../hooks/useSkillCards";
import { useSkills } from "../hooks/useSkills";
import { hasPendingUpdate, needsAttention } from "../lib/pendingUpdates";
import { CreateGroupModal } from "./CreateGroupModal";
import { DeployToProjectModal } from "./DeployToProjectModal";
import { ExportShareCodeModal } from "./ExportShareCodeModal";
import { ImportBundleModal } from "./ImportBundleModal";
import { ImportModal } from "./ImportModal";
import { PublishSkillModal } from "./PublishSkillModal";
import { ScopeDetailDrawer } from "./ScopeDetailDrawer";
import { SkillGrid } from "./SkillGrid";
import { SkillListBanners } from "./SkillListBanners";
import { SkillSelectionBar } from "./SkillSelectionBar";
import { UninstallConfirmDialog } from "./UninstallConfirmDialog";

type SkillSelection = { kind: "installed"; name: string };

interface LocalSkillsContentProps {
  /** Scope switch element built by the page; rendered inside the toolbar title. */
  scopeSwitch: ReactNode;
  initialFocusSkill?: string | null;
  onClearFocus?: () => void;
  onPackSkills?: (skills: string[]) => void;
  /** Pre-filled share code from clipboard auto-detect */
  initialShareCode?: string;
  /** Clear consumed share code */
  onClearShareCode?: () => void;
}

/**
 * Local (hub + filesystem) skill workspace. Self-contained: owns its own toolbar
 * (concrete callbacks, zero scope conditionals), selection/batch state, detail
 * drawer, and modals. Twin of {@link import("../remote/RemoteSkillsContent").RemoteSkillsContent}.
 */
export function LocalSkillsContent({
  scopeSwitch,
  initialFocusSkill,
  onClearFocus,
  onPackSkills,
  initialShareCode,
  onClearShareCode,
}: LocalSkillsContentProps) {
  const { t } = useTranslation();
  const {
    skills,
    loading,
    refresh,
    installSkill,
    reinstallSkill,
    reinstallRepoSkills,
    uninstallSkill,
    runSkillUpdate,
    pendingUpdateNames,
    toggleSkillForAgent,
    pendingAgentToggleKeys,
    readSkillContent,
    updateSkillContent,
    batchRemoveSkillsFromAllAgents,
  } = useSkills();
  const { profiles, deploySkillsToProject } = useAgentProfiles();
  const { createGroup, groups } = useSkillCards();

  const [searchQuery, setSearchQuery] = useState("");
  const [sortBy, setSortBy] = useState<SortOption>("updated");
  const [viewMode, setViewMode] = useViewMode("grid");
  const [agentFilter, setAgentFilter] = useState<string | null>(null);
  const [selection, setSelection] = useState<SkillSelection | null>(null);
  const [selectedSkillNames, setSelectedSkillNames] = useState<Set<string>>(new Set());
  const [quickPackSkills, setQuickPackSkills] = useState<string[]>([]);
  const [quickPackName, setQuickPackName] = useState("");
  const [deployModalOpen, setDeployModalOpen] = useState(false);
  const [groupModalOpen, setGroupModalOpen] = useState(false);
  const [uninstallDialogOpen, setUninstallDialogOpen] = useState(false);
  const [pendingUninstallNames, setPendingUninstallNames] = useState<string[]>([]);
  const [uninstalling, setUninstalling] = useState(false);
  const [uninstallError, setUninstallError] = useState<string | null>(null);
  const [importModalOpen, setImportModalOpen] = useState(false);
  const [importBundleOpen, setImportBundleOpen] = useState(false);
  const [publishTarget, setPublishTarget] = useState<string | null>(null);
  const [brokenCount, setBrokenCount] = useState(0);
  const [sourceFilter, setSourceFilter] = useState<"all" | "hub" | "local">("all");
  const [repoFilter, setRepoFilter] = useState<string | null>(null);
  const [shareCardSkills, setShareCardSkills] = useState<string[] | null>(null);
  const [onlyUpdatesFilter, setOnlyUpdatesFilter] = useState(false);
  const [isUpdatingAll, setIsUpdatingAll] = useState(false);
  const [reinstallingRepoSource, setReinstallingRepoSource] = useState<string | null>(null);
  const [reinstallingName, setReinstallingName] = useState<string | null>(null);
  const [batchLoading, setBatchLoading] = useState(false);
  const [linkMenuOpen, setLinkMenuOpen] = useState(false);

  // The query cache owns Skill data; async completions never restore an old selection.
  const selectedSkill = useMemo(() => {
    if (!selection) return null;
    return skills.find((skill) => skill.name === selection.name) ?? null;
  }, [selection, skills]);

  const closeSkillIfSelected = useCallback((name: string) => {
    setSelection((current) => (current?.kind === "installed" && current.name === name ? null : current));
  }, []);

  const localCount = useMemo(() => skills.filter((s) => s.skill_type === "local").length, [skills]);

  /** Sorted unique repo source strings for the repo filter popover */
  const repoSources = useMemo(() => {
    const set = new Set<string>();
    for (const skill of skills) {
      if (skill.source) set.add(skill.source);
    }
    return Array.from(set).sort((a, b) => a.localeCompare(b));
  }, [skills]);

  // Fetch broken skill count after skills load (lightweight, one extra field from StorageOverview)
  useEffect(() => {
    if (!loading) {
      let cancelled = false;
      tauriInvoke("get_storage_overview")
        .then((overview) => {
          if (!cancelled) setBrokenCount(overview.broken_count);
        })
        .catch((e) => {
          if (import.meta.env.DEV) console.warn("[LocalSkills] Failed to get storage overview:", e);
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
      if (skill) setSelection({ kind: "installed", name: skill.name });
      onClearFocus?.();
    }
  }, [initialFocusSkill, skills, onClearFocus]);

  // Auto-open import modal when clipboard share code is detected
  useEffect(() => {
    if (initialShareCode) {
      setImportModalOpen(true);
    }
  }, [initialShareCode]);

  /** Everything the toolbar filters narrow to, before the updates toggle. */
  const scopedSkills = useMemo(() => {
    let visibleSkills = [...skills];

    if (searchQuery) {
      const normalizedQuery = searchQuery.toLowerCase();
      visibleSkills = visibleSkills.filter(
        (skill) =>
          skill.name.toLowerCase().includes(normalizedQuery) ||
          skill.description.toLowerCase().includes(normalizedQuery) ||
          (skill.localized_description && skill.localized_description.toLowerCase().includes(normalizedQuery)) ||
          (skill.source && skill.source.toLowerCase().includes(normalizedQuery)),
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

    if (repoFilter) {
      visibleSkills = visibleSkills.filter((skill) => skill.source === repoFilter);
    }

    return visibleSkills;
  }, [skills, searchQuery, agentFilter, profiles, sourceFilter, repoFilter]);

  /** The attention filter covers everything the sidebar badge counts — content
   *  updates plus removed upstreams — so the chip can never show 0 while the
   *  badge promises attention. Removed skills explain themselves with the
   *  card's removed chip. The "update all" CTA stays deliberately
   *  content-only and filter-independent (see docs/features/skills/README.md),
   *  so it reads the unfiltered list. */
  const attentionSkills = useMemo(() => scopedSkills.filter(needsAttention), [scopedSkills]);
  const allAttentionSkills = useMemo(() => skills.filter(needsAttention), [skills]);
  const allPendingUpdates = useMemo(() => skills.filter(hasPendingUpdate), [skills]);

  const filteredSkills = useMemo(() => {
    const visibleSkills = [...(onlyUpdatesFilter ? attentionSkills : scopedSkills)];

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
  }, [scopedSkills, attentionSkills, onlyUpdatesFilter, sortBy]);

  // Stable Settings-backed target list for filters, cards, selection actions,
  // and project deployment. Persisted `enabled` alone is insufficient because
  // it may remain true after an Agent is uninstalled.
  const targetableProfiles = useMemo(() => selectTargetableAgentProfiles(profiles), [profiles]);
  const enabledProfiles = useMemo(() => targetableProfiles.filter(supportsGlobalDeploy), [targetableProfiles]);
  const compatibleSelectionProfiles = useMemo(
    () => targetableProfiles.filter(supportsProjectDeploy),
    [targetableProfiles],
  );

  // Stable identities so SkillGrid/SkillCard memoization holds across
  // unrelated re-renders (e.g. every search-input keystroke).
  const handleInstall = useCallback(
    async (url: string, name?: string, agentId?: string) => {
      try {
        await installSkill(url, name, agentId);
      } catch (e) {
        if (import.meta.env.DEV) console.error("[LocalSkills] installSkill failed:", e);
        toast.error(t("mySkills.installFailed"));
        throw e;
      }
    },
    [installSkill, t],
  );

  const handleUpdate = useCallback(
    async (name: string) => {
      try {
        const report = await runSkillUpdate([name]);
        // Failures and upstream-removed skips are toasted by runSkillUpdate.
        // Declined by design rather than failed: without this the button would
        // simply do nothing, which reads as a bug.
        if (report.channel_managed.some((entry) => entry.name === name)) {
          toast.info(t("mySkills.updateChannelManaged", { name }));
        }
      } catch (e) {
        const reason = e instanceof Error ? e.message : String(e);
        toast.error(reason ? `${t("mySkills.updateFailed")}: ${reason}` : t("mySkills.updateFailed"));
      }
    },
    [runSkillUpdate, t],
  );

  const handleSkillClick = useCallback((skill: Skill) => {
    setSelection((current) =>
      current?.kind === "installed" && current.name === skill.name ? null : { kind: "installed", name: skill.name },
    );
  }, []);

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

  const removeSkillFromUi = useCallback(
    (name: string) => {
      closeSkillIfSelected(name);
      setSelectedSkillNames((prev) => {
        const next = new Set(prev);
        next.delete(name);
        return next;
      });
    },
    [closeSkillIfSelected],
  );

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

  /** Uninstall every installed skill that came from a given repo source. */
  const handleRemoveRepoSource = useCallback(
    (source: string) => {
      const names = skills.filter((skill) => skill.source === source).map((skill) => skill.name);
      if (names.length === 0) return;
      if (repoFilter === source) setRepoFilter(null);
      openUninstallDialog(names);
    },
    [openUninstallDialog, repoFilter, skills],
  );

  /** Reinstall the open Skill only — identity fail-closed, never the whole repo. */
  const handleReinstall = useCallback(
    async (url: string, name: string) => {
      if (reinstallingName) return;
      if (!url.trim()) {
        toast.error(t("mySkills.reinstallRepoSourceMissing", { source: name }));
        return;
      }
      setReinstallingName(name);
      try {
        await reinstallSkill(url, name);
        toast.success(t("mySkills.reinstallSuccess", { name }));
      } catch (e) {
        const reason = e instanceof Error ? e.message : String(e);
        toast.error(reason || t("mySkills.reinstallFailed"));
      } finally {
        setReinstallingName(null);
      }
    },
    [reinstallSkill, reinstallingName, t],
  );

  /** Reinstall every Skill found in exactly one GitHub repository source. */
  const handleReinstallRepoSource = useCallback(
    async (source: string) => {
      const repoUrl = skills.find((skill) => skill.source === source && skill.skill_type === "hub")?.git_url;
      if (!repoUrl) {
        toast.error(t("mySkills.reinstallRepoSourceMissing", { source }));
        return;
      }

      setReinstallingRepoSource(source);
      try {
        const installed = await reinstallRepoSkills(repoUrl);
        toast.success(t("mySkills.reinstallRepoSuccess", { count: installed.length }));
      } catch (e) {
        const reason = e instanceof Error ? e.message : String(e);
        const headline = t("mySkills.reinstallRepoFailed", { source });
        toast.error(reason ? `${headline}\n${reason}` : headline);
      } finally {
        setReinstallingRepoSource(null);
      }
    },
    [reinstallRepoSkills, skills, t],
  );

  const confirmUninstall = useCallback(async () => {
    if (pendingUninstallNames.length === 0) return;

    setUninstalling(true);
    const failedNames: string[] = [];

    for (const name of pendingUninstallNames) {
      try {
        await uninstallSkill(name);
        removeSkillFromUi(name);
      } catch {
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

  /** Summarise a finished batch update — the report is already final, so every
   *  Skill is counted exactly once. Failures and upstream-removed skips are
   *  toasted by runSkillUpdate; this covers the aggregate success count. */
  const reportBatchUpdate = useCallback(
    (report: SkillUpdateReport) => {
      if (report.failed.length > 0 && report.updated.length > 0) {
        toast.warning(
          t("mySkills.batchUpdatePartial", {
            success: report.updated.length,
            failed: report.failed.length,
            defaultValue: `${report.updated.length} updated, ${report.failed.length} failed`,
          }),
        );
      }

      if (report.channel_managed.length > 0) {
        toast.info(t("mySkills.batchUpdateChannelManaged", { count: report.channel_managed.length }));
      }

      if (report.updated.length > 0 && report.failed.length === 0) {
        toast.success(
          t("mySkills.batchUpdateSuccess", {
            count: report.updated.length,
            defaultValue: `${report.updated.length} skill(s) updated`,
          }),
        );
      }
    },
    [t],
  );

  /** Skills the user can actually pull: local ones have no remote to check. */
  const updatableNamesAmong = useCallback(
    (candidates: Iterable<string>) =>
      Array.from(candidates).filter((name) => {
        const skill = skills.find((item) => item.name === name);
        return Boolean(skill && hasPendingUpdate(skill));
      }),
    [skills],
  );

  const runBatchUpdate = useCallback(
    async (names: string[], setBusy: (busy: boolean) => void) => {
      if (names.length === 0) {
        toast.info(t("mySkills.noUpdates"));
        return true;
      }

      setBusy(true);
      try {
        const report = await runSkillUpdate(names);
        reportBatchUpdate(report);
        return true;
      } catch (error) {
        const reason = error instanceof Error ? error.message : String(error);
        toast.error(reason ? `${t("mySkills.updateFailed")}: ${reason}` : t("mySkills.updateFailed"));
        return false;
      } finally {
        setBusy(false);
      }
    },
    [reportBatchUpdate, runSkillUpdate, t],
  );

  const handleBatchUpdate = useCallback(async () => {
    if (await runBatchUpdate(updatableNamesAmong(selectedSkillNames), setBatchLoading)) {
      clearSelection();
    }
  }, [clearSelection, runBatchUpdate, selectedSkillNames, updatableNamesAmong]);

  const handleUpdateAll = useCallback(
    () =>
      runBatchUpdate(
        allPendingUpdates.map((skill) => skill.name),
        setIsUpdatingAll,
      ),
    [runBatchUpdate, allPendingUpdates],
  );

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
      toast.error(t("mySkills.batchUnlinkFailed", { defaultValue: "Couldn't unlink those cards. Try again." }));
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

  // One-time hint when the user first enters selection mode. Bottom-center so
  // it never covers the detail drawer's action buttons in the bottom-right.
  useEffect(() => {
    if (!hasSelection) return;
    if (typeof localStorage === "undefined") return;
    if (localStorage.getItem("skillstar.selectionShortcutsHinted")) return;
    localStorage.setItem("skillstar.selectionShortcutsHinted", "1");
    toast.info(
      t("mySkills.selectionShortcutsHint", {
        defaultValue: "Selection mode: A select all · L link · U unlink · Enter deploy · Esc clear",
      }),
      { position: "bottom-center", duration: 5000 },
    );
  }, [hasSelection, t]);

  const getEmptyMessage = () => {
    if (onlyUpdatesFilter) {
      // Attention exists but the active search/source filters hide all of it —
      // say where it went instead of claiming there is nothing to do.
      if (attentionSkills.length === 0 && allAttentionSkills.length > 0) {
        return t("toolbar.attentionOutsideFilters", { count: allAttentionSkills.length });
      }
      return t("toolbar.noPendingUpdates");
    }
    if (skills.length === 0) return t("emptyState.mySkillsDesc");
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
          sortBy={sortBy}
          onSortChange={setSortBy}
          viewMode={viewMode}
          onViewModeChange={setViewMode}
          countText={
            <div className="flex items-center gap-1.5 font-medium">
              <Layers className="w-3 h-3 hover:text-muted-foreground/90 transition-colors" />
              <span>{filteredSkills.length}</span>
            </div>
          }
          hideStarsSort={true}
          onRepoFilterChange={setRepoFilter}
          agentProfiles={enabledProfiles}
          agentFilter={agentFilter}
          onAgentFilterChange={setAgentFilter}
          onImport={() => setImportModalOpen(true)}
          onRefresh={() => refresh(false, true)}
          isRefreshing={loading}
          sourceFilter={sourceFilter}
          onSourceFilterChange={(f) => {
            setSourceFilter(f);
            if (f === "local") setRepoFilter(null);
          }}
          localCount={localCount}
          onUpdateAll={handleUpdateAll}
          isUpdatingAll={isUpdatingAll}
          repoSources={repoSources}
          repoFilter={repoFilter}
          onReinstallRepoSource={handleReinstallRepoSource}
          reinstallingRepoSource={reinstallingRepoSource}
          onRemoveRepoSource={handleRemoveRepoSource}
          pendingUpdateCount={allPendingUpdates.length}
          attentionCount={allAttentionSkills.length}
          filteredAttentionCount={attentionSkills.length}
          onlyUpdatesFilter={onlyUpdatesFilter}
          onOnlyUpdatesFilterChange={setOnlyUpdatesFilter}
        />

        {/* Selection bar — mounts and unmounts outright; it no longer animates
            its own height, so there is nothing left to play out on exit. */}
        {hasSelection && (
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
          />
        )}

        <ExportShareCodeModal
          open={!!shareCardSkills && shareCardSkills.length > 0}
          onClose={() => setShareCardSkills(null)}
          skillNames={shareCardSkills || undefined}
          hubSkills={skills}
          onPublishSkill={(name) => setPublishTarget(name)}
        />

        <SkillListBanners
          brokenCount={brokenCount}
          onlyUpdatesFilter={onlyUpdatesFilter}
          filteredCount={filteredSkills.length}
          onClearUpdatesFilter={() => setOnlyUpdatesFilter(false)}
        />

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
              onSkillClick={handleSkillClick}
              onInstall={handleInstall}
              onUpdate={handleUpdate}
              emptyMessage={getEmptyMessage()}
              emptyAction={getEmptyAction()}
              selectable
              selectedSkills={selectedSkillNames}
              onSelectSkill={handleSelectSkill}
              profiles={profiles}
              onToggleAgent={toggleSkillForAgent}
              pendingUpdateNames={pendingUpdateNames}
              pendingAgentToggleKeys={pendingAgentToggleKeys}
            />
          )}
        </motion.main>
      </div>

      <ScopeDetailDrawer
        kind="local"
        skill={selectedSkill}
        onClose={() => setSelection(null)}
        onInstall={handleInstall}
        onUpdate={handleUpdate}
        onUninstall={handleUninstall}
        uninstalling={uninstalling && selectedSkill != null && pendingUninstallNames.includes(selectedSkill.name)}
        onReinstall={handleReinstall}
        reinstalling={selectedSkill != null && reinstallingName === selectedSkill.name}
        onReadContent={readSkillContent}
        onSaveContent={updateSkillContent}
        onPublish={(name) => setPublishTarget(name)}
      />

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
          setQuickPackName("");
        }}
        availableSkills={skills}
        existingNames={groups.map((g) => g.name)}
        initialName={quickPackName}
        initialSkills={quickPackSkills.length > 0 ? quickPackSkills : Array.from(selectedSkillNames)}
        onSave={async (name, description, icon, skillList) => {
          await createGroup(name, description, icon, skillList);
          clearSelection();
          setQuickPackSkills([]);
          setQuickPackName("");
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
        onPackGroup={(names: string[], defaultName: string) => {
          setImportModalOpen(false);
          setQuickPackSkills(names);
          setQuickPackName(defaultName);
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
          // Keep the modal open on the success ("done") phase so the user can
          // see / copy the repo URL — closing here would skip it entirely.
          // The user dismisses via the footer button (→ onClose). Refresh in
          // the background so the now-published skill reflects its new state.
          refresh(false, true);
        }}
      />
    </>
  );
}
