import { AnimatePresence, motion } from "framer-motion";
import { useTranslation } from "react-i18next";
import { AlertTriangle, Unlink, X } from "lucide-react";
import { Badge } from "../../components/ui/badge";
import { Switch } from "../../components/ui/switch";
import { cn } from "../../lib/utils";
import type { AgentProfile } from "../../types";

interface AgentConnectionsSectionProps {
  profiles: AgentProfile[];
  profilesLoading: boolean;
  confirmDisableId: string | null;
  unlinkingId: string | null;
  expandedAgentId: string | null;
  linkedSkills: Record<string, string[]>;
  onToggleProfile: (profile: AgentProfile) => void;
  onToggleExpand: (agentId: string) => void;
  onCancelDisable: () => void;
  onConfirmDisable: () => void;
  onUnlinkSkill: (skillName: string, agentId: string) => void;
}

export function AgentConnectionsSection({
  profiles,
  profilesLoading,
  confirmDisableId,
  unlinkingId,
  expandedAgentId,
  linkedSkills,
  onToggleProfile,
  onToggleExpand,
  onCancelDisable,
  onConfirmDisable,
  onUnlinkSkill,
}: AgentConnectionsSectionProps) {
  const { t } = useTranslation();

  return (
    <section>
      <div className="flex items-center gap-2 mb-3 px-1">
        <div className="w-7 h-7 rounded-lg bg-indigo-500/10 flex items-center justify-center shrink-0 border border-indigo-500/20">
          <Unlink className="w-4 h-4 text-indigo-500" />
        </div>
        <h2 className="text-sm font-semibold text-foreground tracking-tight">{t("settings.agentConnections")}</h2>
      </div>
      {profilesLoading ? (
        <div className="text-muted-foreground text-sm py-4 px-1">{t("settings.loadingAgents")}</div>
      ) : (
        <div className="rounded-xl border border-border bg-card overflow-hidden divide-y divide-border">
          {profiles.map((profile) => {
            const isPendingConfirm = confirmDisableId === profile.id;
            return (
              <div key={profile.id}>
                <div className="flex items-center gap-3 px-4 py-3">
                  <div
                    className={cn(
                      "w-8 h-8 flex items-center justify-center rounded-[10px] transition-all bg-card border border-border shrink-0 shadow-sm",
                      profile.enabled ? "" : "grayscale opacity-50 border-transparent bg-muted/50"
                    )}
                  >
                    <img src={`/${profile.icon}`} alt={profile.display_name} className="w-5 h-5 object-contain" />
                  </div>

                  <span className="text-sm font-medium flex-1">{profile.display_name}</span>

                  {profile.enabled && profile.synced_count > 0 && (
                    <button
                      onClick={() => onToggleExpand(profile.id)}
                      className={cn(
                        "flex items-center gap-1 text-[11px] tabular-nums rounded-md px-1.5 py-0.5 transition-colors cursor-pointer",
                        expandedAgentId === profile.id
                          ? "text-primary bg-primary/10"
                          : "text-muted-foreground hover:text-foreground hover:bg-muted"
                      )}
                    >
                      {linkedSkills[profile.id]?.length ?? profile.synced_count} {t("settings.linked")}
                    </button>
                  )}

                  {profile.installed ? (
                    <Switch
                      checked={profile.enabled}
                      onCheckedChange={() => onToggleProfile(profile)}
                      disabled={unlinkingId === profile.id}
                    />
                  ) : (
                    <Badge variant="outline" className="text-muted-foreground">
                      {t("settings.notFound")}
                    </Badge>
                  )}
                </div>

                <AnimatePresence>
                  {isPendingConfirm && (
                    <motion.div
                      initial={{ height: 0, opacity: 0 }}
                      animate={{ height: "auto", opacity: 1 }}
                      exit={{ height: 0, opacity: 0 }}
                      transition={{ duration: 0.15 }}
                      className="overflow-hidden"
                    >
                      <div className="flex items-center gap-3 px-4 py-2.5 bg-warning/5 border-t border-warning/20">
                        <AlertTriangle className="w-3.5 h-3.5 text-warning shrink-0" />
                        <span className="text-xs text-warning-foreground">
                          {t("settings.linkedSkillsWarning", { count: profile.synced_count })}
                        </span>
                        <div className="ml-auto flex items-center gap-1.5">
                          <button
                            onClick={onCancelDisable}
                            className="px-2 py-1 rounded-md text-[11px] text-muted-foreground hover:bg-muted transition-colors cursor-pointer"
                          >
                            {t("common.cancel")}
                          </button>
                          <button
                            onClick={onConfirmDisable}
                            disabled={unlinkingId === profile.id}
                            className={cn(
                              "px-2.5 py-1 rounded-md text-[11px] font-medium transition-colors cursor-pointer",
                              "bg-destructive/10 text-destructive hover:bg-destructive/20",
                              unlinkingId === profile.id && "opacity-50 pointer-events-none"
                            )}
                          >
                            <Unlink className="w-3 h-3 inline mr-1" />
                            {unlinkingId === profile.id
                              ? t("common.uninstalling")
                              : t("settings.disableAndUnlink")}
                          </button>
                        </div>
                      </div>
                    </motion.div>
                  )}
                </AnimatePresence>

                <AnimatePresence>
                  {expandedAgentId === profile.id && !isPendingConfirm && (
                    <motion.div
                      initial={{ height: 0, opacity: 0 }}
                      animate={{ height: "auto", opacity: 1 }}
                      exit={{ height: 0, opacity: 0 }}
                      transition={{ duration: 0.15 }}
                      className="overflow-hidden"
                    >
                      <div className="px-4 py-2.5 bg-muted/30 border-t border-border">
                        {(linkedSkills[profile.id] ?? []).length === 0 ? (
                          <span className="text-[11px] text-muted-foreground italic">
                            {t("settings.noSkillsLinked")}
                          </span>
                        ) : (
                          <div className="flex flex-wrap gap-1">
                            {(linkedSkills[profile.id] ?? []).map((skillName) => (
                              <span
                                key={skillName}
                                className="group/chip inline-flex items-center gap-1 px-2 py-0.5 rounded-md bg-card border border-border text-[11px] text-foreground transition-colors hover:border-destructive/30"
                              >
                                {skillName}
                                <button
                                  onClick={() => onUnlinkSkill(skillName, profile.id)}
                                  className="opacity-0 group-hover/chip:opacity-100 p-0.5 rounded hover:bg-destructive/10 text-muted-foreground hover:text-destructive transition-all cursor-pointer"
                                  title={t("settings.unlink", { name: skillName })}
                                >
                                  <X className="w-2.5 h-2.5" />
                                </button>
                              </span>
                            ))}
                          </div>
                        )}
                      </div>
                    </motion.div>
                  )}
                </AnimatePresence>
              </div>
            );
          })}
        </div>
      )}
    </section>
  );
}
