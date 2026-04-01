import { useState, useCallback, useMemo, useEffect, useRef } from "react";
import { motion, AnimatePresence } from "framer-motion";
import { useTranslation } from "react-i18next";
import { Share2, Plus, Rocket, Copy, Trash2, MoreHorizontal, Edit2, Download, FolderKanban, Package, AlertTriangle, Loader2 } from "lucide-react";
import { invoke } from "@tauri-apps/api/core";
import { toast } from "sonner";
import { Button } from "../components/ui/button";
import { Badge } from "../components/ui/badge";
import { Card } from "../components/ui/card";
import { EmptyState } from "../components/ui/EmptyState";
import { HScrollRow } from "../components/ui/HScrollRow";
import { CreateGroupModal } from "../components/skills/CreateGroupModal";
import { ImportShareCodeModal } from "../components/skills/ImportShareCodeModal";
import { ExportShareCodeModal } from "../components/skills/ExportShareCodeModal";
import { PublishSkillModal } from "../components/skills/PublishSkillModal";
import { useSkillCards } from "../hooks/useSkillCards";
import { useSkills } from "../hooks/useSkills";
import { useAgentProfiles } from "../hooks/useAgentProfiles";
import { AgentIcon } from "../components/ui/AgentIcon";
import { cn, agentIconCls } from "../lib/utils";
import type { SkillCardDeck, Skill } from "../types";

interface SkillCardsProps {
  onNavigateToProjects?: (skills?: string[]) => void;
  preSelectedSkills?: string[] | null;
  onClearPreSelected?: () => void;
}

const normalizeSkillName = (name: string) => name.trim();

const uniqueNormalizedSkillNames = (names: string[]): string[] => {
  const seen = new Set<string>();
  const normalized: string[] = [];
  for (const rawName of names) {
    const name = normalizeSkillName(rawName);
    if (!name || seen.has(name)) continue;
    seen.add(name);
    normalized.push(name);
  }
  return normalized;
};

const normalizeSkillSources = (sources?: Record<string, string>): Record<string, string> => {
  const normalized: Record<string, string> = {};
  if (!sources) return normalized;
  for (const [rawName, rawUrl] of Object.entries(sources)) {
    const name = normalizeSkillName(rawName);
    const url = rawUrl?.trim();
    if (!name || !url) continue;
    normalized[name] = url;
  }
  return normalized;
};

// ── Module-level install progress store ─────────────────────────────
// Survives component unmount/remount so switching pages doesn't lose
// the active install state. Each entry maps groupId → progress.
interface InstallProgressEntry {
  done: number;
  total: number;
  abortController?: AbortController;
}
const activeInstalls = new Map<string, InstallProgressEntry>();
const installListeners = new Set<() => void>();
function notifyInstallListeners() {
  for (const fn of installListeners) fn();
}

// Clean up module-level state during HMR to prevent stale data pollution
if (import.meta.hot) {
  import.meta.hot.dispose(() => {
    activeInstalls.clear();
    installListeners.clear();
  });
}

