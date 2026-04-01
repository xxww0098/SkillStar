import { useState } from "react";
import { AnimatePresence, motion } from "framer-motion";
import { useTranslation } from "react-i18next";
import { AlertTriangle, Plus, Unlink, X } from "lucide-react";
import { Badge } from "../../../components/ui/badge";
import { Switch } from "../../../components/ui/switch";
import { AgentIcon } from "../../../components/ui/AgentIcon";
import { cn, agentIconCls } from "../../../lib/utils";
import type { AgentProfile, CustomProfileDef } from "../../../types";
import { AddCustomAgentDialog } from "../components/AddCustomAgentDialog";

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
  onAddCustomProfile?: (def: CustomProfileDef) => void;
  onRemoveCustomProfile?: (id: string) => void;
}

function formatGlobalPath(path: string): string {
  const normalized = path.replace(/\\/g, "/");
  const macOrLinux = normalized.match(/^\/(?:Users|home)\/[^/]+\/(.+)$/);
  if (macOrLinux?.[1]) return `~/${macOrLinux[1]}`;

  const windows = normalized.match(/^[A-Za-z]:\/Users\/[^/]+\/(.+)$/);
  if (windows?.[1]) return `~/${windows[1]}`;

  return normalized;
}

function displayPaths(profile: AgentProfile): string[] {
  const primary = formatGlobalPath(profile.global_skills_dir);
  if (profile.id !== "codex") return [primary];

  const codexLegacyPath = "~/.agents/skills";
  if (primary === codexLegacyPath) return [primary];

  return [primary, codexLegacyPath];
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
  onAddCustomProfile,
  onRemoveCustomProfile,
}: AgentConnectionsSectionProps) {
  const { t } = useTranslation();
  const [addModalOpen, setAddModalOpen] = useState(false);
  const [editingProfile, setEditingProfile] = useState<CustomProfileDef | null>(null);

  return (
    <section>
      <div className="flex items-center justify-between mb-3 px-1">
        <div className="flex items-center gap-2">
          <div className="w-7 h-7 rounded-lg bg-indigo-500/10 flex items-center justify-center shrink-0 border border-indigo-500/20">
            <Unlink className="w-4 h-4 text-indigo-500" />
          </div>
          <h2 className="text-sm font-semibold text-foreground tracking-tight">{t("settings.agentConnections")}</h2>
        </div>
        {onAddCustomProfile && (
          <button
            onClick={() => setAddModalOpen(true)}
            className="flex items-center gap-1.5 px-3 py-1.5 text-xs font-medium text-primary bg-primary/10 border border-primary/20 hover:bg-primary/20 transition-colors duration-200 rounded-full group cursor-pointer"
          >
            <Plus className="w-3.5 h-3.5 transition-transform group-hover:scale-110" />
            {t("settings.addCustomAgent", { defaultValue: "Add Custom Agent" })}
          </button>
        )}
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
                  {profile.id.startsWith("custom_") && onAddCustomProfile ? (
                    <button
                      onClick={() => setEditingProfile({
                        id: profile.id,
                        display_name: profile.display_name,
                        global_skills_dir: profile.global_skills_dir,
                        project_skills_rel: profile.project_skills_rel,
                        icon_data_uri: profile.icon,
                      })}
                      title={t("common.edit", { defaultValue: "Edit" })}
                      className={cn(
                        "w-8 h-8 flex items-center justify-center rounded-[10px] transition bg-card border border-border shrink-0 shadow-sm cursor-pointer hover:border-primary/50 hover:bg-muted/50",
                        profile.enabled ? "" : "grayscale opacity-50 border-transparent bg-muted/50 hover:bg-muted/80"
                      )}
                    >
                      <AgentIcon profile={profile} className={cn(agentIconCls(profile.icon, "w-5 h-5"), "object-contain")} />
                    </button>
                  ) : (
                    <div
                      className={cn(
                        "w-8 h-8 flex items-center justify-center rounded-[10px] transition bg-card border border-border shrink-0 shadow-sm",
                        profile.enabled ? "" : "grayscale opacity-50 border-transparent bg-muted/50"
                      )}
                    >
                      <AgentIcon profile={profile} className={cn(agentIconCls(profile.icon, "w-5 h-5"), "object-contain")} />
                    </div>
                  )}

                  <div className="flex-1 min-w-0">
                    <div className="text-sm font-medium truncate">{profile.display_name}</div>
                    <div className="mt-1 flex flex-wrap items-center gap-1.5 min-w-0">
                      {displayPaths(profile).map((path) => (
                        <span
                          key={`${profile.id}-${path}`}
                          className="text-micro text-muted-foreground/60 font-mono bg-muted/40 px-1.5 py-0.5 rounded-md break-all max-w-full"
                        >
                          {path}
                        </span>
                      ))}
                    </div>
                  </div>

                  {profile.enabled && profile.synced_count > 0 && (
                    <button
                      onClick={() => onToggleExpand(profile.id)}
                      className={cn(
                        "flex items-center gap-1 tabular-nums rounded-md px-1.5 py-0.5 transition-colors cursor-pointer text-[10px] font-medium leading-none",
                        expandedAgentId === profile.id
                          ? "text-primary bg-primary/10"
                          : "text-muted-foreground hover:text-foreground hover:bg-muted"
                      )}
                    >
                      {linkedSkills[profile.id]?.length ?? profile.synced_count} {t("settings.linked")}
                    </button>
                  )}

                  {profile.installed ? (
                    <div className="flex items-center gap-2">
                      <Switch
                        checked={profile.enabled}
                        onCheckedChange={() => onToggleProfile(profile)}
                        disabled={unlinkingId === profile.id}
                      />
                    </div>
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
                            className="px-2 py-1 rounded-md text-micro text-muted-foreground hover:bg-muted transition-colors cursor-pointer"
                          >
                            {t("common.cancel")}
                          </button>
                          <button
                            onClick={onConfirmDisable}
                            disabled={unlinkingId === profile.id}
                            className={cn(
                              "px-2.5 py-1 rounded-md text-micro font-medium transition-colors cursor-pointer",
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
                          <span className="text-micro text-muted-foreground italic">
                            {t("settings.noSkillsLinked")}
                          </span>
                        ) : (
                          <div className="flex flex-wrap gap-1">
                            {(linkedSkills[profile.id] ?? []).map((skillName) => (
                              <span
                                key={skillName}
                                className="group/chip inline-flex items-center gap-1 px-2 py-0.5 rounded-md bg-card border border-border text-micro text-foreground transition-colors hover:border-destructive/30"
                              >
                                {skillName}
                                <button
                                  onClick={() => onUnlinkSkill(skillName, profile.id)}
                                  className="opacity-0 group-hover/chip:opacity-100 p-1 rounded hover:bg-destructive/10 text-muted-foreground hover:text-destructive transition cursor-pointer focus-ring"
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

      {onAddCustomProfile && (
        <AddCustomAgentDialog
          open={addModalOpen || !!editingProfile}
          initialData={editingProfile || undefined}
          onClose={() => {
            setAddModalOpen(false);
            setEditingProfile(null);
          }}
          onConfirm={(def) => {
            onAddCustomProfile(def);
            setAddModalOpen(false);
            setEditingProfile(null);
          }}
          onRemove={
            editingProfile && onRemoveCustomProfile
              ? () => onRemoveCustomProfile(editingProfile.id)
              : undefined
          }
        />
      )}
    </section>
  );
}
