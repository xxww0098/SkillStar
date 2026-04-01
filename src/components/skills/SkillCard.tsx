import { memo } from "react";
import { motion } from "framer-motion";
import { Download, GitBranch, Check, ExternalLink, Loader2, ShieldCheck, ShieldAlert, ShieldX } from "lucide-react";
import { SuccessCheckmark } from "../ui/SuccessCheckmark";
import { useTranslation } from "react-i18next";
import { Card, CardHeader, CardTitle, CardDescription, CardContent, CardFooter } from "../ui/card";
import { Badge } from "../ui/badge";
import { Button } from "../ui/button";
import { AgentIcon } from "../ui/AgentIcon";
import { HScrollRow } from "../ui/HScrollRow";
import { cn, formatInstalls, agentIconCls } from "../../lib/utils";
import type { Skill, AgentProfile, RiskLevel } from "../../types";

interface SkillCardProps {
  skill: Skill;
  onClick: () => void;
  onInstall: (url: string, name: string) => void;
  onUpdate: (name: string) => void;
  compact?: boolean;
  selectable?: boolean;
  selected?: boolean;
  onSelect?: (name: string) => void;
  profiles?: AgentProfile[];
  onToggleAgent?: (skillName: string, agentId: string, enable: boolean, agentName?: string) => void;
  pendingAgentToggleKeys?: Set<string>;
  /** Whether this skill is currently being installed */
  installing?: boolean;
  /** Whether this skill is currently being updated */
  updating?: boolean;
  /** Disable mount animation (use with stagger containers) */
  noAnimate?: boolean;
  /** Security scan risk level (undefined = not scanned) */
  riskLevel?: RiskLevel;
}



/** Get rank badge color based on position */
function rankStyle(rank: number): string {
  if (rank === 1) return "bg-amber-400/20 text-amber-600 border-amber-400/40 font-bold";
  if (rank === 2) return "bg-amber-500/20 text-amber-600 border-amber-500/40 font-bold dark:text-amber-400";
  if (rank === 3) return "bg-orange-400/15 text-orange-500 border-orange-400/30 font-bold";
  if (rank <= 10) return "bg-primary/8 text-primary/80 border-primary/20 font-semibold";
  return "bg-muted text-muted-foreground border-transparent font-medium";
}

const categoryBadge = (category: string, t: (key: string) => string) => {
  const map: Record<string, { label: string; variant: "hot" | "popular" | "rising" | "new" }> = {
    Hot: { label: t("skillCard.hot"), variant: "hot" },
    Popular: { label: t("skillCard.popular"), variant: "popular" },
    Rising: { label: t("skillCard.rising"), variant: "rising" },
    New: { label: t("skillCard.new"), variant: "new" },
  };
  return map[category];
};