export function SkillCards({
  onNavigateToProjects,
  preSelectedSkills,
  onClearPreSelected,
}: SkillCardsProps) {
  const { t } = useTranslation();
  const { groups, loading, createGroup, updateGroup, deleteGroup, duplicateGroup } =
    useSkillCards();
  const { skills, installSkill, toggleSkillForAgent } = useSkills();
  const { profiles } = useAgentProfiles();
  const [createModalOpen, setCreateModalOpen] = useState(false);
  const [importModalOpen, setImportModalOpen] = useState(false);
  const [exportGroupTarget, setExportGroupTarget] = useState<SkillCardDeck | null>(null);
  const [editGroup, setEditGroup] = useState<SkillCardDeck | null>(null);
  const [quickPackSkills, setQuickPackSkills] = useState<string[]>([]);
  const [menuOpenId, setMenuOpenId] = useState<string | null>(null);
  const [publishTarget, setPublishTarget] = useState<string | null>(null);
  const [installingMissing, setInstallingMissing] = useState<string | null>(
    // Restore from module-level store on mount
    () => {
      for (const id of activeInstalls.keys()) return id;
      return null;
    }
  );
  const [installProgress, setInstallProgress] = useState<{ done: number; total: number } | null>(
    () => {
      for (const [, entry] of activeInstalls) return { done: entry.done, total: entry.total };
      return null;
    }
  );
  const [backendInstalledNames, setBackendInstalledNames] = useState<Set<string>>(new Set());
  // Track whether install handler is owned by this mount
  const installOwnerRef = useRef(false);
  const enabledProfiles = profiles.filter((p) => p.enabled);
  // Batch-toggle state: { groupId::agentId → "linking" }
  const [linkState, setLinkState] = useState<Record<string, "linking">>({});
  const skillByName = useMemo(
    () =>
      new Map(
        skills.map((skill) => [normalizeSkillName(skill.name), skill] as const)
      ),
    [skills]
  );
  const installedNameSet = useMemo(() => {
    const next = new Set<string>(backendInstalledNames);
    for (const name of skillByName.keys()) {
      next.add(name);
    }
    return next;
  }, [backendInstalledNames, skillByName]);

  const refreshBackendInstalledNames = useCallback(async () => {
    try {
      const latest = await invoke<Skill[]>("list_skills");
      const next = new Set(
        latest.map((skill) => normalizeSkillName(skill.name)).filter(Boolean)
      );
      setBackendInstalledNames(next);
      return next;
    } catch (e) {
      console.error("Failed to refresh installed skills snapshot:", e);
      return null;
    }
  }, []);

  useEffect(() => {
    void refreshBackendInstalledNames();
  }, [refreshBackendInstalledNames]);

  const buildSkillSources = useCallback(
    (selectedSkills: string[], existingSources?: Record<string, string>) => {
      const nextSources = normalizeSkillSources(existingSources);
      for (const rawName of selectedSkills) {
        const skillName = normalizeSkillName(rawName);
        if (!skillName) continue;
        const existing = nextSources[skillName];
        if (existing) {
          continue;
        }
        const gitUrl = skillByName.get(skillName)?.git_url?.trim();
        if (gitUrl) {
          nextSources[skillName] = gitUrl;
        }
      }
      return nextSources;
    },
    [skillByName]
  );

  const handleToggleGroupAgentLinks = useCallback(
    async (
      group: SkillCardDeck,
      agentId: string,
      agentName: string,
      installedSkillNames: string[],
      allLinked: boolean
    ) => {
      if (installedSkillNames.length === 0) return;
      const key = `${group.id}::${agentId}`;
      if (linkState[key] === "linking") return;

      setLinkState((prev) => ({ ...prev, [key]: "linking" }));
      try {
        await Promise.all(
          installedSkillNames.map((skillName) =>
            toggleSkillForAgent(skillName, agentId, !allLinked, agentName)
          )
        );
      } catch (e) {
        console.error("Batch toggle failed:", e);
      } finally {
        setLinkState((prev) => {
          const next = { ...prev };
          delete next[key];
          return next;
        });
      }
    },
    [linkState, toggleSkillForAgent]
  );

  const handleDelete = async (id: string) => {
    try {
      await deleteGroup(id);
      setMenuOpenId(null);
    } catch (e) {
      console.error("Delete failed:", e);
    }
  };

  const handleDuplicate = async (id: string) => {
    try {
      await duplicateGroup(id);
      setMenuOpenId(null);
    } catch (e) {
      console.error("Duplicate failed:", e);
    }
  };

  // Subscribe to module-level install progress changes so remounts pick up live state
  useEffect(() => {
    const listener = () => {
      const entry = Array.from(activeInstalls.entries())[0];
      if (entry) {
        setInstallingMissing(entry[0]);
        setInstallProgress({ done: entry[1].done, total: entry[1].total });
      } else {
        setInstallingMissing(null);
        setInstallProgress(null);
      }
    };
    installListeners.add(listener);
    return () => { installListeners.delete(listener); };
  }, []);

  const handleInstallMissing = async (group: SkillCardDeck) => {
    if (installingMissing || activeInstalls.has(group.id)) return;
    const groupSkillNames = uniqueNormalizedSkillNames(group.skills);
    if (groupSkillNames.length === 0) return;

    const refreshedInstalled = await refreshBackendInstalledNames();
    const installedSnapshot = refreshedInstalled ?? installedNameSet;
    const missing = groupSkillNames.filter((name) => !installedSnapshot.has(name));
    if (missing.length === 0) return;

    const nextSources = normalizeSkillSources(group.skill_sources);

    // Identify names that have no known source
    const namesNeedingSource = missing.filter((name) => !nextSources[name]);

    // Batch-resolve missing sources via backend marketplace search
    if (namesNeedingSource.length > 0) {
      try {
        const resolved = await invoke<Record<string, string>>("resolve_skill_sources", {
          names: namesNeedingSource,
          existingSources: nextSources,
        });
        for (const [name, url] of Object.entries(resolved)) {
          if (url) nextSources[name] = url;
        }
      } catch (e) {
        console.error("[SkillCards] resolve_skill_sources failed:", e);
      }
    }

    const installQueue: Array<{ name: string; url: string }> = [];
    const noSourceNames: string[] = [];
    for (const name of missing) {
      const url = nextSources[name];
      if (url) {
        installQueue.push({ name, url });
      } else {
        noSourceNames.push(name);
      }
    }

    if (installQueue.length === 0) {
      toast.error(
        t("skillCards.installNoSource", {
          defaultValue: "No install source found for missing skills",
        })
      );
      return;
    }

    // Persist resolved sources back to the group
    const sourcesChanged = namesNeedingSource.some((name) => !!nextSources[name]);

    // Register in module-level store
    const progressEntry: InstallProgressEntry = { done: 0, total: installQueue.length };
    activeInstalls.set(group.id, progressEntry);
    setInstallingMissing(group.id);
    setInstallProgress({ done: 0, total: installQueue.length });
    setMenuOpenId(null);
    installOwnerRef.current = true;
    notifyInstallListeners();

    let successCount = 0;
    const failedNames: string[] = [];

    // Concurrent install with bounded parallelism (3 at a time)
    const CONCURRENCY = 3;
    let cursor = 0;
    const runNext = async (): Promise<void> => {
      while (cursor < installQueue.length) {
        const idx = cursor++;
        const item = installQueue[idx];
        try {
          await installSkill(item.url, item.name);
          successCount++;
        } catch (e) {
          console.error(`Failed to install ${item.name}:`, e);
          failedNames.push(item.name);
        }
        // Update progress
        progressEntry.done++;
        activeInstalls.set(group.id, { ...progressEntry });
        // Only update local state if this mount owns the install
        if (installOwnerRef.current) {
          setInstallProgress({ done: progressEntry.done, total: progressEntry.total });
        }
        notifyInstallListeners();
      }
    };

    try {
      await Promise.all(
        Array.from({ length: Math.min(CONCURRENCY, installQueue.length) }, () => runNext())
      );

      if (sourcesChanged) {
        await updateGroup(group.id, { skillSources: nextSources });
      }
      // Summary toast
      if (successCount > 0 && failedNames.length === 0 && noSourceNames.length === 0) {
        toast.success(
          t("skillCards.installAllSuccess", {
            count: successCount,
            defaultValue: `Successfully installed ${successCount} skill(s)`,
          })
        );
      } else if (successCount > 0) {
        toast.warning(
          t("skillCards.installPartial", {
            success: successCount,
            failed: failedNames.length + noSourceNames.length,
            defaultValue: `Installed ${successCount}, failed ${failedNames.length + noSourceNames.length}`,
          })
        );
      } else {
        toast.error(
          t("skillCards.installAllFailed", {
            defaultValue: "Failed to install skills",
          })
        );
      }
    } finally {
      activeInstalls.delete(group.id);
      installOwnerRef.current = false;
      setInstallingMissing(null);
      setInstallProgress(null);
      notifyInstallListeners();
      void refreshBackendInstalledNames();
      window.dispatchEvent(new Event("skillstar:refresh-skills"));
    }
  };

  useEffect(() => {
    if (!preSelectedSkills || preSelectedSkills.length === 0) return;
    setQuickPackSkills([...new Set(preSelectedSkills)]);
    setEditGroup(null);
    setCreateModalOpen(true);
    onClearPreSelected?.();
  }, [preSelectedSkills, onClearPreSelected]);

  useEffect(() => {
    if (!menuOpenId) return;
    const handleClickOutside = () => setMenuOpenId(null);
    document.addEventListener("click", handleClickOutside);
    return () => document.removeEventListener("click", handleClickOutside);
  }, [menuOpenId]);

  return (
    <div className="flex-1 flex flex-col overflow-hidden">
      {/* Header */}
      <div className="h-14 flex items-center justify-between px-6 border-b border-border bg-sidebar">
        <div className="flex items-center gap-3">
          <h1>{t("sidebar.groups")}</h1>
          {!loading && (
            <Badge variant="outline">{t("skillCards.groupsCount", { count: groups.length })}</Badge>
          )}
        </div>
        <div className="flex items-center gap-2">
          <Button size="sm" variant="outline" onClick={() => onNavigateToProjects?.()}>
            <FolderKanban className="w-3.5 h-3.5" />
            {t("skillCards.manageProject")}
          </Button>
          <Button size="sm" variant="secondary" onClick={() => setImportModalOpen(true)}>
            <Download className="w-3.5 h-3.5" />
            {t("common.import")}
          </Button>
          <Button
            size="sm"
            onClick={() => {
              setQuickPackSkills([]);
              setEditGroup(null);
              setCreateModalOpen(true);
            }}
          >
            <Plus className="w-3.5 h-3.5" />
            {t("skillCards.newGroup")}
          </Button>
        </div>
      </div>

      <motion.main
        initial={{ opacity: 0 }}
        animate={{ opacity: 1 }}
        transition={{ duration: 0.2 }}
        className="flex-1 overflow-y-auto p-6"
      >
        <div className="space-y-6">

          {loading ? (
            <div className="text-zinc-500 text-sm">
              {t("skillCards.loading")}
            </div>
          ) : groups.length === 0 ? (
            <EmptyState
              icon={<Package className="w-6 h-6 text-primary" />}
              title={t("skillCards.emptyTitle")}
              description={t("skillCards.emptyDesc")}
              action={
                <Button onClick={() => setCreateModalOpen(true)}>
                  <Plus className="w-3.5 h-3.5" />
                  {t("skillCards.createFirst")}
                </Button>
              }
            />
          ) : (
            <div className="grid grid-cols-[repeat(auto-fill,minmax(300px,1fr))] gap-5 max-w-6xl mx-auto">
              <AnimatePresence>
                {groups.map((group) => {
                  const groupSkillNames = uniqueNormalizedSkillNames(group.skills);
                  const groupInstalledSkillNames = groupSkillNames.filter((name) =>
                    installedNameSet.has(name)
                  );
                  const installedCount = groupInstalledSkillNames.length;
                  const totalCount = groupSkillNames.length;
                  const missingCount = totalCount - installedCount;
                  const isInstallingThis = installingMissing === group.id;
                  return (
                    <motion.div
                      key={group.id}
                      initial={{ opacity: 0, scale: 0.95 }}
                      animate={{ opacity: 1, scale: 1 }}
                      exit={{ opacity: 0, scale: 0.95 }}
                      className={cn(
                        "relative transition-shadow min-h-[200px]",
                        menuOpenId === group.id ? "z-50" : "z-0 hover:z-10"
                      )}
                    >
                      <Card className="hover:bg-card-hover flex flex-col h-full relative group shadow-sm hover:shadow-xl transition p-0 border border-border bg-card overflow-hidden">
                        <div className="p-4 flex flex-col flex-1 relative min-h-0">
                          {/* Top Action Row (Context Menu) */}
                          <div className="absolute top-4 right-4 z-20 flex items-center gap-1">
                            <button
                              onClick={(e) => {
                                e.stopPropagation();
                                setExportGroupTarget(group);
                              }}
                              className="p-2.5 -mr-1 rounded-lg hover:bg-muted text-muted-foreground transition-colors outline-none focus-visible:ring-2 focus-visible:ring-primary"
                              title={t("skillCards.exportShareCode")}
                              aria-label={t("skillCards.exportShareCode")}
                            >
                              <Share2 className="w-4 h-4" />
                            </button>
                            <div className="relative">
                              <button
                                onClick={(e) => {
                                  e.stopPropagation();
                                  setMenuOpenId(
                                    menuOpenId === group.id ? null : group.id
                                  );
                                }}
                                className="p-2.5 rounded-lg hover:bg-muted text-muted-foreground transition-colors outline-none focus-visible:ring-2 focus-visible:ring-primary mt-0.5"
                                aria-label={t("common.more")}
                              >
                                <MoreHorizontal className="w-4 h-4" />
                              </button>

                              {menuOpenId === group.id && (
                                <motion.div
                                  initial={{ opacity: 0, scale: 0.95 }}
                                  animate={{ opacity: 1, scale: 1 }}
                                  className="absolute right-0 top-full mt-1 w-36 p-1 rounded-xl border border-border bg-card backdrop-blur-xl shadow-xl z-30"
                                >
                                  <button
                                    onClick={() => {
                                      setEditGroup(group);
                                      setMenuOpenId(null);
                                    }}
                                    className="w-full flex items-center gap-2 px-3 py-1.5 rounded-lg text-xs hover:bg-card-hover transition-colors cursor-pointer"
                                  >
                                    <Edit2 className="w-3 h-3" />
                                    {t("common.edit")}
                                  </button>
                                  <button
                                    onClick={() => handleDuplicate(group.id)}
                                    className="w-full flex items-center gap-2 px-3 py-1.5 rounded-lg text-xs hover:bg-card-hover transition-colors cursor-pointer"
                                  >
                                    <Copy className="w-3 h-3" />
                                    {t("common.duplicate")}
                                  </button>
                                  <button
                                    onClick={() => handleDelete(group.id)}
                                    className="w-full flex items-center gap-2 px-3 py-1.5 rounded-lg text-xs text-destructive hover:bg-destructive/10 transition-colors cursor-pointer"
                                  >
                                    <Trash2 className="w-3 h-3" />
                                    {t("common.delete")}
                                  </button>
                                  {missingCount > 0 && (
                                    <button
                                      onClick={() => handleInstallMissing(group)}
                                      disabled={isInstallingThis}
                                      className="w-full flex items-center gap-2 px-3 py-1.5 rounded-lg text-xs text-warning-foreground hover:bg-warning/10 transition-colors cursor-pointer"
                                    >
                                      {isInstallingThis ? (
                                        <Loader2 className="w-3 h-3 animate-spin" />
                                      ) : (
                                        <Download className="w-3 h-3" />
                                      )}
                                      {isInstallingThis && installProgress
                                        ? `${installProgress.done}/${installProgress.total}`
                                        : t("skillCards.installMissing", { count: missingCount, defaultValue: `Install missing (${missingCount})` })}
                                    </button>
                                  )}
                                </motion.div>
                              )}
                            </div>
                          </div>

                          {/* Header section */}
                          <div className="flex items-start gap-4 pr-8 mb-5">
                            <div className="w-12 h-12 rounded-xl bg-primary/5 border border-primary/10 flex items-center justify-center text-2xl shrink-0">
                              {group.icon}
                            </div>
                            <div className="min-w-0 pt-1">
                              <h3 className="text-base font-semibold leading-tight truncate text-foreground transition-colors">
                                <button type="button" onClick={() => setEditGroup(group)} className="w-full text-left truncate rounded outline-none focus-visible:ring-2 focus-visible:ring-primary hover:text-primary cursor-pointer transition-colors">
                                  {group.name}
                                </button>
                              </h3>
                              {group.description ? (
                                <p className="text-xs text-muted-foreground line-clamp-2 mt-1">
                                  {group.description}
                                </p>
                              ) : (
                                <p className="text-xs text-muted-foreground italic mt-1 opacity-60">
                                  {t("skillCards.noDescription")}
                                </p>
                              )}
                            </div>
                          </div>

                          {/* Skills Preview Tags */}
                          <div className="flex flex-wrap items-center gap-1.5 mt-auto overflow-hidden max-h-[46px]">
                            {groupSkillNames.slice(0, 5).map((skillName) => {
                              const skill = skillByName.get(skillName);
                              return (
                                <Badge
                                  key={skillName}
                                  variant="outline"
                                  className={cn(
                                    "text-micro font-medium px-2 py-0.5 h-5",
                                    skill ? "bg-muted text-muted-foreground border-transparent" : "text-warning bg-warning/5 border-warning/20 font-normal"
                                  )}
                                >
                                  {skillName}
                                </Badge>
                              );
                            })}
                            {groupSkillNames.length > 5 && (
                              <Badge
                                variant="outline"
                                className="text-micro font-medium px-2 py-0.5 h-5 bg-muted text-muted-foreground border-transparent"
                              >
                                +{groupSkillNames.length - 5}
                              </Badge>
                            )}
                            {missingCount > 0 && (
                              <Badge
                                variant="outline"
                                className="text-micro font-medium px-2 py-0.5 h-5 bg-warning/5 text-warning border-warning/20 tabular-nums"
                              >
                                {installedCount}/{totalCount}
                              </Badge>
                            )}
                          </div>
                        </div>

                        {/* Footer section */}
                        <div className="px-4 py-2.5 border-t border-border/50 mt-auto flex items-center rounded-b-xl min-h-[44px]">
                          {installedCount === 0 ? (
                            /* All skills missing — show warning */
                            <div className="flex items-center gap-2 flex-1 min-w-0">
                              <AlertTriangle className="w-3.5 h-3.5 text-warning shrink-0" />
                              <span className="text-xs text-muted-foreground truncate">
                                {t("skillCards.noSkillsInstalled", { defaultValue: "No skills installed" })}
                              </span>
                              {missingCount > 0 && (
                                <Button
                                  size="sm"
                                  variant="ghost"
                                  className="h-7 px-3 text-xs ml-auto text-muted-foreground hover:text-foreground shrink-0"
                                  disabled={isInstallingThis}
                                  onClick={(e) => {
                                    e.stopPropagation();
                                    handleInstallMissing(group);
                                  }}
                                >
                                  {isInstallingThis ? (
                                    <Loader2 className="w-3 h-3 animate-spin" />
                                  ) : (
                                    <Download className="w-3 h-3" />
                                  )}
                                  {isInstallingThis && installProgress
                                    ? `${installProgress.done}/${installProgress.total}`
                                    : t("skillCards.installAll", { defaultValue: "Install all" })}
                                </Button>
                              )}
                            </div>
                          ) : (
                            /* Normal footer: Agent icons + Deploy */
                            <div className="flex items-center gap-1.5 flex-1 min-w-0">
                              {/* Link to Agent icons */}
                              <HScrollRow count={enabledProfiles.length} maxVisible={6} itemWidth={28} gap={6} className="gap-1.5">
                              {enabledProfiles.map((profile) => {
                                const key = `${group.id}::${profile.id}`;
                                const state = linkState[key];
                                const linkedCount = groupInstalledSkillNames.filter((name) =>
                                  skillByName
                                    .get(name)
                                    ?.agent_links?.includes(profile.display_name)
                                ).length;
                                const allLinked =
                                  groupInstalledSkillNames.length > 0 &&
                                  linkedCount === groupInstalledSkillNames.length;
                                const partialLinked =
                                  linkedCount > 0 && linkedCount < groupInstalledSkillNames.length;
                                const linking = state === "linking";
                                return (
                                  <button
                                    key={profile.id}
                                    onClick={(e) => {
                                      e.stopPropagation();
                                      void handleToggleGroupAgentLinks(
                                        group,
                                        profile.id,
                                        profile.display_name,
                                        groupInstalledSkillNames,
                                        allLinked
                                      );
                                    }}
                                    disabled={linking || groupInstalledSkillNames.length === 0}
                                    title={
                                      allLinked
                                        ? t("skillCards.unlinkAllFrom", {
                                            agent: profile.display_name,
                                          })
                                        : t("skillCards.linkAllTo", {
                                            agent: profile.display_name,
                                          })
                                    }
                                    className={cn(
                                      "w-7 h-7 rounded-lg flex items-center justify-center border transition cursor-pointer shrink-0",
                                      allLinked
                                        ? "border-primary/20 bg-primary/5 shadow-sm"
                                        : linking
                                          ? "border-primary/30 bg-primary/5 opacity-60"
                                          : partialLinked
                                            ? "border-warning/30 bg-warning/5"
                                          : "border-transparent hover:bg-muted hover:border-border text-muted-foreground"
                                    )}
                                  >
                                    <AgentIcon
                                      profile={profile}
                                      className={cn(
                                        agentIconCls(profile.icon),
                                        "transition-[filter,opacity] duration-300",
                                        linking && "animate-pulse",
                                        !allLinked && !partialLinked &&
                                          "grayscale opacity-40 hover:opacity-80 hover:grayscale-0"
                                      )}
                                    />
                                  </button>
                                );
                              })}
                              </HScrollRow>

                              {/* Separator */}
                              {enabledProfiles.length > 0 && (
                                <div className="w-px h-4 bg-border mx-0.5 ml-auto" />
                              )}

                              {/* Deploy to project */}
                              <Button
                                size="sm"
                                className="h-7 px-3 text-xs group/btn bg-primary hover:bg-primary/90"
                                onClick={(e) => {
                                  e.stopPropagation();
                                  onNavigateToProjects?.(groupSkillNames);
                                }}
                              >
                                <Rocket className="w-3 h-3 mr-1.5 transition-transform group-hover/btn:-translate-y-[1px] group-hover/btn:translate-x-[1px]" />
                                {t("skillCards.deploy")}
                              </Button>
                            </div>
                          )}
                        </div>
                      </Card>
                    </motion.div>
                  );
                })}
              </AnimatePresence>
            </div>
          )}
        </div>
      </motion.main>

      <CreateGroupModal
        open={createModalOpen || editGroup !== null}
        onClose={() => {
          setCreateModalOpen(false);
          setEditGroup(null);
          setQuickPackSkills([]);
        }}
        availableSkills={skills}
        existingNames={groups.map((g) => g.name)}
        initialName={editGroup?.name}
        initialDescription={editGroup?.description}
        initialIcon={editGroup?.icon}
        initialSkills={editGroup?.skills ?? quickPackSkills}
        mode={editGroup ? "edit" : "create"}
        onSave={async (name, desc, icon, selectedSkills) => {
          if (editGroup) {
            await updateGroup(editGroup.id, {
              name,
              description: desc,
              icon,
              skills: selectedSkills,
              skillSources: buildSkillSources(selectedSkills, editGroup.skill_sources),
            });
          } else {
            await createGroup(name, desc, icon, selectedSkills, buildSkillSources(selectedSkills));
            setQuickPackSkills([]);
          }
        }}
      />

      <ImportShareCodeModal
        open={importModalOpen}
        onClose={() => setImportModalOpen(false)}
        onImport={async (name, desc, icon, skillNames, sources) => {
          await createGroup(name, desc, icon, skillNames, sources);
        }}
      />

      <ExportShareCodeModal
        open={!!exportGroupTarget}
        onClose={() => setExportGroupTarget(null)}
        group={exportGroupTarget}
        hubSkills={skills}
        onPublishSkill={(name) => {
          setExportGroupTarget(null);
          setPublishTarget(name);
        }}
      />

      <PublishSkillModal
        open={!!publishTarget}
        onClose={() => setPublishTarget(null)}
        skillName={publishTarget || ""}
        skillDescription={
          skills.find((s) => s.name === publishTarget)?.description || ""
        }
        onPublished={() => {
          setPublishTarget(null);
        }}
      />
    </div>
  );
}
