import { AnimatePresence, motion } from "framer-motion";
import { Download, Layers, Package, Plus } from "lucide-react";
import { useCallback, useEffect, useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { MOTION_TRANSITION } from "../comm/motion";
import { PageToolbar } from "../components/layout/PageToolbar";
import { Button } from "../components/ui/button";
import { EmptyState } from "../components/ui/EmptyState";
import { SearchInput } from "../components/ui/SearchInput";
import { ViewToggle } from "../components/ui/ViewToggle";
import { CreateGroupModal } from "../features/my-skills/components/CreateGroupModal";
import { DeckCard } from "../features/my-skills/components/DeckCard";
import { ExportShareCodeModal } from "../features/my-skills/components/ExportShareCodeModal";
import { ImportDeckBundleModal } from "../features/my-skills/components/ImportDeckBundleModal";
import { ImportShareCodeModal } from "../features/my-skills/components/ImportShareCodeModal";
import { PublishSkillModal } from "../features/my-skills/components/PublishSkillModal";
import { useDeckInstallProgress } from "../features/my-skills/hooks/useDeckInstallProgress";
import { useSkillCards } from "../features/my-skills/hooks/useSkillCards";
import { useSkills } from "../features/my-skills/hooks/useSkills";
import { firstSkipPath, formatBatchToggleSkip } from "../features/my-skills/lib/batchToggleSkip";
import {
  normalizeSkillName,
  normalizeSkillSources,
  uniqueNormalizedSkillNames,
} from "../features/my-skills/lib/skillNames";
import { useAgentProfiles } from "../hooks/useAgentProfiles";
import { useViewMode } from "../hooks/useViewMode";
import { selectTargetableAgentProfiles, supportsGlobalDeploy } from "../lib/agentProfiles";
import { tauriInvoke } from "../lib/ipc";
import { cn } from "../lib/utils";
import type { SkillCardDeck } from "../types";

interface SkillCardsProps {
  onNavigateToProjects?: (skills?: string[]) => void;
  preSelectedSkills?: string[] | null;
  onClearPreSelected?: () => void;
}

export function SkillCards({ onNavigateToProjects, preSelectedSkills, onClearPreSelected }: SkillCardsProps) {
  const { t } = useTranslation();
  const { groups, loading, createGroup, updateGroup, deleteGroup, duplicateGroup } = useSkillCards();
  const { skills, installSkill } = useSkills();
  const { profiles } = useAgentProfiles();
  const [createModalOpen, setCreateModalOpen] = useState(false);
  const [importModalOpen, setImportModalOpen] = useState(false);
  const [importBundleOpen, setImportBundleOpen] = useState(false);
  const [exportGroupTarget, setExportGroupTarget] = useState<SkillCardDeck | null>(null);
  const [editGroup, setEditGroup] = useState<SkillCardDeck | null>(null);
  const [quickPackSkills, setQuickPackSkills] = useState<string[]>([]);
  const [searchQuery, setSearchQuery] = useState("");
  const [viewMode, setViewMode] = useViewMode("grid");
  const [menuOpenId, setMenuOpenId] = useState<string | null>(null);
  const [publishTarget, setPublishTarget] = useState<string | null>(null);
  const enabledProfiles = useMemo(
    () => selectTargetableAgentProfiles(profiles).filter(supportsGlobalDeploy),
    [profiles],
  );
  // Batch-toggle state: { groupId::agentId → "linking" }
  const [linkState, setLinkState] = useState<Record<string, "linking">>({});
  const skillByName = useMemo(
    () => new Map(skills.map((skill) => [normalizeSkillName(skill.name), skill] as const)),
    [skills],
  );
  const { installingMissing, installProgress, installedNameSet, handleInstallMissing } = useDeckInstallProgress({
    skillByName,
    installSkill,
    updateGroup,
  });
  const filteredGroups = useMemo(() => {
    const query = searchQuery.trim().toLowerCase();
    if (!query) return groups;
    return groups.filter((group) => {
      if (group.name.toLowerCase().includes(query)) return true;
      if ((group.description ?? "").toLowerCase().includes(query)) return true;
      return group.skills.some((skillName) => skillName.toLowerCase().includes(query));
    });
  }, [groups, searchQuery]);

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
    [skillByName],
  );

  const handleToggleGroupAgentLinks = useCallback(
    async (
      group: SkillCardDeck,
      agentId: string,
      _agentName: string,
      installedSkillNames: string[],
      allLinked: boolean,
    ) => {
      if (installedSkillNames.length === 0) return;
      const key = `${group.id}::${agentId}`;
      if (linkState[key] === "linking") return;

      setLinkState((prev) => ({ ...prev, [key]: "linking" }));
      try {
        const report = await tauriInvoke("batch_toggle_skills_for_agent", {
          skillNames: installedSkillNames,
          agentId,
          enable: !allLinked,
          operationId: crypto.randomUUID(),
        });
        const failed = report.failed.length;
        const skipped = report.skipped.length;

        if (failed > 0) {
          const visibleFailures = report.failed
            .slice(0, 3)
            .map((failure) => `${failure.skill_name}: ${failure.error}`)
            .join("\n");
          const hiddenCount = Math.max(0, failed - 3);
          toast.error(
            [
              t("skillCards.batchTogglePartialFailed", {
                failed,
                total: installedSkillNames.length,
                defaultValue: "Couldn't update {{failed}}/{{total}} links",
              }),
              visibleFailures,
              hiddenCount > 0 ? `+${hiddenCount}` : "",
            ]
              .filter(Boolean)
              .join("\n"),
          );
        } else if (skipped > 0) {
          // Name collisions (e.g. Hermes owns ~/.hermes/skills/research as a
          // category folder) are expected — surface them as a soft notice so
          // "link all" still feels successful for every skill that could link.
          // Reason must stay explicit: users need the path + "left in place".
          const visibleSkips = report.skipped.slice(0, 3).map((skip) => formatBatchToggleSkip(skip, t));
          const hiddenCount = Math.max(0, skipped - 3);
          const occupiedPath = firstSkipPath(report.skipped);
          toast.message(
            t("skillCards.batchTogglePartialSkipped", {
              skipped,
              total: installedSkillNames.length,
              defaultValue: "Skipped {{skipped}}/{{total}} links — name already occupied on that Agent (left in place)",
            }),
            {
              description: [
                ...visibleSkips,
                hiddenCount > 0
                  ? t("skillCards.batchToggleMoreSkipped", {
                      count: hiddenCount,
                      defaultValue: "+{{count}} more (see logs for full list)",
                    })
                  : "",
              ]
                .filter(Boolean)
                .join("\n"),
              duration: 10000,
              action: occupiedPath
                ? {
                    label: t("skillCards.openOccupiedFolder", { defaultValue: "Open folder" }),
                    onClick: () => {
                      void tauriInvoke("open_folder", { path: occupiedPath }).catch((err) => {
                        if (import.meta.env.DEV) console.error("open_folder failed:", err);
                      });
                    },
                  }
                : undefined,
            },
          );
        }

        // Record the deck's own claim on this Agent. Skipped when every Skill
        // failed — nothing moved on disk, so the rail must not change either.
        // Skips still count as "handled" (the path is intentionally left alone),
        // so a batch of all-skips still claims the Agent.
        if (failed < installedSkillNames.length) {
          const current = group.agent_links ?? [];
          const nextLinks = allLinked ? current.filter((id) => id !== agentId) : [...new Set([...current, agentId])];
          try {
            await updateGroup(group.id, { agentLinks: nextLinks });
          } catch (e) {
            if (import.meta.env.DEV) console.error("Failed to persist deck agent links:", e);
          }
        }

        window.dispatchEvent(new Event("skillstar:refresh-skills"));
      } finally {
        setLinkState((prev) => {
          const next = { ...prev };
          delete next[key];
          return next;
        });
      }
    },
    [linkState, updateGroup, t],
  );

  const handleDelete = async (id: string) => {
    try {
      await deleteGroup(id);
      setMenuOpenId(null);
    } catch (e) {
      if (import.meta.env.DEV) console.error("Delete failed:", e);
    }
  };

  const handleDuplicate = async (id: string) => {
    try {
      await duplicateGroup(id);
      setMenuOpenId(null);
    } catch (e) {
      if (import.meta.env.DEV) console.error("Duplicate failed:", e);
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
    <div className="flex-1 min-w-0 flex flex-col overflow-hidden">
      {/* Header */}
      <PageToolbar
        title={t("sidebar.groups")}
        search={
          <SearchInput
            containerClassName="w-56"
            value={searchQuery}
            onChange={(e) => setSearchQuery(e.target.value)}
            placeholder={t("skillCards.searchPlaceholder")}
            className="pl-8 h-8 text-xs bg-sidebar/50 focus-visible:bg-background"
            iconClassName="left-2.5"
          />
        }
        filters={
          !loading ? (
            <div className="h-8 px-3 flex items-center justify-center gap-1.5 rounded-lg border border-border/80 bg-background/60 shadow-2xs text-xs font-bold text-foreground tabular-nums whitespace-nowrap shrink-0">
              <Layers className="w-3.5 h-3.5 text-primary" />
              {filteredGroups.length}
            </div>
          ) : undefined
        }
        actions={
          <>
            <Button size="sm" variant="secondary" onClick={() => setImportModalOpen(true)}>
              <Download className="w-3.5 h-3.5" />
              {t("common.import")}
            </Button>
            <Button size="sm" variant="secondary" onClick={() => setImportBundleOpen(true)}>
              <Package className="w-3.5 h-3.5" />
              {t("toolbar.importFile")}
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
            <ViewToggle viewMode={viewMode} onViewModeChange={setViewMode} />
          </>
        }
      />

      <motion.main
        initial={{ opacity: 0 }}
        animate={{ opacity: 1 }}
        transition={MOTION_TRANSITION.fadeBase}
        className="ss-page-scroll"
      >
        <div className="ss-page-stack">
          {loading ? (
            <div className="text-zinc-500 text-sm">{t("skillCards.loading")}</div>
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
          ) : filteredGroups.length === 0 ? (
            <EmptyState
              icon={<Package className="w-6 h-6 text-muted-foreground" />}
              title={t("skillCards.noMatching")}
              description={t("skillCards.tryDifferent")}
              size="lg"
            />
          ) : (
            <div className={cn(viewMode === "grid" ? "ss-decks-grid" : "ss-decks-list")}>
              <AnimatePresence>
                {filteredGroups.map((group) => {
                  const groupSkillNames = uniqueNormalizedSkillNames(group.skills);
                  const groupInstalledSkillNames = groupSkillNames.filter((name) => installedNameSet.has(name));
                  return (
                    <DeckCard
                      key={group.id}
                      group={group}
                      viewMode={viewMode}
                      groupSkillNames={groupSkillNames}
                      groupInstalledSkillNames={groupInstalledSkillNames}
                      skillByName={skillByName}
                      enabledProfiles={enabledProfiles}
                      linkState={linkState}
                      installingMissing={installingMissing}
                      installProgress={installProgress}
                      menuOpenId={menuOpenId}
                      onMenuOpenChange={setMenuOpenId}
                      onEdit={setEditGroup}
                      onExport={setExportGroupTarget}
                      onDuplicate={handleDuplicate}
                      onDelete={handleDelete}
                      onInstallMissing={handleInstallMissing}
                      onToggleGroupAgentLinks={handleToggleGroupAgentLinks}
                      onDeploy={(skillNames) => onNavigateToProjects?.(skillNames)}
                    />
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
        existingGroups={groups}
        onImport={async (name, desc, icon, skillNames, sources, download) => {
          const newGroup = await createGroup(name, desc, icon, skillNames, sources);
          if (download && newGroup) {
            setTimeout(() => {
              handleInstallMissing(newGroup);
            }, 100);
          }
        }}
      />

      <ImportDeckBundleModal
        open={importBundleOpen}
        onClose={() => setImportBundleOpen(false)}
        onDeckImported={async (skillNames, name, description) => {
          await createGroup(name, description, "📦", skillNames, buildSkillSources(skillNames));
          window.dispatchEvent(new Event("skillstar:refresh-skills"));
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
        skillDescription={skills.find((s) => s.name === publishTarget)?.description || ""}
        onPublished={() => {
          // Stay on the success ("done") phase so the user can see / copy the
          // published repo URL; closing here would skip it. Refresh the skills
          // snapshot in the background so the new git-backed state is reflected.
          window.dispatchEvent(new Event("skillstar:refresh-skills"));
        }}
      />
    </div>
  );
}