function SkillCardInner({
  skill, onClick, onInstall, onUpdate, compact, selectable, selected, onSelect,
  profiles, onToggleAgent, pendingAgentToggleKeys, installing, updating, noAnimate,
  riskLevel,
}: SkillCardProps) {
  const { t } = useTranslation();
  const cat = categoryBadge(skill.category, t);
  const isLocalSkill = skill.skill_type === "local";

  const handleCheckboxClick = (e: React.MouseEvent) => {
    e.stopPropagation();
    onSelect?.(skill.name);
  };

  const Wrapper = noAnimate ? "div" : motion.div;
  const wrapperProps = noAnimate ? {} : {
    initial: { opacity: 0 },
    animate: { opacity: 1 },
    transition: { duration: 0.15 },
  };

  return (
    <Wrapper {...wrapperProps} className="h-full">
        <Card
        className={cn(
          "h-full flex flex-col cursor-pointer group relative rounded-2xl bg-card border-border shadow-[0_4px_20px_-8px_var(--color-shadow)] hover:bg-card-hover hover:shadow-[0_8px_30px_-10px_var(--color-shadow)] transition",
          compact && "p-2",
          selected && "ring-2 ring-primary/40 border-primary/30"
        )}
        onClick={onClick}
      >
        {/* Rank badge (top-left) */}
        {skill.rank && skill.rank <= 100 && (
          <div
            className={cn(
              "absolute top-3 left-3 z-10 w-7 h-7 rounded-lg border flex items-center justify-center leading-none tabular-nums",
              skill.rank >= 100 ? "text-[10px] tracking-tight" : "text-micro",
              rankStyle(skill.rank)
            )}
          >
            {skill.rank}
          </div>
        )}

        {/* Security risk badge (bottom-left) */}
        {riskLevel && (
          <div
            className={cn(
              "absolute bottom-3 left-3 z-10 w-5 h-5 rounded-md flex items-center justify-center",
              riskLevel === "Safe" && "bg-emerald-500/15 text-emerald-400",
              riskLevel === "Low" && "bg-amber-500/15 text-amber-300",
              riskLevel === "Medium" && "bg-orange-500/15 text-orange-400",
              riskLevel === "High" && "bg-red-500/15 text-red-400",
              riskLevel === "Critical" && "bg-red-500/20 text-red-500",
            )}
            title={`Security: ${riskLevel}`}
          >
            {riskLevel === "Safe" && <ShieldCheck size={12} />}
            {riskLevel === "Low" && <ShieldAlert size={12} />}
            {riskLevel === "Medium" && <ShieldAlert size={12} />}
            {riskLevel === "High" && <ShieldX size={12} />}
            {riskLevel === "Critical" && <ShieldX size={12} />}
          </div>
        )}



        {/* Status Actions (Top Right) */}
        <div className="absolute top-3 right-3 z-10 flex items-center">
          {skill.installed ? (
            skill.update_available && !isLocalSkill ? (
              <Button
                size="sm"
                variant="outline"
                className="h-7 px-2.5 text-xs bg-card hover:bg-muted font-medium border-warning text-warning-foreground shadow-[0_0_10px_rgba(234,179,8,0.1)] transition-colors"
                disabled={updating}
                onClick={(e) => {
                  e.stopPropagation();
                  void onUpdate?.(skill.name);
                }}
                onMouseDown={(e) => {
                  e.stopPropagation();
                }}
              >
                {updating ? (
                  <Loader2 className="w-3 h-3 mr-1.5 animate-spin" />
                ) : (
                  <span className="relative flex h-2 w-2 mr-1.5">
                    <span className="animate-ping-limited absolute inline-flex h-full w-full rounded-full bg-warning opacity-75"></span>
                    <span className="relative inline-flex rounded-full h-2 w-2 bg-warning"></span>
                  </span>
                )}
                {updating
                  ? t("common.updating", { defaultValue: "Updating..." })
                  : t("common.update")}
              </Button>
            ) : (
              <motion.div
                initial={{ scale: 0.8, opacity: 0 }}
                animate={{ scale: 1, opacity: 1 }}
                transition={{ duration: 0.4, ease: [0.16, 1, 0.3, 1] }}
              >
                <Button
                  size="sm"
                  variant="secondary"
                  className="h-7 px-2.5 text-xs font-medium pointer-events-none bg-success/10 text-success hover:bg-success/10 border-success/20 disabled:opacity-100"
                  disabled
                >
                  <SuccessCheckmark size={14} className="text-success mr-1" />
                   {t("skillCard.installed")}
                </Button>
              </motion.div>
            )
          ) : installing ? (
            <Button
              size="sm"
              variant="outline"
              className="h-7 px-2.5 text-xs font-medium pointer-events-none"
              disabled
            >
              <Loader2 className="w-3 h-3 mr-1.5 animate-spin" />
               {t("common.installing")}
            </Button>
          ) : (
            <Button
              size="sm"
              variant="default"
              className="h-7 px-2.5 text-xs font-medium"
              onClick={(e) => {
                e.stopPropagation();
                onInstall?.(skill.git_url, skill.name);
              }}
            >
              <Download className="w-3 h-3 mr-1.5" />
               {t("common.install")}
            </Button>
          )}
        </div>

        <CardHeader className={cn(skill.rank ? "pl-12" : undefined, "pr-24")}>
          <div className="flex items-center gap-2.5">
            {selectable ? (
              <button
                onClick={handleCheckboxClick}
                className={cn(
                  "w-9 h-9 rounded-xl flex items-center justify-center shrink-0 transition-colors cursor-pointer",
                  selected
                    ? "bg-primary text-primary-foreground shadow-md shadow-primary/20"
                    : "bg-primary/10 text-primary hover:bg-primary/20"
                )}
              >
                {selected ? (
                  <Check className="w-4 h-4" />
                ) : (
                  <GitBranch className="w-4 h-4" />
                )}
              </button>
            ) : (
              <div className="w-9 h-9 rounded-xl bg-primary/10 flex items-center justify-center shrink-0">
                <GitBranch className="w-4 h-4 text-primary" />
              </div>
            )}
            <div className="min-w-0">
              <CardTitle className="truncate">{skill.name}</CardTitle>
              {isLocalSkill && (
                <span className="text-caption text-xs">local</span>
              )}
              {!isLocalSkill && skill.source && (
                <span className="text-caption text-xs">{skill.source}</span>
              )}
              {!isLocalSkill && !skill.source && skill.author && (
                <span className="text-caption text-xs">{skill.author}</span>
              )}
            </div>
          </div>
        </CardHeader>

        <CardContent className="flex-1">
          <CardDescription className="line-clamp-2">{skill.description || t("skillCard.noDescription")}</CardDescription>
        </CardContent>

        <CardFooter className="flex items-center justify-between px-4 py-2.5 mt-auto rounded-b-xl min-h-[44px]">
          {/* Left side: Installs & Category */}
          <div className="flex items-center gap-2">
            {skill.stars > 0 && (
              <div className="flex items-center gap-1">
                <Download className="w-3.5 h-3.5 text-primary/60" />
                <span className="text-xs font-medium text-muted-foreground tabular-nums">
                  {formatInstalls(skill.stars)}
                </span>
              </div>
            )}

            {cat && (
              <Badge variant={cat.variant} className="text-micro px-1.5 py-0 h-4 font-medium opacity-90">
                {cat.label}
              </Badge>
            )}
          </div>

          {/* Right side: Agent badges or install command hint */}
          <div className="flex items-center gap-1.5 relative z-10 flex-1 min-w-0 justify-end">
            {profiles && onToggleAgent ? (
              <HScrollRow
                count={profiles.length}
                maxVisible={10}
                itemWidth={28}
                gap={6}
                className="gap-1.5"
              >
                {profiles.map((profile) => {
                  const isUsed = skill.agent_links?.includes(profile.display_name) ?? false;
                  const toggleKey = `${skill.name}::${profile.id}`;
                  const isToggling = pendingAgentToggleKeys?.has(toggleKey) ?? false;
                  return (
                    <button
                      key={profile.id}
                      onClick={(e) => {
                        e.stopPropagation();
                        if (isToggling) return;
                        onToggleAgent(skill.name, profile.id, !isUsed, profile.display_name);
                      }}
                      disabled={isToggling}
                      className={cn(
                        "w-7 h-7 shrink-0 rounded-lg flex items-center justify-center border transition-colors cursor-pointer focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-primary/45 focus-visible:ring-offset-1 focus-visible:ring-offset-background disabled:cursor-wait",
                        isUsed
                          ? "border-primary/40 bg-primary/10 shadow-[0_0_0_1px_rgba(var(--color-primary-rgb),0.15)] hover:shadow-[0_0_0_1px_rgba(var(--color-primary-rgb),0.3)] hover:bg-primary/20"
                          : "border-transparent bg-transparent hover:bg-muted",
                        isToggling && "opacity-65"
                      )}
                      title={`${profile.display_name} ${isUsed ? "(Remove)" : "(Add)"}`}
                    >
                      <AgentIcon
                        profile={profile}
                        className={cn(
                          agentIconCls(profile.icon, "w-4 h-4"),
                          "transition-[filter,opacity] drop-shadow-sm",
                          !isUsed && "grayscale opacity-40 hover:opacity-70"
                        )}
                      />
                    </button>
                  );
                })}
              </HScrollRow>
            ) : skill.source ? (
              <span className="text-micro text-muted-foreground/60 font-mono flex items-center gap-1">
                <ExternalLink className="w-3 h-3" />
                skills.sh
              </span>
            ) : (
              skill.agent_links && skill.agent_links.length > 0 && (
                skill.agent_links.map((agent) => (
                  <Badge key={agent} variant="outline" className="text-micro px-1.5 py-0 h-4 leading-none font-normal text-muted-foreground shadow-sm">
                    {agent}
                  </Badge>
                ))
              )
            )}
          </div>
        </CardFooter>
      </Card>
    </Wrapper>
  );
}

export const SkillCard = memo(SkillCardInner);
