import { useState, useCallback, useMemo, useEffect } from "react";
import { motion, AnimatePresence } from "framer-motion";
import { useTranslation } from "react-i18next";
import { Share2, Plus, Rocket, Copy, Trash2, MoreHorizontal, Edit2, Download, FolderKanban, Package } from "lucide-react";
import { Button } from "../components/ui/button";
import { Badge } from "../components/ui/badge";
import { Card } from "../components/ui/card";
import { EmptyState } from "../components/ui/EmptyState";
import { CreateGroupModal } from "../components/skills/CreateGroupModal";
import { ImportShareCodeModal } from "../components/skills/ImportShareCodeModal";
import { ExportShareCodeModal } from "../components/skills/ExportShareCodeModal";
import { PublishSkillModal } from "../components/skills/PublishSkillModal";
import { useSkillCards } from "../hooks/useSkillCards";
import { useSkills } from "../hooks/useSkills";
import { useAgentProfiles } from "../hooks/useAgentProfiles";
import { cn } from "../lib/utils";
import type { SkillCardDeck } from "../types";

interface SkillCardsProps {
  onNavigateToProjects?: (skills?: string[]) => void;
  preSelectedSkills?: string[] | null;
  onClearPreSelected?: () => void;
}

export function SkillCards({
  onNavigateToProjects,
  preSelectedSkills,
  onClearPreSelected,
}: SkillCardsProps) {
  const { t } = useTranslation();
  const { groups, loading, createGroup, updateGroup, deleteGroup, duplicateGroup } =
    useSkillCards();
  const { skills, toggleSkillForAgent } = useSkills();
  const { profiles } = useAgentProfiles();
  const [createModalOpen, setCreateModalOpen] = useState(false);
  const [importModalOpen, setImportModalOpen] = useState(false);
  const [exportGroupTarget, setExportGroupTarget] = useState<SkillCardDeck | null>(null);
  const [editGroup, setEditGroup] = useState<SkillCardDeck | null>(null);
  const [quickPackSkills, setQuickPackSkills] = useState<string[]>([]);
  const [menuOpenId, setMenuOpenId] = useState<string | null>(null);
  const [publishTarget, setPublishTarget] = useState<string | null>(null);
  const enabledProfiles = profiles.filter((p) => p.enabled);
  // Batch-toggle state: { groupId::agentId → "linking" }
  const [linkState, setLinkState] = useState<Record<string, "linking">>({});
  const skillByName = useMemo(
    () => new Map(skills.map((skill) => [skill.name, skill])),
    [skills]
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
      <div className="h-14 flex items-center justify-between px-6 border-b border-border bg-card/30 backdrop-blur-sm">
        <div className="flex items-center gap-3">
          <h1 className="text-heading-md text-zinc-100">{t("sidebar.groups")}</h1>
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
        className="flex-1 overflow-y-auto px-8 py-8 bg-gradient-to-br from-transparent via-card/10 to-transparent"
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
            <div className="grid gap-5" style={{ gridTemplateColumns: "repeat(auto-fill, minmax(320px, 1fr))" }}>
              <AnimatePresence>
                {groups.map((group) => {
                  return (
                    <motion.div
                      key={group.id}
                      initial={{ opacity: 0, scale: 0.95 }}
                      animate={{ opacity: 1, scale: 1 }}
                      exit={{ opacity: 0, scale: 0.95 }}
                      className={cn(
                        "relative transition-shadow h-[200px]",
                        menuOpenId === group.id ? "z-50" : "z-0 hover:z-10"
                      )}
                    >
                      <Card className="hover:bg-card/60 flex flex-col h-full relative group shadow-sm hover:shadow-xl transition-all p-0 border border-white/10 bg-card/40 backdrop-blur-sm overflow-hidden">
                        <div className="p-4 flex flex-col flex-1 relative min-h-0">
                          {/* Top Action Row (Context Menu) */}
                          <div className="absolute top-4 right-4 z-20 flex items-center gap-1">
                            <button
                              onClick={(e) => {
                                e.stopPropagation();
                                setExportGroupTarget(group);
                              }}
                              className="p-1.5 rounded-lg hover:bg-muted text-muted-foreground transition-colors cursor-pointer"
                              title={t("skillCards.exportShareCode")}
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
                                className="p-1.5 rounded-lg hover:bg-muted text-muted-foreground transition-colors cursor-pointer"
                              >
                                <MoreHorizontal className="w-4 h-4" />
                              </button>

                              {menuOpenId === group.id && (
                                <motion.div
                                  initial={{ opacity: 0, scale: 0.95 }}
                                  animate={{ opacity: 1, scale: 1 }}
                                  className="absolute right-0 top-full mt-1 w-36 p-1 rounded-xl border border-white/10 bg-card/80 backdrop-blur-xl shadow-xl z-30"
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
                              <h3 className="text-base font-semibold leading-tight truncate text-foreground group-hover:text-primary transition-colors cursor-pointer" onClick={() => setEditGroup(group)}>
                                {group.name}
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

                          {/* Skills Preview Tags (Max 4) */}
                          <div className="flex flex-wrap gap-1.5 mt-auto overflow-hidden max-h-[46px]">
                            {group.skills.slice(0, 4).map((skillName) => {
                              const skill = skills.find((s) => s.name === skillName);
                              return (
                                <Badge
                                  key={skillName}
                                  variant={skill ? "outline" : "outline"}
                                  className={cn(
                                    "text-[10px] font-medium px-2 py-0.5 h-5",
                                    skill ? "bg-muted text-muted-foreground border-transparent" : "text-warning bg-warning/5 border-warning/20 font-normal"
                                  )}
                                >
                                  {skillName}
                                </Badge>
                              );
                            })}
                            {group.skills.length > 4 && (
                              <Badge
                                variant="outline"
                                className="text-[10px] font-medium px-2 py-0.5 h-5 bg-muted text-muted-foreground border-transparent"
                              >
                                +{group.skills.length - 4}
                              </Badge>
                            )}
                          </div>
                        </div>

                        {/* Footer section */}
                        <div className="px-4 py-2.5 border-t border-border/50 mt-auto flex flex-wrap items-center justify-between rounded-b-xl gap-x-2 gap-y-3 min-h-[44px]">
                          <div className="flex items-center gap-2 shrink-0">
                            <span className="text-xs font-medium text-muted-foreground whitespace-nowrap">
                              {t("skillCards.skillsCount", { count: group.skills.length })}
                            </span>
                          </div>

                          {/* Agent link icons + Deploy */}
                          <div className="flex items-center gap-1.5">
                            {/* Link to Agent icons */}
                            {enabledProfiles.map((profile) => {
                              const key = `${group.id}::${profile.id}`;
                              const state = linkState[key];
                              const installedSkillNames = group.skills.filter((name) =>
                                skillByName.has(name)
                              );
                              const linkedCount = installedSkillNames.filter((name) =>
                                skillByName
                                  .get(name)
                                  ?.agent_links?.includes(profile.display_name)
                              ).length;
                              const allLinked =
                                installedSkillNames.length > 0 &&
                                linkedCount === installedSkillNames.length;
                              const partialLinked =
                                linkedCount > 0 && linkedCount < installedSkillNames.length;
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
                                      installedSkillNames,
                                      allLinked
                                    );
                                  }}
                                  disabled={linking || installedSkillNames.length === 0}
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
                                    "w-7 h-7 rounded-lg flex items-center justify-center border transition-all cursor-pointer",
                                    allLinked
                                      ? "border-primary/20 bg-primary/5 shadow-sm"
                                      : linking
                                        ? "border-primary/30 bg-primary/5 opacity-60"
                                        : partialLinked
                                          ? "border-warning/30 bg-warning/5"
                                        : "border-transparent hover:bg-muted hover:border-border text-muted-foreground"
                                  )}
                                >
                                  <img
                                    src={`/${profile.icon}`}
                                    alt={profile.display_name}
                                    className={cn(
                                      "w-3.5 h-3.5 transition-[filter,opacity] duration-300",
                                      linking && "animate-pulse",
                                      !allLinked && !partialLinked &&
                                        "grayscale opacity-40 hover:opacity-80 hover:grayscale-0"
                                    )}
                                  />
                                </button>
                              );
                            })}

                            {/* Separator */}
                            {enabledProfiles.length > 0 && (
                              <div className="w-px h-4 bg-border mx-0.5" />
                            )}

                            {/* Deploy to project */}
                            <Button
                              size="sm"
                              className="h-7 px-3 text-xs group/btn bg-primary hover:bg-primary/90"
                              onClick={(e) => {
                                e.stopPropagation();
                                onNavigateToProjects?.(group.skills);
                              }}
                            >
                              <Rocket className="w-3 h-3 mr-1.5 transition-transform group-hover/btn:-translate-y-[1px] group-hover/btn:translate-x-[1px]" />
                              {t("skillCards.deploy")}
                            </Button>
                          </div>
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
            });
          } else {
            await createGroup(name, desc, icon, selectedSkills);
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
