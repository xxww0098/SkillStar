import { AnimatePresence, motion } from "framer-motion";
import { Plus, Unlink, X } from "lucide-react";
import { useState } from "react";
import { useTranslation } from "react-i18next";
import { AgentIcon } from "../../../components/ui/AgentIcon";
import { Badge } from "../../../components/ui/badge";
import { Switch } from "../../../components/ui/switch";
import {
  agentIconCls,
  cn,
  detectPlatform,
  formatGlobalPathForDisplay,
  inferUserHomeRoot,
  type Platform,
} from "../../../lib/utils";
import type { AgentProfile, CustomProfileDef } from "../../../types";
import { AddCustomAgentDialog } from "../components/AddCustomAgentDialog";

interface AgentConnectionsSectionProps {
  profiles: AgentProfile[];
  profilesLoading: boolean;
  expandedAgentId: string | null;
  linkedSkills: Record<string, string[]>;
  onToggleProfile: (profile: AgentProfile) => void;
  onToggleExpand: (agentId: string) => void;
  onUnlinkSkill: (skillName: string, agentId: string) => void;
  onAddCustomProfile?: (def: CustomProfileDef) => void;
  onRemoveCustomProfile?: (id: string) => void;
}

function normalizePathForCompare(path: string): string {
  return path.replace(/\\/g, "/").toLowerCase();
}

function displayPaths(profile: AgentProfile, platform: Platform): string[] {
  const primary = formatGlobalPathForDisplay(profile.global_skills_dir, platform);
  if (profile.id !== "codex") return [primary];

  const inferredHome = inferUserHomeRoot(profile.global_skills_dir);
  const codexLegacyRaw = inferredHome ? `${inferredHome}/.agents/skills` : "~/.agents/skills";
  const codexLegacyPath = formatGlobalPathForDisplay(codexLegacyRaw, platform);
  if (normalizePathForCompare(primary) === normalizePathForCompare(codexLegacyPath)) return [primary];

  return [primary, codexLegacyPath];
}

export function AgentConnectionsSection({
  profiles,
  profilesLoading,
  expandedAgentId,
  linkedSkills,
  onToggleProfile,
  onToggleExpand,
  onUnlinkSkill,
  onAddCustomProfile,
  onRemoveCustomProfile,
}: AgentConnectionsSectionProps) {
  const { t } = useTranslation();
  const platform = detectPlatform();
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
            type="button"
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
            return (
              <div key={profile.id}>
                <div className="flex items-center gap-3 px-4 py-3">
                  {profile.id.startsWith("custom_") && onAddCustomProfile ? (
                    <button
                      type="button"
                      onClick={() =>
                        setEditingProfile({
                          id: profile.id,
                          display_name: profile.display_name,
                          global_skills_dir: profile.global_skills_dir,
                          project_skills_rel: profile.project_skills_rel,
                          icon_data_uri: profile.icon,
                        })
                      }
                      title={t("common.edit", { defaultValue: "Edit" })}
                      className={cn(
                        "w-8 h-8 flex items-center justify-center rounded-[10px] transition bg-card border border-border shrink-0 shadow-sm cursor-pointer hover:border-primary/50 hover:bg-muted/50",
                        profile.enabled ? "" : "grayscale opacity-50 border-transparent bg-muted/50 hover:bg-muted/80",
                      )}
                    >
                      <AgentIcon
                        profile={profile}
                        className={cn(agentIconCls(profile.icon, "w-5 h-5"), "object-contain")}
                      />
                    </button>
                  ) : (
                    <div
                      className={cn(
                        "w-8 h-8 flex items-center justify-center rounded-[10px] transition bg-card border border-border shrink-0 shadow-sm",
                        profile.enabled ? "" : "grayscale opacity-50 border-transparent bg-muted/50",
                      )}
                    >
                      <AgentIcon
                        profile={profile}
                        className={cn(agentIconCls(profile.icon, "w-5 h-5"), "object-contain")}
                      />
                    </div>
                  )}

                  <div className="flex-1 min-w-0">
                    <div className="text-sm font-medium truncate">{profile.display_name}</div>
                    <div className="mt-1 flex flex-wrap items-center gap-1.5 min-w-0">
                      {displayPaths(profile, platform).map((path) => (
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
                      type="button"
                      onClick={() => onToggleExpand(profile.id)}
                      className={cn(
                        "flex items-center gap-1 tabular-nums rounded-md px-1.5 py-0.5 transition-colors cursor-pointer text-[10px] font-medium leading-none",
                        expandedAgentId === profile.id
                          ? "text-primary bg-primary/10"
                          : "text-muted-foreground hover:text-foreground hover:bg-muted",
                      )}
                    >
                      {linkedSkills[profile.id]?.length ?? profile.synced_count} {t("settings.linked")}
                    </button>
                  )}

                  {profile.installed ? (
                    <div className="flex items-center gap-2">
                      <Switch checked={profile.enabled} onCheckedChange={() => onToggleProfile(profile)} />
                    </div>
                  ) : (
                    <Badge variant="outline" className="text-muted-foreground">
                      {t("settings.notFound")}
                    </Badge>
                  )}
                </div>

                <AnimatePresence>
                  {expandedAgentId === profile.id && (
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
                                  type="button"
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
            editingProfile && onRemoveCustomProfile ? () => onRemoveCustomProfile(editingProfile.id) : undefined
          }
        />
      )}
    </section>
  );
}
