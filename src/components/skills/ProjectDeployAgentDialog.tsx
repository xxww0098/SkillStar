import { useCallback, useEffect, useMemo, useState } from "react";
import { AnimatePresence, motion, useReducedMotion } from "framer-motion";
import { Check, FolderKanban, Layers3, Rocket, X, Sparkles, ChevronRight } from "lucide-react";
import { useTranslation } from "react-i18next";
import { Badge } from "../ui/badge";
import { Button } from "../ui/button";
import { SelectAllButton } from "../ui/SelectAllButton";
import { AgentIcon } from "../ui/AgentIcon";
import { cn, agentIconCls } from "../../lib/utils";
import type { AgentProfile, ProjectEntry } from "../../types";

interface ProjectDeployAgentDialogProps {
  open: boolean;
  project: ProjectEntry | null;
  skillNames: string[];
  profiles: AgentProfile[];
  initialSelectedAgentIds?: string[];
  onClose: () => void;
  onConfirm: (agentIds: string[]) => void;
}

export function ProjectDeployAgentDialog({
  open,
  project,
  skillNames,
  profiles,
  initialSelectedAgentIds,
  onClose,
  onConfirm,
}: ProjectDeployAgentDialogProps) {
  const { t } = useTranslation();
  const prefersReducedMotion = useReducedMotion();
  const enabledProfiles = useMemo(
    () => profiles.filter((profile) => profile.enabled && profile.id !== "openclaw"),
    [profiles]
  );

  const pathByAgentId = useMemo(() => {
    const map = new Map<string, string>();
    for (const profile of enabledProfiles) {
      map.set(profile.id, profile.project_skills_rel || profile.id);
    }
    return map;
  }, [enabledProfiles]);

  const agentIdsByPath = useMemo(() => {
    const map = new Map<string, string[]>();
    for (const profile of enabledProfiles) {
      const path = profile.project_skills_rel || profile.id;
      const current = map.get(path) ?? [];
      current.push(profile.id);
      map.set(path, current);
    }
    return map;
  }, [enabledProfiles]);

  const conflictAgentIdsByAgent = useMemo(() => {
    const map = new Map<string, Set<string>>();
    for (const ids of agentIdsByPath.values()) {
      if (ids.length <= 1) continue;
      const idSet = new Set(ids);
      for (const id of ids) {
        map.set(id, idSet);
      }
    }
    return map;
  }, [agentIdsByPath]);

  const normalizeSelection = useCallback(
    (ids: string[]): string[] => {
      const allowedIds = new Set(enabledProfiles.map((profile) => profile.id));
      const seenPaths = new Set<string>();
      const next: string[] = [];

      for (const id of ids) {
        if (!allowedIds.has(id)) continue;
        const path = pathByAgentId.get(id) ?? id;
        if (seenPaths.has(path)) continue;
        seenPaths.add(path);
        next.push(id);
      }

      return next;
    },
    [enabledProfiles, pathByAgentId]
  );

  const normalizedInitialSelection = useMemo(() => {
    const selected = normalizeSelection(initialSelectedAgentIds ?? []);

    if (selected.length > 0) return selected;

    const fallback = enabledProfiles[0];
    return fallback ? [fallback.id] : [];
  }, [initialSelectedAgentIds, enabledProfiles, normalizeSelection]);

  const [selectedAgentIds, setSelectedAgentIds] = useState<string[]>(
    normalizedInitialSelection
  );

  useEffect(() => {
    if (!open) return;
    setSelectedAgentIds(normalizedInitialSelection);
  }, [normalizedInitialSelection, open, project?.name]);

  const visibleSkillNames = skillNames.slice(0, 6);
  const extraSkillCount = Math.max(skillNames.length - visibleSkillNames.length, 0);

  const toggleAgent = (agentId: string) => {
    setSelectedAgentIds((prev) => {
      if (prev.includes(agentId)) {
        return prev.filter((id) => id !== agentId);
      }

      const conflictGroup = conflictAgentIdsByAgent.get(agentId);
      if (!conflictGroup) {
        return [...prev, agentId];
      }

      const next = prev.filter((id) => !conflictGroup.has(id));
      next.push(agentId);
      return next;
    });
  };

  const allSelected = useMemo(() => {
    if (enabledProfiles.length === 0) return false;

    const selected = new Set(selectedAgentIds);
    for (const ids of agentIdsByPath.values()) {
      if (ids.length <= 1) {
        const onlyId = ids[0];
        if (!onlyId || !selected.has(onlyId)) return false;
        continue;
      }

      if (!ids.some((id) => selected.has(id))) return false;
    }

    return true;
  }, [enabledProfiles, selectedAgentIds, agentIdsByPath]);

  const handleToggleSelectAll = () => {
    if (enabledProfiles.length === 0) {
      setSelectedAgentIds([]);
      return;
    }
    if (allSelected) {
      setSelectedAgentIds([]);
      return;
    }

    setSelectedAgentIds((prev) => {
      const next: string[] = [];

      for (const ids of agentIdsByPath.values()) {
        if (ids.length <= 1) {
          if (ids[0]) next.push(ids[0]);
          continue;
        }

        const existing = prev.find((id) => ids.includes(id));
        next.push(existing ?? ids[0]);
      }

      return normalizeSelection(next);
    });
  };

  const handleConfirm = () => {
    if (selectedAgentIds.length === 0) return;
    onConfirm(selectedAgentIds);
  };

  return (
    <AnimatePresence>
      {open && project && skillNames.length > 0 && (
        <>
          {/* Backdrop overlay */}
          <motion.div
            initial={{ opacity: 0 }}
            animate={{ opacity: 1 }}
            exit={{ opacity: 0 }}
            transition={{ duration: prefersReducedMotion ? 0.01 : 0.3 }}
            className="fixed inset-0 z-50 bg-black/40 backdrop-blur-sm"
            onClick={onClose}
          />

          {/* Modal Container */}
          <motion.div
            initial={
              prefersReducedMotion ? { opacity: 0 } : { opacity: 0, scale: 0.94, y: 20 }
            }
            animate={prefersReducedMotion ? { opacity: 1 } : { opacity: 1, scale: 1, y: 0 }}
            exit={prefersReducedMotion ? { opacity: 0 } : { opacity: 0, scale: 0.94, y: 15 }}
            transition={{
              duration: prefersReducedMotion ? 0.01 : 0.35,
              ease: [0.16, 1, 0.3, 1],
            }}
            className="fixed left-1/2 top-1/2 z-50 w-full max-w-[640px] -translate-x-1/2 -translate-y-1/2 p-4"
          >
            <div role="dialog" aria-modal="true" aria-label={t("projectDeployDialog.title")} className="relative overflow-hidden rounded-[24px] border border-border bg-card/95 shadow-[0_0_80px_-20px_rgba(0,0,0,0.5)] backdrop-blur-3xl ring-1 ring-border-subtle">
              
              {/* Top ambient glow */}
              <div className="pointer-events-none absolute -left-20 -top-20 h-48 w-48 rounded-full bg-primary/20 blur-[60px] opacity-70" />
              <div className="pointer-events-none absolute -right-20 -top-20 h-48 w-48 rounded-full bg-accent/10 blur-[60px] opacity-70" />

              <div className="relative z-10">
                {/* ─── Header ────────────────────────────────────────────────────── */}
                <div className="flex items-center justify-between border-b border-border-subtle bg-card/70 px-6 py-4">
                  <div className="flex items-center gap-3">
                    <div className="relative flex h-10 w-10 shrink-0 items-center justify-center overflow-hidden rounded-xl border border-white/10 bg-gradient-to-br from-primary/20 to-primary/5 text-primary shadow-inner">
                      <div className="absolute inset-0 bg-primary/20 opacity-0 transition-opacity duration-500 hover:opacity-100" />
                      <Rocket className="h-4 w-4 relative z-10" />
                    </div>
                    <div>
                      <h2 className="text-base font-bold tracking-tight text-foreground flex items-center gap-2">
                        {t("projectDeployDialog.title")}
                        <Badge variant="outline" className="h-[18px] px-1.5 text-micro uppercase tracking-widest font-bold bg-primary/10 text-primary border-primary/20 rounded-full">
                          {t("projectDeployDialog.multiSelect")}
                        </Badge>
                      </h2>
                      <p className="mt-0.5 text-xs text-muted-foreground max-w-[400px]">
                        {t("projectDeployDialog.subtitle", { project: project.name })}
                      </p>
                    </div>
                  </div>

                  <button
                    onClick={onClose}
                    aria-label={t("common.close")}
                    className="flex h-8 w-8 items-center justify-center rounded-full bg-muted/60 text-muted-foreground transition hover:bg-muted hover:text-foreground hover:rotate-90 duration-300 cursor-pointer border border-border"
                  >
                    <X className="h-3.5 w-3.5" />
                  </button>
                </div>

                {/* ─── Main Content ──────────────────────────────────────────────── */}
                <div className="p-6 space-y-6">
                  {/* Context Cards */}
                  <div className="grid gap-3 md:grid-cols-[1fr_1fr]">
                    <div className="group relative overflow-hidden rounded-2xl border border-border bg-gradient-to-br from-background/70 to-muted/40 p-3.5 transition hover:border-primary/25 hover:shadow-md">
                      <div className="absolute inset-0 bg-gradient-to-br from-primary/5 to-accent/5 opacity-0 transition-opacity duration-300 group-hover:opacity-100" />
                      <div className="relative z-10">
                        <div className="mb-2 flex items-center gap-1.5 text-micro font-bold uppercase tracking-widest text-muted-foreground/70">
                          <FolderKanban className="h-3.5 w-3.5 text-indigo-400/80" />
                          {t("projectDeployDialog.destination")}
                        </div>
                        <div className="text-sm font-semibold text-foreground">{project.name}</div>
                        <div className="mt-1 truncate font-mono text-micro text-muted-foreground/80 bg-muted/70 inline-block px-1.5 py-0.5 rounded border border-border">
                          {project.path}
                        </div>
                      </div>
                    </div>

                    <div className="group relative overflow-hidden rounded-2xl border border-border bg-gradient-to-br from-background/70 to-muted/40 p-3.5 transition hover:border-primary/25 hover:shadow-md flex flex-col justify-between">
                      <div className="absolute inset-0 bg-gradient-to-br from-primary/5 to-accent/5 opacity-0 transition-opacity duration-300 group-hover:opacity-100" />
                      <div className="relative z-10">
                        <div className="mb-2 flex items-center gap-1.5 text-micro font-bold uppercase tracking-widest text-muted-foreground/70">
                          <Layers3 className="h-3.5 w-3.5 text-emerald-400/80" />
                          {t("projectDeployDialog.payload")}
                        </div>
                        <div className="flex flex-wrap gap-1.5 text-micro font-medium">
                          {visibleSkillNames.map((skillName) => (
                            <span
                              key={skillName}
                              className="rounded border border-border bg-background/80 px-2 py-0.5 text-foreground shadow-sm backdrop-blur-md"
                            >
                              {skillName}
                            </span>
                          ))}
                          {extraSkillCount > 0 && (
                            <span className="rounded border border-dashed border-border bg-background/60 px-2 py-0.5 text-muted-foreground backdrop-blur-md">
                              +{extraSkillCount}
                            </span>
                          )}
                        </div>
                      </div>
                    </div>
                  </div>

                  {/* Agents Grid */}
                  <div>
                    <div className="flex items-end justify-between mb-3">
                      <div className="flex items-center gap-2">
                        <Sparkles className="w-4 h-4 text-primary/70" />
                        <h3 className="text-xs font-bold uppercase tracking-widest text-foreground/90">
                          {t("projectDeployDialog.selectAgents")}
                        </h3>
                      </div>
                      <div className="flex items-center gap-2">
                        <Badge variant="outline" className="h-5 px-2 text-micro font-semibold bg-primary/5 text-primary border-primary/20 rounded-full">
                          {t("projectDeployDialog.selectedCount", { count: selectedAgentIds.length })}
                        </Badge>
                        <SelectAllButton
                          allSelected={allSelected}
                          onToggle={handleToggleSelectAll}
                          variant="ghost"
                          size="sm"
                          className="h-5 px-2 text-micro font-semibold text-muted-foreground hover:text-foreground"
                          disabled={enabledProfiles.length === 0}
                        />
                      </div>
                    </div>

                    {enabledProfiles.length === 0 ? (
                      <div className="rounded-2xl border border-dashed border-border bg-background/60 px-4 py-6 text-center text-xs text-muted-foreground">
                        {t("projectDeployDialog.noEnabledAgents")}
                      </div>
                    ) : (
                      <div className="grid gap-3 sm:grid-cols-2 max-h-[220px] overflow-y-auto pr-2 pb-2 -mr-2 scrollbar-thin scrollbar-thumb-white/10 scrollbar-track-transparent">
                      {enabledProfiles.map((profile, index) => {
                        const isSelected = selectedAgentIds.includes(profile.id);
                        const isOpenCodeIcon = profile.icon.toLowerCase().includes("opencode");

                        return (
                          <motion.button
                            key={profile.id}
                            type="button"
                            initial={prefersReducedMotion ? false : { opacity: 0, y: 10 }}
                            animate={{ opacity: 1, y: 0 }}
                            transition={{
                              duration: 0.3,
                              delay: prefersReducedMotion ? 0 : index * 0.04,
                              ease: [0.16, 1, 0.3, 1],
                            }}
                            onClick={() => toggleAgent(profile.id)}
                            className={cn(
                              "group relative outline-none flex items-center gap-3 overflow-hidden rounded-[16px] border p-3 text-left transition duration-300 cursor-pointer",
                              isSelected
                                ? "border-primary/50 bg-primary/10 shadow-[0_4px_20px_-8px_rgba(var(--color-primary-rgb),0.3)]"
                                : "border-border bg-background/70 hover:border-primary/25 hover:bg-card hover:-translate-y-px"
                            )}
                          >
                            {/* Animated Background Gradient for Selection */}
                            <AnimatePresence>
                              {isSelected && (
                                <motion.div
                                  initial={{ opacity: 0 }}
                                  animate={{ opacity: 1 }}
                                  exit={{ opacity: 0 }}
                                  className="absolute inset-0 bg-gradient-to-br from-primary/10 via-transparent to-transparent pointer-events-none"
                                />
                              )}
                            </AnimatePresence>

                            <div
                              className={cn(
                                "flex h-10 w-10 shrink-0 items-center justify-center rounded-[12px] border transition duration-300 relative z-10 shadow-sm overflow-hidden",
                                isSelected
                                  ? "border-primary/30 bg-background shadow-[0_0_10px_rgba(var(--color-primary-rgb),0.15)] scale-[1.03]"
                                  : "border-border bg-background/80 group-hover:bg-card",
                                isOpenCodeIcon &&
                                  (isSelected
                                    ? "border-zinc-500/90 bg-zinc-900"
                                    : "border-zinc-500/70 bg-zinc-900 group-hover:bg-zinc-800 group-hover:border-zinc-300/70")
                              )}
                            >
                              <AgentIcon
                                profile={profile}
                                className={cn(
                                  agentIconCls(profile.icon, "w-6 h-6"),
                                  "transition duration-500 drop-shadow-sm",
                                  isSelected ? "opacity-100 scale-110" : "grayscale opacity-50 group-hover:grayscale-0 group-hover:opacity-80",
                                  isOpenCodeIcon && "grayscale-0 invert brightness-200 contrast-125",
                                  isOpenCodeIcon && !isSelected && "opacity-85 group-hover:opacity-100"
                                )}
                              />
                            </div>

                            <div className="min-w-0 flex-1 relative z-10 flex flex-col justify-center">
                              <div className="flex items-center gap-2 mb-0.5">
                                <span className={cn("truncate text-caption font-bold transition-colors", isSelected ? "text-primary" : "text-foreground")}>
                                  {profile.display_name}
                                </span>
                                {profile.enabled && (
                                  <span className="flex h-1.5 w-1.5 shrink-0 rounded-full bg-emerald-500 shadow-[0_0_6px_rgba(var(--color-success-rgb),0.8)]" />
                                )}
                              </div>
                              <p className="text-micro text-muted-foreground/70 font-mono tracking-tight flex items-center gap-1 truncate">
                                <ChevronRight className="w-2.5 h-2.5 opacity-50 shrink-0" />
                                <span className="truncate">{profile.project_skills_rel}</span>
                              </p>
                            </div>

                            <div
                              className={cn(
                                "relative z-10 flex h-5 w-5 shrink-0 items-center justify-center rounded-full border transition duration-300",
                                isSelected
                                  ? "border-primary bg-primary text-primary-foreground shadow-[0_0_10px_rgba(var(--color-primary-rgb),0.3)] scale-[1.05]"
                                  : "border-border bg-transparent text-transparent group-hover:border-primary/25"
                              )}
                            >
                              <Check className="h-3 w-3" strokeWidth={3} />
                            </div>
                          </motion.button>
                        );
                      })}
                    </div>
                    )}
                  </div>
                </div>

                {/* ─── Footer ────────────────────────────────────────────────────── */}
                <div className="flex items-center justify-end border-t border-border-subtle bg-card/70 px-6 py-4 rounded-b-[24px]">
                  <div className="flex flex-1 sm:flex-none justify-end gap-2.5">
                    <Button
                      variant="ghost"
                      size="sm"
                      className="rounded-lg px-4 border border-border bg-card/80 text-foreground hover:bg-muted/70"
                      onClick={onClose}
                    >
                      {t("common.cancel")}
                    </Button>
                    <Button
                      size="sm"
                      className={cn("rounded-lg px-5 relative overflow-hidden transition", selectedAgentIds.length > 0 && "shadow-[0_0_15px_rgba(var(--color-primary-rgb),0.3)] hover:shadow-[0_0_20px_rgba(var(--color-primary-rgb),0.4)]")}
                      onClick={handleConfirm}
                      disabled={selectedAgentIds.length === 0}
                    >
                      <span className="relative z-10 flex items-center gap-1.5 font-semibold text-xs text-white">
                        {t("projectDeployDialog.stageDeployment")}
                        <Rocket className="h-3.5 w-3.5" />
                      </span>
                    </Button>
                  </div>
                </div>

              </div>
            </div>
          </motion.div>
        </>
      )}
    </AnimatePresence>
  );
}
