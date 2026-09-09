import { motion } from "framer-motion";
import { Globe, Layers } from "lucide-react";
import { type ReactNode, useCallback, useEffect, useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { cn } from "../../../lib/utils";
import { Button } from "../../../components/ui/button";
import { LoadingLogo } from "../../../components/ui/LoadingLogo";
import { useAgentProfiles } from "../../../hooks/useAgentProfiles";
import { useSkillsSelectionShortcuts } from "../../../hooks/useSkillsSelectionShortcuts";
import { useViewMode } from "../../../hooks/useViewMode";
import { selectTargetableAgentProfiles, supportsGlobalDeploy, supportsProjectDeploy } from "../../../lib/agentProfiles";
import { tauriInvoke } from "../../../lib/ipc";
import { toast } from "../../../lib/toast";
import { Toolbar } from "../../../components/layout/Toolbar";
import type { RepoNewSkill, Skill, SkillUpdateRunReport, SortOption } from "../../../types";
import { useSkillCards } from "../hooks/useSkillCards";
import { useSkills } from "../hooks/useSkills";
import { hasPendingUpdate } from "../lib/pendingUpdates";
import { syntheticSkillFromGhost } from "../lib/ghostSkill";
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

type SkillSelection = { kind: "installed"; name: string } | { kind: "ghost"; ghost: RepoNewSkill };

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
    reinstallRepoSkills,
    uninstallSkill,
    runSkillUpdate,
    resolveRemovedSkill,
    migrateRenamedSkill,
    pendingMigrationNames,
    pendingUpdateNames,
    toggleSkillForAgent,
    pendingAgentToggleKeys,
    readSkillContent,
    updateSkillContent,
    batchRemoveSkillsFromAllAgents,
    ghostSkills,
    dismissGhostSkill,
    dismissGhostRepo,
    installGhostSkill,
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
  /** When set, a successful uninstall also dismisses ghost skills for this repo source. */
  const [pendingRemoveSource, setPendingRemoveSource] = useState<string | null>(null);
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
  const [batchLoading, setBatchLoading] = useState(false);
  const [linkMenuOpen, setLinkMenuOpen] = useState(false);

  // The query cache owns Skill data; async completions never restore an old selection.
  const selectedSkill = useMemo(() => {
    if (!selection) return null;
    if (selection.kind === "installed") return skills.find((skill) => skill.name === selection.name) ?? null;
    const { ghost } = selection;
    return (
      skills.find((skill) => skill.name === ghost.skill_id && skill.source === ghost.repo_source) ??
      syntheticSkillFromGhost(ghost)
    );
  }, [selection, skills]);

  const closeSkillIfSelected = useCallback((name: string) => {
    setSelection((current) => {
      const selectedName = current?.kind === "installed" ? current.name : current?.ghost.skill_id;
      return selectedName === name ? null : current;
    });
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

  /** Full skill universe for Spotlight (client-side title/description/source match). */
  const spotlightItems = useMemo(
    () =>
      skills.map((skill) => ({
        id: skill.name,
        title: skill.name,
        subtitle: skill.localized_description || skill.description || undefined,
        meta: skill.source,
      })),
    [skills],
  );

  const handleSpotlightSelect = useCallback(
    (id: string) => {
      if (skills.some((skill) => skill.name === id)) setSelection({ kind: "installed", name: id });
    },
    [skills],
  );

  const handleGhostClick = useCallback((ghost: RepoNewSkill) => {
    setSelection((current) =>
      current?.kind === "ghost" &&
      current.ghost.repo_source === ghost.repo_source &&
      current.ghost.skill_id === ghost.skill_id &&
      current.ghost.folder_path === ghost.folder_path
        ? null
        : { kind: "ghost", ghost },
    );
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

    if (repoFilter) {
      visibleSkills = visibleSkills.filter((skill) => skill.source === repoFilter);
    }

    return visibleSkills;
  }, [skills, searchQuery, agentFilter, profiles, sourceFilter, repoFilter]);

  /** The chip counts what the updates filter would actually show, so it can
   *  never claim updates the active search has already filtered away. The
   *  "update all" CTA stays deliberately filter-independent (see
   *  docs/features/skills/README.md), so it reads the unfiltered list. */
  const pendingUpdateSkills = useMemo(() => scopedSkills.filter(hasPendingUpdate), [scopedSkills]);
  const allPendingUpdates = useMemo(() => skills.filter(hasPendingUpdate), [skills]);

  const filteredSkills = useMemo(() => {
    const visibleSkills = [...(onlyUpdatesFilter ? pendingUpdateSkills : scopedSkills)];

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
  }, [scopedSkills, pendingUpdateSkills, onlyUpdatesFilter, sortBy]);

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
        const failure = report.failed.find((entry) => entry.name === name) ?? report.failed[0];
        if (failure) {
          toast.error(failure.error ? `${t("mySkills.updateFailed")}: ${failure.error}` : t("mySkills.updateFailed"));
          return;
        }
        // Declined by design rather than failed. Without this the button would
        // simply do nothing, which reads as a bug.
        if (report.channel_managed.some((entry) => entry.name === name)) {
          toast.info(t("mySkills.updateChannelManaged", { name }));
          return;
        }
        if (report.uninstalled.includes(name)) {
          toast.success(t("mySkills.droppedSkillRemoved", { name }));
          closeSkillIfSelected(name);
          return;
        }
      } catch (e) {
        const reason = e instanceof Error ? e.message : String(e);
        toast.error(reason ? `${t("mySkills.updateFailed")}: ${reason}` : t("mySkills.updateFailed"));
      }
    },
    [runSkillUpdate, closeSkillIfSelected, t],
  );

  /** Upstream dropped the Skill: same dialog and exits as a blocked update. */
  const handleResolveRemoved = useCallback(
    async (name: string) => {
      const outcome = await resolveRemovedSkill(name);
      if (!outcome) return;
      if (outcome.uninstalled.includes(name)) {
        toast.success(t("mySkills.droppedSkillRemoved", { name }));
        closeSkillIfSelected(name);
      }
      for (const copy of outcome.localCopies) {
        toast.success(t("mySkills.keptAsLocalCopy", { from: name, to: copy.name }));
        closeSkillIfSelected(name);
      }
      const failure = outcome.failed[0];
      if (failure) {
        toast.error(failure.error ? `${t("mySkills.updateFailed")}: ${failure.error}` : t("mySkills.updateFailed"));
      }
    },
    [resolveRemovedSkill, closeSkillIfSelected, t],
  );

  /** Upstream renamed the Skill: one step installs the successor, keeps the
   *  deployments and removes the old entry. Partial outcomes are spelled out. */
  const handleMigrate = useCallback(
    async (name: string) => {
      if (pendingMigrationNames.has(name)) return;
      try {
        const report = await migrateRenamedSkill(name);
        const details = [
          ...report.agent_failures,
          ...report.project_failures,
          ...(report.removal_failure ? [report.removal_failure] : []),
        ];
        if (details.length > 0) {
          toast.warning(t("mySkills.migratePartial", { to: report.installed, details: details.join("; ") }));
        } else {
          toast.success(t("mySkills.migrateSuccess", { from: name, to: report.installed }));
        }
        closeSkillIfSelected(name);
      } catch (e) {
        const reason = e instanceof Error ? e.message : String(e);
        toast.error(
          reason ? `${t("mySkills.migrateFailed", { name })}: ${reason}` : t("mySkills.migrateFailed", { name }),
        );
      }
    },
    [migrateRenamedSkill, pendingMigrationNames, closeSkillIfSelected, t],
  );

  const handleMigrateGhost = useCallback(
    (ghost: RepoNewSkill) => {
      if (ghost.renamed_from) return handleMigrate(ghost.renamed_from);
    },
    [handleMigrate],
  );

  const drawerMigrationName = selectedSkill?.installed
    ? selectedSkill.name
    : selection?.kind === "ghost"
      ? selection.ghost.renamed_from
      : null;
  const handleDrawerMigrate = useCallback(() => {
    if (drawerMigrationName) void handleMigrate(drawerMigrationName);
  }, [drawerMigrationName, handleMigrate]);

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

  const openUninstallDialog = useCallback((names: Iterable<string>, removeSource: string | null = null) => {
    const nextNames = Array.from(new Set(names));
    if (nextNames.length === 0) return;
    setPendingUninstallNames(nextNames);
    setPendingRemoveSource(removeSource);
    setUninstallError(null);
    setUninstallDialogOpen(true);
  }, []);

  const closeUninstallDialog = useCallback(() => {
    if (uninstalling) return;
    setPendingUninstallNames([]);
    setPendingRemoveSource(null);
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
      openUninstallDialog(names, source);
    },
    [openUninstallDialog, repoFilter, skills],
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
        // A repository with unresolved local edits fails closed on purpose.
        // Point at the update flow, which is where that choice is offered.
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
    const sourceToClean = pendingRemoveSource;

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
      if (sourceToClean) {
        try {
          await dismissGhostRepo(sourceToClean);
        } catch {
          // Ghost dismiss is best-effort; installed skills already removed.
        }
      }
      closeUninstallDialog();
      return;
    }

    setPendingUninstallNames(failedNames);
    setUninstallError(
      failedNames.length === 1
        ? t("mySkills.batchUninstallFailed", { name: failedNames[0], count: 1 })
        : t("mySkills.batchUninstallFailed", { name: failedNames[0], count: failedNames.length }),
    );
  }, [
    closeUninstallDialog,
    dismissGhostRepo,
    pendingRemoveSource,
    pendingUninstallNames,
    removeSkillFromUi,
    uninstallSkill,
    t,
  ]);

  /** Summarise a finished batch update — the report is already final, so every
   *  Skill is counted exactly once. `skipped` counts as updated: the backend
   *  collapsed those names into a sibling's pull and their content moved too.
   *  `blocked` here means the user closed the divergence dialog. */
  const reportBatchUpdate = useCallback(
    (report: SkillUpdateRunReport) => {
      const successCount = report.updated.length + report.skipped.length;

      if (report.uninstalled.length > 0) {
        toast.success(t("mySkills.droppedSkillsRemoved", { count: report.uninstalled.length }));
      }

      if (report.failed.length > 0) {
        const reason = report.failed[0]?.error;
        if (successCount > 0) {
          toast.warning(
            t("mySkills.batchUpdatePartial", {
              success: successCount,
              failed: report.failed.length,
              defaultValue: `${successCount} updated, ${report.failed.length} failed`,
            }),
          );
        } else {
          toast.error(reason ? `${t("mySkills.updateFailed")}: ${reason}` : t("mySkills.updateFailed"));
        }
      }

      if (report.blocked.length > 0) {
        toast.warning(
          t("mySkills.batchUpdateBlocked", {
            count: report.blocked.length,
            defaultValue: `${report.blocked.length} update(s) paused for local changes`,
          }),
        );
      }

      if (report.channel_managed.length > 0) {
        toast.info(t("mySkills.batchUpdateChannelManaged", { count: report.channel_managed.length }));
      }

      if (successCount > 0 && report.failed.length === 0) {
        toast.success(
          t("mySkills.batchUpdateSuccess", { count: successCount, defaultValue: `${successCount} skill(s) updated` }),
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
        for (const name of report.uninstalled) closeSkillIfSelected(name);
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
    [reportBatchUpdate, runSkillUpdate, closeSkillIfSelected, t],
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
    if (onlyUpdatesFilter) return t("toolbar.noPendingUpdates");
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
      <div
        className={cn(
          "flex min-w-0 flex-1 flex-col overflow-hidden transition-[padding] duration-300 ease-out",
          // Make room for the detail drawer instead of letting it cover the
          // rightmost grid column (drawer is w-full max-w-md = 448px).
          selectedSkill && "lg:pr-[448px]",
        )}
      >
        <Toolbar
          titleNode={
            <div className="flex flex-wrap items-center gap-3">
              <h1>{t("sidebar.skills")}</h1>
              {scopeSwitch}
            </div>
          }
          searchQuery={searchQuery}
          onSearchChange={setSearchQuery}
          searchItems={spotlightItems}
          onSearchSelect={handleSpotlightSelect}
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
          filteredUpdateCount={pendingUpdateSkills.length}
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
              onResolveRemoved={handleResolveRemoved}
              onMigrate={handleMigrate}
              migratingNames={pendingMigrationNames}
              emptyMessage={getEmptyMessage()}
              emptyAction={getEmptyAction()}
              selectable
              selectedSkills={selectedSkillNames}
              onSelectSkill={handleSelectSkill}
              profiles={profiles}
              onToggleAgent={toggleSkillForAgent}
              pendingUpdateNames={pendingUpdateNames}
              pendingAgentToggleKeys={pendingAgentToggleKeys}
              ghostSkills={
                !searchQuery && !agentFilter && sourceFilter === "all" && !repoFilter && !onlyUpdatesFilter
                  ? ghostSkills
                  : undefined
              }
              onInstallGhost={installGhostSkill}
              onDismissGhost={dismissGhostSkill}
              onDismissGhostRepo={dismissGhostRepo}
              onGhostClick={handleGhostClick}
              onMigrateGhost={handleMigrateGhost}
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
        onResolveRemoved={handleResolveRemoved}
        onMigrate={handleDrawerMigrate}
        migrating={drawerMigrationName != null && pendingMigrationNames.has(drawerMigrationName)}
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
