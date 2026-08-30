import { AnimatePresence, motion } from "framer-motion";
import { ChevronDown, Plus, Unlink, X } from "lucide-react";
import { memo, useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { AgentIcon } from "../../../components/ui/AgentIcon";
import { Switch } from "../../../components/ui/switch";
import { supportsGlobalDeploy } from "../../../lib/agentProfiles";
import {
  agentIconCls,
  cn,
  detectPlatform,
  formatGlobalPathForDisplay,
  inferUserHomeRoot,
  type Platform,
} from "../../../lib/utils";
import type { AgentManagedSkillsState } from "../../../lib/ipc/commands/agents";
import type { AgentProfile, CustomProfileDef } from "../../../types";
import { getAgentSkillPauseSnapshot, globalSkillsTargetKey } from "../lib/agentSkillSync";
import { AddCustomAgentDialog } from "../components/AddCustomAgentDialog";
import { AgentListFilterBar } from "../components/AgentListFilterBar";
import {
  type AgentStatusFilter,
  countAgentStatuses,
  filterAgentProfilesByStatus,
  isAgentFilterActive,
  searchAgentProfiles,
} from "../lib/agentFilters";

interface AgentConnectionsSectionProps {
  profiles: AgentProfile[];
  profilesLoading: boolean;
  expandedAgentId: string | null;
  linkedSkills: Record<string, string[]>;
  managedSkillStates?: Record<string, AgentManagedSkillsState>;
  pendingSkillTargetKeys?: Record<string, true>;
  onToggleProfile: (profile: AgentProfile) => void;
  onToggleExpand: (agentId: string) => void;
  onUnlinkSkill: (skillName: string, agentId: string) => void;
  onToggleAllSkills?: (profile: AgentProfile) => void;
  onAddCustomProfile?: (def: CustomProfileDef) => void;
  onRemoveCustomProfile?: (id: string) => void;
}

const DEFAULT_VISIBLE_AGENT_COUNT = 10;

interface AgentRowProps {
  profile: AgentProfile;
  platform: Platform;
  expanded: boolean;
  linkedSkillNames?: string[];
  managedSkillState?: AgentManagedSkillsState;
  pendingSkillTargetKeys: Record<string, true>;
  sharedAgentNames: string[];
  onToggleProfile: (profile: AgentProfile) => void;
  onToggleExpand: (agentId: string) => void;
  onUnlinkSkill: (skillName: string, agentId: string) => void;
  onToggleAllSkills: (profile: AgentProfile) => void;
  onEdit?: () => void;
}

function normalizePathForCompare(path: string): string {
  return path.replace(/\\/g, "/").toLowerCase();
}

function displayPaths(profile: AgentProfile, platform: Platform): string[] {
  const primary = formatGlobalPathForDisplay(profile.global_skills_dir, platform);
  if (!primary) return [];
  if (profile.id !== "codex") return [primary];

  const inferredHome = inferUserHomeRoot(profile.global_skills_dir);
  const codexLegacyRaw = inferredHome ? `${inferredHome}/.agents/skills` : "~/.agents/skills";
  const codexLegacyPath = formatGlobalPathForDisplay(codexLegacyRaw, platform);
  if (normalizePathForCompare(primary) === normalizePathForCompare(codexLegacyPath)) return [primary];

  return [primary, codexLegacyPath];
}

const AgentRow = memo(function AgentRow({
  profile,
  platform,
  expanded,
  linkedSkillNames,
  managedSkillState,
  pendingSkillTargetKeys,
  sharedAgentNames,
  onToggleProfile,
  onToggleExpand,
  onUnlinkSkill,
  onToggleAllSkills,
  onEdit,
}: AgentRowProps) {
  const { t } = useTranslation();
  const resolvedLinkedSkillNames = linkedSkillNames ?? [];
  const linkedCount = linkedSkillNames?.length ?? profile.synced_count;
  const readyForCards = profile.enabled;
  const supportsManagedSkills = supportsGlobalDeploy(profile);
  const managedSkillsSnapshot = getAgentSkillPauseSnapshot(managedSkillState);
  const targetKey = globalSkillsTargetKey(profile.global_skills_dir);
  const managedSkillsPending = Boolean(pendingSkillTargetKeys[targetKey]);
  const managedSkillsDisabled =
    !readyForCards ||
    managedSkillsPending ||
    managedSkillsSnapshot.status === "loading" ||
    managedSkillsSnapshot.status === "empty";
  const managedSkillsStatusText = !readyForCards
    ? t("settings.enableAgentForManagedSkills")
    : managedSkillsSnapshot.status === "loading"
      ? t("settings.managedSkillsChecking")
      : managedSkillsSnapshot.status === "empty"
        ? t("settings.noManagedSkills")
        : managedSkillsSnapshot.status === "paused"
          ? t("settings.managedSkillsPausedCount", { count: managedSkillsSnapshot.suspendedSkillNames.length })
          : managedSkillsSnapshot.status === "partial"
            ? t("settings.managedSkillsPartialCount", {
                active: managedSkillsSnapshot.activeSkillNames.length,
                paused: managedSkillsSnapshot.suspendedSkillNames.length,
              })
            : t("settings.managedSkillsActiveCount", { count: managedSkillsSnapshot.activeSkillNames.length });

  const icon = <AgentIcon profile={profile} className={cn(agentIconCls(profile.icon, "w-4 h-4"), "object-contain")} />;

  return (
    <div role="listitem" className={cn("transition-colors", readyForCards && "bg-primary/[0.025]")}>
      <div className="flex items-center gap-3 px-3 py-2.5 sm:px-4">
        {onEdit ? (
          <button
            type="button"
            onClick={onEdit}
            title={t("common.edit", { defaultValue: "Edit" })}
            aria-label={`${t("common.edit", { defaultValue: "Edit" })} ${profile.display_name}`}
            className={cn(
              "flex h-8 w-8 shrink-0 cursor-pointer items-center justify-center rounded-lg border bg-card shadow-sm transition-[background-color,border-color,transform] duration-200 hover:-translate-y-0.5 hover:border-primary/40 hover:bg-muted/60 focus-ring active:translate-y-0",
              !readyForCards && "border-border/60 grayscale opacity-65",
            )}
          >
            {icon}
          </button>
        ) : (
          <div
            className={cn(
              "flex h-8 w-8 shrink-0 items-center justify-center rounded-lg border bg-card shadow-sm transition-colors",
              readyForCards ? "border-primary/20" : "border-border/60 grayscale opacity-65",
            )}
          >
            {icon}
          </div>
        )}

        <div className="min-w-0 flex-1">
          <div className="flex min-w-0 flex-wrap items-center gap-1.5">
            <span className="truncate text-[13px] leading-4 font-semibold tracking-[-0.01em] text-foreground">
              {profile.display_name}
            </span>
            <span
              className={cn(
                "inline-flex items-center gap-1 rounded px-1.5 py-px text-[10px] font-medium leading-none",
                readyForCards ? "bg-success/10 text-success" : "bg-muted text-muted-foreground",
              )}
            >
              <span
                aria-hidden="true"
                className={cn("h-1 w-1 rounded-full", readyForCards ? "bg-success" : "bg-muted-foreground/45")}
              />
              {t(readyForCards ? "settings.agentEnabled" : "settings.agentDisabled")}
            </span>
          </div>

          <div className="mt-1 flex min-w-0 flex-wrap items-center gap-1">
            {displayPaths(profile, platform).map((path) => (
              <span
                key={`${profile.id}-${path}`}
                title={path}
                className="max-w-full truncate rounded bg-muted/45 px-1 py-px font-mono text-[10px] leading-3 font-medium text-muted-foreground/70"
              >
                {path}
              </span>
            ))}
          </div>

          {readyForCards && supportsManagedSkills && (
            <div className="mt-1.5 flex min-w-0 flex-wrap items-center gap-x-2 gap-y-1">
              <div className="flex min-w-0 items-center gap-2">
                <span className="shrink-0 text-[10px] font-medium leading-3 text-foreground/85">
                  {t("settings.managedSkills")}
                </span>
                <span
                  aria-live="polite"
                  className={cn(
                    "min-w-0 truncate text-[10px] leading-3 tabular-nums",
                    managedSkillsSnapshot.status === "partial" || managedSkillsSnapshot.status === "paused"
                      ? "text-primary"
                      : "text-muted-foreground",
                  )}
                >
                  {managedSkillsPending ? t("settings.managedSkillsUpdating") : managedSkillsStatusText}
                </span>
              </div>
              <Switch
                size="sm"
                checked={managedSkillsSnapshot.checked}
                onCheckedChange={() => onToggleAllSkills(profile)}
                disabled={managedSkillsDisabled}
                aria-busy={managedSkillsPending || undefined}
                aria-label={t("settings.toggleManagedSkills", { name: profile.display_name })}
                title={
                  managedSkillsDisabled
                    ? managedSkillsStatusText
                    : t(
                        managedSkillsSnapshot.action === "pause"
                          ? "settings.pauseManagedSkills"
                          : "settings.restoreManagedSkills",
                        { name: profile.display_name },
                      )
                }
                className={cn(
                  "shrink-0",
                  (managedSkillsSnapshot.status === "partial" || managedSkillsSnapshot.status === "paused") &&
                    "data-[state=unchecked]:border-primary/50 data-[state=unchecked]:bg-primary/15",
                )}
              />
            </div>
          )}

          {supportsManagedSkills && sharedAgentNames.length > 0 && (
            <p className="mt-1 text-[10px] leading-3 text-muted-foreground">
              {t("settings.sharedSkillsTarget", { names: sharedAgentNames.join(", ") })}
            </p>
          )}
        </div>

        {(linkedCount > 0 || expanded) && (
          <button
            type="button"
            onClick={() => onToggleExpand(profile.id)}
            aria-expanded={expanded}
            aria-controls={`agent-links-${profile.id}`}
            className={cn(
              "flex shrink-0 cursor-pointer items-center gap-1 rounded-md px-2 py-1 text-[10px] font-medium leading-none tabular-nums transition-colors focus-ring",
              expanded ? "bg-primary/10 text-primary" : "text-muted-foreground hover:bg-muted hover:text-foreground",
            )}
          >
            {linkedCount} {t("settings.linked")}
            <ChevronDown className={cn("h-3 w-3 transition-transform duration-200", expanded && "rotate-180")} />
          </button>
        )}

        <Switch
          checked={profile.enabled}
          onCheckedChange={() => onToggleProfile(profile)}
          aria-label={t("settings.toggleAgent", { name: profile.display_name })}
          title={t("settings.toggleAgent", { name: profile.display_name })}
        />
      </div>

      <AnimatePresence initial={false}>
        {expanded && (
          <motion.div
            id={`agent-links-${profile.id}`}
            initial={{ height: 0, opacity: 0 }}
            animate={{ height: "auto", opacity: 1 }}
            exit={{ height: 0, opacity: 0 }}
            transition={{ duration: 0.18, ease: "easeOut" }}
            className="overflow-hidden"
          >
            <div className="border-t border-border/70 bg-muted/25 px-4 py-2.5">
              {resolvedLinkedSkillNames.length === 0 ? (
                <span className="text-micro italic text-muted-foreground">{t("settings.noSkillsLinked")}</span>
              ) : (
                <div className="flex flex-wrap gap-1.5">
                  {resolvedLinkedSkillNames.map((skillName) => (
                    <span
                      key={skillName}
                      className="group/chip inline-flex items-center gap-1 rounded-md border border-border bg-card px-2 py-0.5 text-micro text-foreground transition-colors hover:border-destructive/30"
                    >
                      {skillName}
                      <button
                        type="button"
                        onClick={() => onUnlinkSkill(skillName, profile.id)}
                        disabled={managedSkillsPending}
                        className={cn(
                          "cursor-pointer rounded p-1 text-muted-foreground opacity-0 transition group-hover/chip:opacity-100 hover:bg-destructive/10 hover:text-destructive focus-ring focus:opacity-100",
                          managedSkillsPending &&
                            "cursor-not-allowed opacity-50 hover:bg-transparent hover:text-muted-foreground",
                        )}
                        title={t("settings.unlink", { name: skillName })}
                        aria-label={t("settings.unlink", { name: skillName })}
                      >
                        <X className="h-2.5 w-2.5" />
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
});

export const AgentConnectionsSection = memo(function AgentConnectionsSection({
  profiles,
  profilesLoading,
  expandedAgentId,
  linkedSkills,
  managedSkillStates = {},
  pendingSkillTargetKeys = {},
  onToggleProfile,
  onToggleExpand,
  onUnlinkSkill,
  onToggleAllSkills = () => {},
  onAddCustomProfile,
  onRemoveCustomProfile,
}: AgentConnectionsSectionProps) {
  const { t } = useTranslation();
  const platform = detectPlatform();
  const [addModalOpen, setAddModalOpen] = useState(false);
  const [editingProfile, setEditingProfile] = useState<CustomProfileDef | null>(null);
  const [showAllAgents, setShowAllAgents] = useState(false);
  const [query, setQuery] = useState("");
  const [statusFilter, setStatusFilter] = useState<AgentStatusFilter>("all");
  const { enabledCount, orderedProfiles } = useMemo(() => {
    const enabledProfiles: AgentProfile[] = [];
    const disabledProfiles: AgentProfile[] = [];

    for (const profile of profiles) {
      (profile.enabled ? enabledProfiles : disabledProfiles).push(profile);
    }

    return {
      enabledCount: enabledProfiles.length,
      orderedProfiles: [...enabledProfiles, ...disabledProfiles],
    };
  }, [profiles]);

  const filterActive = isAgentFilterActive({ query, status: statusFilter });
  const { matchedProfiles, statusCounts } = useMemo(() => {
    const searched = searchAgentProfiles(orderedProfiles, query);
    return {
      matchedProfiles: filterAgentProfilesByStatus(searched, statusFilter),
      statusCounts: countAgentStatuses(searched),
    };
  }, [orderedProfiles, query, statusFilter]);

  // Narrowing already shortens the list, so folding it a second time would just
  // hide the rows the user was searching for.
  const hiddenAgentCount = filterActive ? 0 : Math.max(matchedProfiles.length - DEFAULT_VISIBLE_AGENT_COUNT, 0);
  const visibleProfiles =
    filterActive || showAllAgents ? matchedProfiles : matchedProfiles.slice(0, DEFAULT_VISIBLE_AGENT_COUNT);

  const clearFilters = () => {
    setQuery("");
    setStatusFilter("all");
  };

  const editProfile = (profile: AgentProfile) => {
    setEditingProfile({
      id: profile.id,
      display_name: profile.display_name,
      global_skills_dir: profile.global_skills_dir,
      project_skills_rel: profile.project_skills_rel,
      icon_data_uri: profile.icon,
    });
  };

  const renderProfile = (profile: AgentProfile) => {
    const targetKey = globalSkillsTargetKey(profile.global_skills_dir);
    const sharedAgentNames = supportsGlobalDeploy(profile)
      ? profiles
          .filter(
            (candidate) =>
              candidate.id !== profile.id &&
              supportsGlobalDeploy(candidate) &&
              globalSkillsTargetKey(candidate.global_skills_dir) === targetKey,
          )
          .map((candidate) => candidate.display_name)
      : [];

    return (
      <AgentRow
        key={profile.id}
        profile={profile}
        platform={platform}
        expanded={expandedAgentId === profile.id}
        linkedSkillNames={linkedSkills[profile.id]}
        managedSkillState={managedSkillStates[targetKey]}
        pendingSkillTargetKeys={pendingSkillTargetKeys}
        sharedAgentNames={sharedAgentNames}
        onToggleProfile={onToggleProfile}
        onToggleExpand={onToggleExpand}
        onUnlinkSkill={onUnlinkSkill}
        onToggleAllSkills={onToggleAllSkills}
        onEdit={profile.id.startsWith("custom_") && onAddCustomProfile ? () => editProfile(profile) : undefined}
      />
    );
  };

  return (
    <section aria-labelledby="settings-agent-connections-title">
      <div className="mb-3 flex items-start justify-between gap-3">
        <div className="flex min-w-0 items-start gap-2.5">
          <div className="mt-0.5 flex h-8 w-8 shrink-0 items-center justify-center rounded-[10px] border border-indigo-500/20 bg-indigo-500/10">
            <Unlink className="h-4 w-4 text-indigo-500" />
          </div>
          <div className="min-w-0">
            <h2
              id="settings-agent-connections-title"
              className="text-[13px] font-semibold tracking-tight text-foreground"
            >
              {t("settings.agentConnections")}
            </h2>
          </div>
        </div>

        {onAddCustomProfile && (
          <button
            type="button"
            onClick={() => setAddModalOpen(true)}
            className="group flex shrink-0 cursor-pointer items-center gap-1.5 rounded-lg border border-primary/25 bg-primary/10 px-2.5 py-1.5 text-[11px] font-medium text-primary transition-[background-color,border-color,transform] duration-200 hover:-translate-y-0.5 hover:border-primary/40 hover:bg-primary/15 focus-ring active:translate-y-0"
          >
            <Plus className="h-3.5 w-3.5 transition-transform group-hover:rotate-90" />
            {t("settings.addCustomAgent", { defaultValue: "Add Custom Agent" })}
          </button>
        )}
      </div>

      {profilesLoading ? (
        <div
          role="status"
          aria-label={t("settings.loadingAgents")}
          className="overflow-hidden rounded-2xl border border-border bg-card"
        >
          {[0, 1, 2].map((index) => (
            <div
              key={index}
              className="flex animate-pulse items-center gap-3 border-b border-border/70 px-4 py-3 last:border-0"
            >
              <div className="h-8 w-8 rounded-lg bg-muted" />
              <div className="flex-1 space-y-1.5">
                <div className="h-3 w-28 rounded bg-muted" />
                <div className="h-2.5 w-48 max-w-[65%] rounded bg-muted/70" />
              </div>
              <div className="h-5 w-9 rounded-full bg-muted" />
            </div>
          ))}
          <span className="sr-only">{t("settings.loadingAgents")}</span>
        </div>
      ) : (
        <div className="overflow-hidden rounded-2xl border border-border bg-card/80 shadow-[0_16px_40px_-32px_rgba(15,23,42,0.45)] backdrop-blur-sm">
          <div className="flex items-center justify-between gap-3 border-b border-border/70 bg-muted/20 px-4 py-2.5">
            <div className="flex min-w-0 items-center gap-2">
              <span
                aria-hidden="true"
                className={cn(
                  "h-2 w-2 shrink-0 rounded-full",
                  enabledCount > 0 ? "bg-success shadow-[0_0_0_3px_rgba(34,197,94,0.1)]" : "bg-muted-foreground/35",
                )}
              />
              <span className="truncate text-[11px] font-medium text-foreground/80">
                {t("settings.manualAgentActivation")}
              </span>
            </div>
            <span className="shrink-0 text-micro font-medium tabular-nums text-muted-foreground">
              {t("settings.activeCount", {
                enabled: enabledCount,
                total: profiles.length,
              })}
            </span>
          </div>

          {profiles.length > 0 ? (
            <>
              <AgentListFilterBar
                query={query}
                status={statusFilter}
                counts={statusCounts}
                onQueryChange={setQuery}
                onStatusChange={setStatusFilter}
              />
              {visibleProfiles.length > 0 ? (
                <div
                  id="settings-agent-profile-list"
                  role="list"
                  aria-label={t("settings.registeredAgents")}
                  className="divide-y divide-border/70"
                >
                  {visibleProfiles.map(renderProfile)}
                </div>
              ) : (
                <div className="flex flex-col items-center gap-2 px-5 py-8 text-center">
                  <span className="text-xs text-muted-foreground">{t("settings.noAgentsMatchFilter")}</span>
                  <button
                    type="button"
                    onClick={clearFilters}
                    className="cursor-pointer rounded-md px-2 py-1 text-xs font-medium text-primary transition-colors hover:bg-primary/10 focus-ring"
                  >
                    {t("settings.clearAgentFilters")}
                  </button>
                </div>
              )}
              {hiddenAgentCount > 0 && (
                <button
                  type="button"
                  aria-expanded={showAllAgents}
                  aria-controls="settings-agent-profile-list"
                  onClick={() => setShowAllAgents((current) => !current)}
                  className="flex w-full cursor-pointer items-center justify-center gap-1.5 border-t border-border/70 bg-muted/15 px-5 py-2.5 text-xs font-medium text-muted-foreground transition-colors hover:bg-muted/35 hover:text-foreground focus-ring"
                >
                  {showAllAgents
                    ? t("settings.collapseAgentList")
                    : t("settings.showRemainingAgents", { count: hiddenAgentCount })}
                  <ChevronDown
                    className={cn("h-3.5 w-3.5 transition-transform duration-200", showAllAgents && "rotate-180")}
                  />
                </button>
              )}
            </>
          ) : (
            <div className="px-5 py-8 text-center text-xs text-muted-foreground">
              {t("settings.noAgentsRegistered")}
            </div>
          )}
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
});
