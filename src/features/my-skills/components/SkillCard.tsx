import {
  AlertTriangle,
  ArrowRightLeft,
  ArrowUpCircle,
  Check,
  Download,
  GitBranch,
  HardDrive,
  Loader2,
  Star,
} from "lucide-react";
import { memo } from "react";
import { useTranslation } from "react-i18next";
import { AgentTargetCarousel } from "../../../components/shared/AgentTargetCarousel";
import { AgentIcon } from "../../../components/ui/AgentIcon";
import { Button } from "../../../components/ui/button";
import { selectTargetableAgentProfiles } from "../../../lib/agentProfiles";
import { handleExternalAnchorClick } from "../../../lib/externalOpen";
import { tauriInvoke } from "../../../lib/ipc";
import { agentIconCls, cn, formatInstalls } from "../../../lib/utils";
import type { AgentProfile, Skill } from "../../../types";
import { SkillAvatar } from "./SkillAvatar";

export type SkillCardRemoteContext = {
  agentProfile: AgentProfile;
  sizeLabel?: string;
  /** Click the agent badge to filter the grid by this agent (stops the card from opening). */
  onAgentClick?: () => void;
  /** Whether this agent is the active filter — highlights the badge. */
  agentActive?: boolean;
};

export interface SkillCardProps {
  skill: Skill;
  onClick: (skill: Skill) => void;
  /** Optional: remote cards never call these. */
  onInstall?: (url: string, name: string, agentId?: string) => void;
  onUpdate?: (name: string) => void;
  /** Upstream dropped the Skill with no successor: keep a local copy or remove. */
  onResolveRemoved?: (name: string) => void;
  /** Upstream renamed the Skill: install the successor, carry deployments over, remove this one. */
  onMigrate?: (name: string) => void;
  migrating?: boolean;
  compact?: boolean;
  selectable?: boolean;
  selected?: boolean;
  onSelect?: (name: string) => void;
  profiles?: AgentProfile[];
  onToggleAgent?: (skillName: string, agentId: string, enable: boolean, agentName?: string) => void;
  pendingAgentToggleKeys?: Set<string>;
  installing?: boolean;
  updating?: boolean;
  /** Accepted for callers; enter/exit is owned by SkillGrid. */
  noAnimate?: boolean;
  /** SSH remote page: same chrome as library cards, delete + single agent footer. */
  remoteContext?: SkillCardRemoteContext;
}

