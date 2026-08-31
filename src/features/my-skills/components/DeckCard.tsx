import { motion } from "framer-motion";
import { AlertTriangle, Copy, Download, Edit2, Loader2, MoreHorizontal, Rocket, Share2, Trash2 } from "lucide-react";
import { useTranslation } from "react-i18next";
import { MOTION_TRANSITION } from "../../../comm/motion";
import { AgentTargetCarousel } from "../../../components/shared/AgentTargetCarousel";
import { Badge } from "../../../components/ui/badge";
import { Button } from "../../../components/ui/button";
import { CardTemplate } from "../../../components/ui/card-template";
import { cn } from "../../../lib/utils";
import type { AgentProfile, Skill, SkillCardDeck, ViewMode } from "../../../types";

interface DeckCardProps {
  group: SkillCardDeck;
  viewMode: ViewMode;
  /** Normalized (deduped) skill names belonging to this deck. */
  groupSkillNames: string[];
  /** Subset of groupSkillNames that are currently installed. */
  groupInstalledSkillNames: string[];
  /** Installed-skill lookup (normalized name → skill), from the hub skills list. */
  skillByName: Map<string, Skill>;
  /** Enabled agent profiles shown in the footer's link-toggle row. */
  enabledProfiles: AgentProfile[];
  /** Batch-toggle state: `${groupId}::${agentId}` → "linking". */
  linkState: Record<string, "linking">;
  /** Group id currently installing its missing skills, or null. */
  installingMissing: string | null;
  installProgress: { done: number; total: number } | null;
  /** Group id whose context menu is open, or null. */
  menuOpenId: string | null;
  onMenuOpenChange: (id: string | null) => void;
  onEdit: (group: SkillCardDeck) => void;
  onExport: (group: SkillCardDeck) => void;
  onDuplicate: (id: string) => void;
  onDelete: (id: string) => void;
  onInstallMissing: (group: SkillCardDeck) => void;
  onToggleGroupAgentLinks: (
    group: SkillCardDeck,
    agentId: string,
    agentName: string,
    installedSkillNames: string[],
    allLinked: boolean,
  ) => void;
  onDeploy: (skillNames: string[]) => void;
}

