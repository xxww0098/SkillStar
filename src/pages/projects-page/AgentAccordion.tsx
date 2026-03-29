import { AnimatePresence, motion } from "framer-motion";
import { useTranslation } from "react-i18next";
import { ChevronRight, Plus, Search, X } from "lucide-react";
import { Badge } from "../../components/ui/badge";
import { Input } from "../../components/ui/input";
import { Switch } from "../../components/ui/switch";
import { cn } from "../../lib/utils";
import type { AgentProfile, Skill } from "../../types";

interface AgentAccordionProps {
  enabledProfiles: AgentProfile[];
  enabledAgents: string[];
  expandedAgent: string | null;
  agentSkills: Record<string, string[]>;
  skillFilter: string;
  getAvailableSkills: (agentId: string) => Skill[];
  onToggleExpand: (agentId: string) => void;
  onToggleAgent: (agentId: string) => void;
  onNavigateToSkill?: (skillName: string) => void;
  onRemoveSkill: (agentId: string, skillName: string) => void;
  onSkillFilterChange: (value: string) => void;
  onAddSkill: (agentId: string, skillName: string) => void;
}

export function AgentAccordion({
  enabledProfiles,
  enabledAgents,
  expandedAgent,
  agentSkills,
  skillFilter,
  getAvailableSkills,
  onToggleExpand,
  onToggleAgent,
  onNavigateToSkill,
  onRemoveSkill,
  onSkillFilterChange,
  onAddSkill,
}: AgentAccordionProps) {
  const { t } = useTranslation();

  return (
    <div>
      <div className="text-xs font-medium text-muted-foreground uppercase tracking-wider mb-2">
        {t("projects.agentsSection")}
      </div>
      <div className="rounded-xl border border-border overflow-hidden divide-y divide-border">
        {enabledProfiles.length === 0 && (
          <div className="px-3.5 py-6 text-xs text-muted-foreground text-center">
            {t("projects.noAgents")}
          </div>
        )}
        {enabledProfiles.map((profile) => {
          const isEnabled = enabledAgents.includes(profile.id);
          const isExpanded = expandedAgent === profile.id;
          const skills = agentSkills[profile.id] ?? [];
          const available = isExpanded ? getAvailableSkills(profile.id) : [];

          return (
            <div key={profile.id}>
              <button
                onClick={() => onToggleExpand(profile.id)}
                className={cn(
                  "w-full flex items-center gap-2.5 px-3.5 h-11 text-left transition-colors cursor-pointer",
                  isExpanded ? "bg-primary/[0.03]" : isEnabled ? "hover:bg-muted/50" : "hover:bg-muted/30"
                )}
              >
                <ChevronRight
                  className={cn(
                    "w-3.5 h-3.5 shrink-0 transition-transform duration-200",
                    isExpanded && "rotate-90",
                    !isEnabled && "text-muted-foreground/40"
                  )}
                />
                <img
                  src={`/${profile.icon}`}
                  alt=""
                  className={cn("w-4 h-4 shrink-0 transition-all", !isEnabled && "grayscale opacity-40")}
                />
                <span
                  className={cn("text-sm font-medium flex-1 truncate", !isEnabled && "text-muted-foreground")}
                >
                  {profile.display_name}
                </span>
                {isEnabled && skills.length > 0 && (
                  <Badge variant="outline" className="text-[10px] h-4 px-1.5 shrink-0">
                    {skills.length}
                  </Badge>
                )}
                <div onClick={(event) => event.stopPropagation()} className="shrink-0">
                  <Switch checked={isEnabled} onCheckedChange={() => onToggleAgent(profile.id)} />
                </div>
              </button>

              <AnimatePresence initial={false}>
                {isExpanded && isEnabled && (
                  <motion.div
                    initial={{ height: 0, opacity: 0 }}
                    animate={{ height: "auto", opacity: 1 }}
                    exit={{ height: 0, opacity: 0 }}
                    transition={{ duration: 0.2, ease: "easeInOut" }}
                    className="overflow-hidden"
                  >
                    <div className="px-3.5 pb-3 pt-1 pl-10">
                      {skills.length > 0 ? (
                        <div className="flex flex-wrap gap-1 mb-2">
                          {skills.map((skillName) => (
                            <motion.span
                              key={skillName}
                              layout
                              initial={{ scale: 0.9, opacity: 0 }}
                              animate={{ scale: 1, opacity: 1 }}
                              className="inline-flex items-center gap-1 px-2 py-0.5 rounded-md bg-muted text-xs text-foreground group/chip"
                            >
                              <span
                                onClick={(event) => {
                                  event.stopPropagation();
                                  onNavigateToSkill?.(skillName);
                                }}
                                className="cursor-pointer hover:text-primary hover:underline underline-offset-2 transition-colors"
                              >
                                {skillName}
                              </span>
                              <button
                                onClick={(event) => {
                                  event.stopPropagation();
                                  onRemoveSkill(profile.id, skillName);
                                }}
                                className="p-0.5 rounded hover:bg-destructive/10 text-muted-foreground hover:text-destructive transition-all cursor-pointer opacity-40 hover:opacity-100"
                              >
                                <X className="w-2.5 h-2.5" />
                              </button>
                            </motion.span>
                          ))}
                        </div>
                      ) : (
                        <p className="text-[11px] text-muted-foreground italic mb-2">No skills assigned</p>
                      )}

                      <div className="space-y-1.5">
                        <div className="relative">
                          <Search className="absolute left-2 top-1/2 -translate-y-1/2 w-3 h-3 text-muted-foreground pointer-events-none" />
                          <Input
                            value={skillFilter}
                            onChange={(event) => onSkillFilterChange(event.target.value)}
                            placeholder={t("projects.addSkills")}
                            className="pl-6 h-7 text-xs"
                          />
                        </div>
                        <div className="max-h-24 overflow-y-auto">
                          {available.length > 0 ? (
                            <div className="flex flex-wrap gap-1">
                              {available.map((skill) => (
                                <button
                                  key={skill.name}
                                  onClick={() => onAddSkill(profile.id, skill.name)}
                                  className="inline-flex items-center gap-1 px-2 py-0.5 rounded-md border border-border text-[11px] text-muted-foreground hover:border-primary/40 hover:text-primary hover:bg-primary/5 transition-colors cursor-pointer"
                                >
                                  <Plus className="w-2.5 h-2.5" />
                                  {skill.name}
                                </button>
                              ))}
                            </div>
                          ) : (
                            <div className="text-[11px] text-muted-foreground text-center py-1">
                              {skillFilter ? t("projects.noMatchingSkills") : t("projects.allAssigned")}
                            </div>
                          )}
                        </div>
                      </div>
                    </div>
                  </motion.div>
                )}
              </AnimatePresence>
            </div>
          );
        })}
      </div>
    </div>
  );
}