function SkillCardInner({
  skill,
  onClick,
  onInstall,
  onUpdate,
  onResolveRemoved,
  onMigrate,
  migrating,
  compact,
  selectable,
  selected,
  onSelect,
  profiles,
  onToggleAgent,
  pendingAgentToggleKeys,
  installing,
  updating,
  remoteContext,
}: SkillCardProps) {
  const { t } = useTranslation();
  const isLocalSkill = skill.skill_type === "local";
  const isRemoteCard = Boolean(remoteContext);
  const isLibrary = isRemoteCard || Boolean(selectable);

  const handleCheckboxClick = (e: React.MouseEvent) => {
    e.stopPropagation();
    onSelect?.(skill.name);
  };

  const stopCard = (e: React.SyntheticEvent) => {
    e.stopPropagation();
  };

  const upstreamChange = !isRemoteCard && !isLocalSkill ? (skill.upstream_change ?? null) : null;

  let statusAction: React.ReactNode = null;
  if (upstreamChange?.kind === "removed" && upstreamChange.successor) {
    const successor = upstreamChange.successor;
    statusAction = (
      <Button
        size="xs"
        variant="ghost"
        className={cn(
          "group/migrate relative h-6 max-w-[11rem] px-2.5 rounded-full text-xs font-semibold tracking-tight transition-all duration-200 cursor-pointer shadow-2xs select-none",
          "bg-violet-500/15 text-violet-300 border border-violet-500/35",
          "paper:bg-violet-500/10 paper:text-violet-800 paper:border-violet-500/30",
          "hover:bg-violet-500/25 hover:text-violet-100 hover:border-violet-500/60 hover:-translate-y-0.5",
          "paper:hover:bg-violet-500/20 paper:hover:text-violet-950 paper:hover:border-violet-500/50",
          "active:translate-y-0 active:scale-95 focus-visible:ring-2 focus-visible:ring-violet-500/40",
          migrating && "opacity-75 cursor-not-allowed pointer-events-none",
        )}
        disabled={migrating}
        title={t("skillCard.upstreamRenamed", { name: successor.skill_id })}
        onClick={(e) => {
          stopCard(e);
          onMigrate?.(skill.name);
        }}
        onMouseDown={stopCard}
      >
        {migrating ? (
          <Loader2 className="w-3 h-3 mr-1 animate-spin shrink-0" />
        ) : (
          <ArrowRightLeft className="w-3 h-3 mr-1 shrink-0 transition-transform duration-200 group-hover/migrate:translate-x-0.5" />
        )}
        <span className="truncate">
          {migrating ? t("skillCard.migrating") : t("skillCard.migrate", { name: successor.skill_id })}
        </span>
      </Button>
    );
  } else if (upstreamChange?.kind === "removed") {
    statusAction = (
      <Button
        size="xs"
        variant="ghost"
        className={cn(
          "relative h-6 px-2.5 rounded-full text-xs font-semibold tracking-tight transition-all duration-200 cursor-pointer shadow-2xs select-none",
          "bg-rose-500/15 text-rose-300 border border-rose-500/35",
          "paper:bg-rose-500/10 paper:text-rose-800 paper:border-rose-500/30",
          "hover:bg-rose-500/25 hover:text-rose-100 hover:border-rose-500/60 hover:-translate-y-0.5",
          "paper:hover:bg-rose-500/20 paper:hover:text-rose-950 paper:hover:border-rose-500/50",
          "active:translate-y-0 active:scale-95 focus-visible:ring-2 focus-visible:ring-rose-500/40",
        )}
        title={t("skillCard.resolveRemoved")}
        onClick={(e) => {
          stopCard(e);
          onResolveRemoved?.(skill.name);
        }}
        onMouseDown={stopCard}
      >
        <AlertTriangle className="w-3 h-3 mr-1 shrink-0" />
        <span>{t("skillCard.upstreamRemoved")}</span>
      </Button>
    );
  } else if (!isRemoteCard && skill.update_available && !isLocalSkill) {
    statusAction = (
      <Button
        size="xs"
        variant="ghost"
        className={cn(
          "group/update relative h-6 px-2.5 rounded-full text-xs font-semibold tracking-tight transition-all duration-200 cursor-pointer shadow-2xs select-none",
          "bg-amber-500/15 text-amber-300 border border-amber-500/35",
          "paper:bg-amber-500/10 paper:text-amber-800 paper:border-amber-500/30",
          "hover:bg-amber-500/25 hover:text-amber-100 hover:border-amber-500/60 hover:shadow-[0_2px_10px_-2px_rgba(245,158,11,0.4)] hover:-translate-y-0.5",
          "paper:hover:bg-amber-500/20 paper:hover:text-amber-950 paper:hover:border-amber-500/50 paper:hover:shadow-[0_2px_8px_-2px_rgba(217,119,6,0.25)]",
          "active:translate-y-0 active:scale-95",
          "focus-visible:ring-2 focus-visible:ring-amber-500/40",
          updating && "opacity-75 cursor-not-allowed pointer-events-none",
        )}
        disabled={updating}
        onClick={(e) => {
          stopCard(e);
          void onUpdate?.(skill.name);
        }}
        onMouseDown={stopCard}
      >
        {updating ? (
          <Loader2 className="w-3 h-3 mr-1 animate-spin shrink-0 text-amber-400 paper:text-amber-700" />
        ) : (
          <span className="relative flex items-center justify-center mr-1">
            <ArrowUpCircle className="w-3 h-3 shrink-0 text-amber-400 paper:text-amber-600 transition-transform duration-200 group-hover/update:-translate-y-0.5" />
            <span className="absolute -top-0.5 -right-0.5 flex h-1.5 w-1.5">
              <span className="animate-ping absolute inline-flex h-full w-full rounded-full bg-amber-400 opacity-75" />
              <span className="relative inline-flex rounded-full h-1.5 w-1.5 bg-amber-500" />
            </span>
          </span>
        )}
        <span>{updating ? t("common.updating", { defaultValue: "Updating..." }) : t("common.update")}</span>
      </Button>
    );
  } else if (!isRemoteCard && !skill.installed && installing) {
    statusAction = (
      <Button size="sm" variant="outline" className="h-6 px-2 text-xs font-medium pointer-events-none" disabled>
        <Loader2 className="w-3 h-3 mr-1 animate-spin" />
        {t("common.installing")}
      </Button>
    );
  } else if (!isRemoteCard && !skill.installed) {
    statusAction = (
      <Button
        size="sm"
        variant="default"
        className="h-6 px-2.5 text-xs font-medium"
        onClick={(e) => {
          stopCard(e);
          onInstall?.(skill.git_url, skill.name);
        }}
      >
        <Download className="w-3 h-3 mr-1" />
        {t("common.install")}
      </Button>
    );
  } else if (!isLibrary && skill.installed) {
    statusAction = <span className="text-xs text-foreground/60 select-none">{t("skillCard.installed")}</span>;
  }

  const sourceBadge = (() => {
    if (isRemoteCard) return null;
    if (isLocalSkill) {
      return (
        <button
          type="button"
          onClick={async (e) => {
            e.stopPropagation();
            try {
              await tauriInvoke("open_skill_folder", { name: skill.name });
            } catch (err) {
              if (import.meta.env.DEV) console.error("Failed to open local skill folder:", err);
            }
          }}
          className="inline-flex items-center gap-1 px-1.5 py-0.5 rounded-md text-[10px] font-semibold bg-emerald-500/15 text-emerald-600 dark:text-emerald-400 border border-emerald-500/30 hover:bg-emerald-500/25 hover:border-emerald-500/50 hover:text-emerald-700 dark:hover:text-emerald-300 transition-colors cursor-pointer group/local shadow-2xs"
          title={t("skillCard.openLocalDir", { label: t("toolbar.local") })}
        >
          <HardDrive className="w-2.5 h-2.5 group-hover/local:scale-110 transition-transform" />
          <span className="group-hover/local:underline">{t("toolbar.local")}</span>
        </button>
      );
    }
    if (skill.source && skill.source !== "remote") {
      const repoUrl =
        skill.git_url && skill.git_url.startsWith("http") ? skill.git_url : `https://github.com/${skill.source}`;

      return (
        <a
          href={repoUrl}
          target="_blank"
          rel="noopener noreferrer"
          onClick={(e) => {
            e.stopPropagation();
            handleExternalAnchorClick(e, repoUrl);
          }}
          className="inline-flex items-center gap-1 px-1.5 py-0.5 rounded-md text-[10px] font-medium bg-muted/80 text-foreground/80 border border-border/70 truncate max-w-[180px] hover:bg-muted hover:text-foreground hover:border-primary/50 transition-colors cursor-pointer group/repo shadow-2xs"
          title={t("skillCard.openRepo", { source: skill.source })}
        >
          <GitBranch className="w-2.5 h-2.5 shrink-0 opacity-70 group-hover/repo:text-primary transition-colors" />
          <span className="truncate group-hover/repo:underline">{skill.source}</span>
        </a>
      );
    }
    if (skill.author) {
      const authorUrl = `https://github.com/${skill.author.replace(/^@/, "")}`;
      return (
        <a
          href={authorUrl}
          target="_blank"
          rel="noopener noreferrer"
          onClick={(e) => {
            e.stopPropagation();
            handleExternalAnchorClick(e, authorUrl);
          }}
          className="inline-flex items-center px-1.5 py-0.5 rounded-md text-[10px] font-medium bg-muted/70 text-foreground/75 border border-border/60 hover:bg-muted hover:text-foreground hover:border-primary/50 transition-colors cursor-pointer group/author shadow-2xs"
          title={t("skillCard.openAuthor", { author: skill.author })}
        >
          <span className="group-hover/author:underline">@{skill.author}</span>
        </a>
      );
    }
    return null;
  })();

  const targetableProfiles = profiles ? selectTargetableAgentProfiles(profiles) : [];

  let agentRail: React.ReactNode = null;
  if (remoteContext) {
    agentRail = (
      <button
        type="button"
        onClick={(e) => {
          stopCard(e);
          remoteContext.onAgentClick?.();
        }}
        disabled={!remoteContext.onAgentClick}
        aria-pressed={remoteContext.agentActive}
        title={remoteContext.agentProfile.display_name}
        className={cn(
          "w-7 h-7 shrink-0 rounded-lg flex items-center justify-center border transition-all focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-primary/45",
          remoteContext.onAgentClick && "cursor-pointer",
          remoteContext.agentActive
            ? "border-primary/60 bg-primary/25 shadow-sm"
            : "border-primary/40 bg-primary/10 hover:bg-primary/20",
        )}
      >
        <AgentIcon
          profile={remoteContext.agentProfile}
          className={cn(agentIconCls(remoteContext.agentProfile.icon, "w-4 h-4"), "drop-shadow-sm")}
        />
      </button>
    );
  } else if (targetableProfiles.length > 0 && (onToggleAgent || onInstall)) {
    agentRail = (
      <AgentTargetCarousel
        items={targetableProfiles.map((profile) => {
          const linked = skill.installed && (skill.agent_links?.includes(profile.display_name) ?? false);
          const title = linked
            ? `${profile.display_name} (${t("skillCard.remove")})`
            : t("skillCard.installFor", { agent: profile.display_name });
          return {
            id: profile.id,
            profile,
            selected: linked,
            pending: pendingAgentToggleKeys?.has(`${skill.name}::${profile.id}`) ?? false,
            title,
          };
        })}
        onToggle={({ profile, selected }) => {
          if (selected === true) {
            onToggleAgent?.(skill.name, profile.id, false, profile.display_name);
            return;
          }
          if (skill.git_url && onInstall) {
            onInstall(skill.git_url, skill.name, profile.id);
            return;
          }
          onToggleAgent?.(skill.name, profile.id, true, profile.display_name);
        }}
      />
    );
  }

  const stars =
    !isLibrary && skill.stars > 0 ? (
      <span className="inline-flex items-center gap-1 text-[11px] font-semibold text-amber-500 dark:text-amber-400 tabular-nums">
        <Star className="w-3 h-3 fill-amber-500/30" />
        {formatInstalls(skill.stars)}
      </span>
    ) : null;

  const remoteSize = remoteContext?.sizeLabel ? (
    <span className="text-[11px] font-semibold text-muted-foreground tabular-nums">{remoteContext.sizeLabel}</span>
  ) : null;

  const descText = skill.localized_description || skill.description || t("skillCard.noDescription");

  return (
    <div
      onClick={() => onClick(skill)}
      className={cn(
        "group relative h-full flex flex-col justify-between rounded-[20px] border border-border/80 bg-card/70 backdrop-blur-md cursor-pointer transition-all duration-200 overflow-hidden",
        "hover:bg-card-hover hover:border-primary/50 hover:-translate-y-1 hover:shadow-[0_12px_32px_-8px_var(--color-shadow)]",
        "border-t border-t-white/20 paper:border-t-black/10",
        selected &&
          "ring-2 ring-primary border-primary bg-primary/10 shadow-[0_0_24px_-4px_rgba(var(--color-primary-rgb),0.4)]",
        compact && "p-2",
      )}
    >
      {/* Top Card Body */}
      <div className="p-3.5 pb-2 flex-1 flex flex-col gap-2">
        {/* Header Row: Checkbox + Avatar + Title + Status */}
        <div className="flex items-start gap-2.5 min-w-0">
          {/* Avatar with integrated selection checkbox */}
          <div className="relative shrink-0 group/avatar">
            <SkillAvatar
              skill={skill}
              size="md"
              className={cn(
                selectable && "cursor-pointer",
                selected && "ring-2 ring-primary ring-offset-1 ring-offset-background",
              )}
            />
            {selectable ? (
              <button
                type="button"
                onClick={handleCheckboxClick}
                aria-pressed={selected}
                aria-label={skill.name}
                className={cn(
                  "absolute inset-0 flex items-center justify-center rounded-xl transition-all duration-150 cursor-pointer",
                  selected
                    ? "bg-primary text-primary-foreground opacity-100 shadow-xs"
                    : "bg-background/60 backdrop-blur-xs text-foreground/80 opacity-0 group-hover:opacity-100 hover:bg-background/80 hover:text-primary",
                )}
              >
                {selected ? (
                  <Check className="h-4 w-4 stroke-[3]" />
                ) : (
                  <div className="w-4 h-4 rounded-md border border-foreground/40 hover:border-primary flex items-center justify-center bg-background/50">
                    <Check className="h-3 w-3 opacity-0 hover:opacity-70 stroke-[2.5]" />
                  </div>
                )}
              </button>
            ) : null}
          </div>

          {/* Title & Badge */}
          <div className="min-w-0 flex-1">
            <div className="flex items-center gap-1.5 min-w-0">
              {!isLibrary && skill.rank && skill.rank <= 100 ? (
                <span className="text-[10px] font-bold text-amber-500 px-1 py-0.2 rounded bg-amber-500/15 border border-amber-500/30 tabular-nums shrink-0">
                  #{skill.rank}
                </span>
              ) : null}
              <h3 className="text-sm font-bold tracking-tight text-foreground truncate group-hover:text-primary transition-colors">
                {skill.name}
              </h3>
            </div>
            <div className="mt-1 flex items-center gap-1.5 flex-wrap">{sourceBadge}</div>
          </div>

          {/* Action / Install / Update Slot */}
          {statusAction && <div className="shrink-0">{statusAction}</div>}
        </div>

        {/* Description */}
        <p className="text-xs text-muted-foreground/90 line-clamp-2 leading-relaxed mt-0.5">{descText}</p>
      </div>

      {/* Footer / Agent Target Rail */}
      {(stars || remoteSize || agentRail) && (
        <div className="px-3.5 py-2 border-t border-border/40 bg-muted/30 flex items-center justify-between min-h-[42px] gap-2 rounded-b-[20px]">
          <div className="flex items-center gap-2 shrink-0">
            {remoteSize}
            {stars}
          </div>
          <div className="flex items-center gap-1.5 relative z-10 flex-1 min-w-0 justify-end">{agentRail}</div>
        </div>
      )}
    </div>
  );
}

export const SkillCard = memo(SkillCardInner);