export function DeckCard({
  group,
  viewMode,
  groupSkillNames,
  groupInstalledSkillNames,
  skillByName,
  enabledProfiles,
  linkState,
  installingMissing,
  installProgress,
  menuOpenId,
  onMenuOpenChange,
  onEdit,
  onExport,
  onDuplicate,
  onDelete,
  onInstallMissing,
  onToggleGroupAgentLinks,
  onDeploy,
}: DeckCardProps) {
  const { t } = useTranslation();
  const installedCount = groupInstalledSkillNames.length;
  const totalCount = groupSkillNames.length;
  const missingCount = totalCount - installedCount;
  const isInstallingThis = installingMissing === group.id;
  const maxSkillsToShow = viewMode === "grid" ? 5 : 999;
  // The rail reflects the deck's own Agent links, not the links its Skills
  // happen to have: a new deck starts dark even though installing a Skill
  // already deploys it to every enabled Agent.
  const deckAgentLinks = new Set(group.agent_links ?? []);

  return (
    <motion.div
      initial={{ opacity: 0, scale: 0.95 }}
      animate={{ opacity: 1, scale: 1 }}
      exit={{ opacity: 0, scale: 0.95 }}
      transition={MOTION_TRANSITION.enter}
      className={cn(
        "relative transition-shadow",
        viewMode === "grid" ? "min-h-[200px]" : "min-h-[140px]",
        menuOpenId === group.id ? "z-50" : "z-0 hover:z-10",
      )}
    >
      <CardTemplate
        className={cn(
          "hover:bg-card-hover flex relative group shadow-sm hover:shadow-[0_12px_32px_-8px_var(--color-shadow)] transition-all duration-200 p-0 border border-border/80 bg-card/70 hover:border-primary/50 hover:-translate-y-0.5",
          viewMode === "list" ? "flex-row items-center min-h-[96px]" : "flex-col h-full",
        )}
      >
        <div
          className={cn(
            "ss-card-body flex flex-1 relative min-h-0",
            viewMode === "list" ? "flex-row items-center py-4" : "flex-col",
          )}
        >
          {/* Header section */}
          <div className={cn("flex items-start gap-4 pr-4 shrink-0", viewMode === "grid" ? "mb-5" : "w-[220px]")}>
            <div className="w-12 h-12 rounded-xl bg-primary/12 border border-primary/25 flex items-center justify-center text-2xl shrink-0 shadow-2xs">
              {group.icon}
            </div>
            <div className="min-w-0 pt-1">
              <h3 className="ss-card-title truncate text-foreground transition-colors font-bold">
                <button
                  type="button"
                  onClick={() => onEdit(group)}
                  className="w-full text-left truncate rounded outline-none focus-visible:ring-2 focus-visible:ring-primary hover:text-primary cursor-pointer transition-colors"
                >
                  {group.name}
                </button>
              </h3>
              {group.description ? (
                <p className="ss-card-desc mt-1 leading-relaxed text-muted-foreground/90">{group.description}</p>
              ) : (
                <p className="ss-card-meta italic mt-1 opacity-60">{t("skillCards.noDescription")}</p>
              )}
            </div>
          </div>

          {/* Skills Preview Tags */}
          <div
            className={cn(
              "flex flex-wrap items-center gap-1.5",
              viewMode === "grid" ? "mt-auto overflow-hidden max-h-[46px]" : "ml-2 flex-1 py-1",
            )}
          >
            {groupSkillNames.slice(0, maxSkillsToShow).map((skillName) => {
              const skill = skillByName.get(skillName);
              return (
                <Badge
                  key={skillName}
                  variant="outline"
                  className={cn(
                    "text-micro font-semibold px-2 py-0.5 h-5",
                    skill
                      ? "bg-muted/80 text-foreground/80 border-border/70 shadow-2xs"
                      : "text-amber-500 bg-amber-500/12 border-amber-500/30 font-medium shadow-2xs",
                  )}
                >
                  {skillName}
                </Badge>
              );
            })}
            {groupSkillNames.length > maxSkillsToShow && (
              <Badge
                variant="outline"
                className="text-micro font-semibold px-2 py-0.5 h-5 bg-muted/80 text-muted-foreground border-border/70"
              >
                +{groupSkillNames.length - maxSkillsToShow}
              </Badge>
            )}
          </div>

          {/* Top Action Row (Context Menu) */}
          <div
            className={cn(
              "z-20 flex items-center gap-1 shrink-0",
              viewMode === "list" ? "pr-4 pl-2" : "absolute top-4 right-4",
            )}
          >
            <button
              onClick={(e) => {
                e.stopPropagation();
                onExport(group);
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
                  onMenuOpenChange(menuOpenId === group.id ? null : group.id);
                }}
                className="p-2.5 rounded-lg hover:bg-muted text-muted-foreground transition-colors outline-none focus-visible:ring-2 focus-visible:ring-primary mt-0.5"
                aria-label={t("common.moreActions")}
              >
                <MoreHorizontal className="w-4 h-4" />
              </button>

              {menuOpenId === group.id && (
                <motion.div
                  initial={{ opacity: 0, scale: 0.95 }}
                  animate={{ opacity: 1, scale: 1 }}
                  transition={MOTION_TRANSITION.fadeFast}
                  className="absolute right-0 top-full mt-1 w-32 p-1 rounded-xl border border-border bg-card backdrop-blur-xl shadow-xl z-30"
                >
                  <button
                    onClick={() => {
                      onEdit(group);
                      onMenuOpenChange(null);
                    }}
                    className="w-full flex items-center gap-2 px-3 py-1.5 rounded-lg text-xs hover:bg-card-hover transition-colors cursor-pointer"
                  >
                    <Edit2 className="w-3 h-3" />
                    {t("common.edit")}
                  </button>
                  <button
                    onClick={() => onDuplicate(group.id)}
                    className="w-full flex items-center gap-2 px-3 py-1.5 rounded-lg text-xs hover:bg-card-hover transition-colors cursor-pointer"
                  >
                    <Copy className="w-3 h-3" />
                    {t("common.duplicate")}
                  </button>
                  <button
                    onClick={() => onDelete(group.id)}
                    className="w-full flex items-center gap-2 px-3 py-1.5 rounded-lg text-xs text-destructive hover:bg-destructive/10 transition-colors cursor-pointer"
                  >
                    <Trash2 className="w-3 h-3" />
                    {t("common.delete")}
                  </button>
                </motion.div>
              )}
            </div>
          </div>
        </div>

        {/* Footer section */}
        <div
          className={cn(
            "border-border/50 flex items-center shrink-0",
            viewMode === "grid" ? "ss-card-footer mt-auto rounded-b-xl" : "pl-6 pr-16 py-4 border-l w-[320px]",
          )}
        >
          {installedCount === 0 ? (
            /* All skills missing — show warning */
            <div className="flex items-center gap-2 flex-1 min-w-0">
              <AlertTriangle className="w-3.5 h-3.5 text-warning shrink-0" />
              <span className="text-xs text-muted-foreground truncate">
                {t("skillCards.noSkillsInstalled", { defaultValue: "No cards installed" })}
                <span className="ml-1.5 opacity-60 tabular-nums">
                  ({installedCount}/{totalCount})
                </span>
              </span>
              {missingCount > 0 && (
                <Button
                  size="sm"
                  variant="ghost"
                  className="h-7 px-3 text-xs ml-auto text-muted-foreground hover:text-foreground shrink-0"
                  disabled={isInstallingThis}
                  onClick={(e) => {
                    e.stopPropagation();
                    onInstallMissing(group);
                  }}
                >
                  {isInstallingThis ? <Loader2 className="w-3 h-3 animate-spin" /> : <Download className="w-3 h-3" />}
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
              <AgentTargetCarousel
                items={enabledProfiles.map((profile) => {
                  const claimed = deckAgentLinks.has(profile.id);
                  const linkedCount = groupInstalledSkillNames.filter((name) =>
                    skillByName.get(name)?.agent_links?.includes(profile.display_name),
                  ).length;
                  // Only a claimed Agent can light up. Claimed but not fully
                  // linked on disk (a Skill was added or unlinked since) shows
                  // as mixed, so drift stays visible instead of reading as done.
                  const allLinked =
                    claimed && groupInstalledSkillNames.length > 0 && linkedCount === groupInstalledSkillNames.length;
                  return {
                    id: profile.id,
                    profile,
                    selected: allLinked ? true : claimed ? ("mixed" as const) : false,
                    pending: linkState[`${group.id}::${profile.id}`] === "linking",
                    disabled: groupInstalledSkillNames.length === 0,
                    title: allLinked
                      ? t("skillCards.unlinkAllFrom", { agent: profile.display_name })
                      : t("skillCards.linkAllTo", { agent: profile.display_name }),
                  };
                })}
                onToggle={({ profile, selected }) => {
                  onToggleGroupAgentLinks(
                    group,
                    profile.id,
                    profile.display_name,
                    groupInstalledSkillNames,
                    selected === true,
                  );
                }}
              />

              {/* Separator */}
              {enabledProfiles.length > 0 && <div className="w-px h-4 bg-border mx-0.5 ml-auto shrink-0" />}

              {missingCount > 0 && (
                <Button
                  size="sm"
                  variant="outline"
                  className="h-7 px-2 text-xs border-warning/20 bg-warning/5 text-warning hover:bg-warning/10 hover:text-warning shrink-0"
                  onClick={(e) => {
                    e.stopPropagation();
                    onInstallMissing(group);
                  }}
                  disabled={isInstallingThis}
                  title={t("skillCards.installMissing", {
                    count: missingCount,
                    defaultValue: `Install missing (${missingCount})`,
                  })}
                >
                  {isInstallingThis ? (
                    <Loader2 className="w-3 h-3 animate-spin mr-1" />
                  ) : (
                    <Download className="w-3 h-3 mr-1" />
                  )}
                  <span className="tabular-nums font-medium">
                    {isInstallingThis && installProgress
                      ? `${installProgress.done}/${installProgress.total}`
                      : `${installedCount}/${totalCount}`}
                  </span>
                </Button>
              )}

              {/* Deploy to project */}
              <Button
                size="sm"
                className="h-7 w-7 p-0 shrink-0 group/btn bg-primary hover:bg-primary/90"
                onClick={(e) => {
                  e.stopPropagation();
                  onDeploy(groupSkillNames);
                }}
                title={t("skillCards.deploy")}
                aria-label={t("skillCards.deploy")}
              >
                <Rocket className="w-3.5 h-3.5 transition-transform group-hover/btn:-translate-y-[1px] group-hover/btn:translate-x-[1px]" />
              </Button>
            </div>
          )}
        </div>
      </CardTemplate>
    </motion.div>
  );
}
