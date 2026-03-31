import { useEffect, useState } from "react";
import { AnimatePresence, motion, useReducedMotion } from "framer-motion";
import { Check, FolderSearch, X, ChevronRight } from "lucide-react";
import { useTranslation } from "react-i18next";
import { Button } from "../ui/button";
import { AgentIcon } from "../ui/AgentIcon";
import { cn, agentIconCls } from "../../lib/utils";
import type { AmbiguousGroup, DetectedAgent } from "../../types";

interface AgentDisambiguationDialogProps {
  open: boolean;
  /** The ambiguous group to resolve. Each group = one shared-path prompt.
   *  Queuing multiple groups is handled by the parent. */
  group: AmbiguousGroup | null;
  /** All detected agents so we can render icon/name for each candidate. */
  allDetected: DetectedAgent[];
  onClose: () => void;
  /** Called with the user's chosen agent ID for the ambiguous path. */
  onConfirm: (selectedAgentId: string) => void;
}

export function AgentDisambiguationDialog({
  open,
  group,
  allDetected,
  onClose,
  onConfirm,
}: AgentDisambiguationDialogProps) {
  const { t } = useTranslation();
  const prefersReducedMotion = useReducedMotion();

  const [selectedId, setSelectedId] = useState<string | null>(null);

  // Reset selection when dialog opens with a new group
  useEffect(() => {
    if (open && group) {
      // Default: select the first candidate
      setSelectedId(group.agent_ids[0] ?? null);
    }
  }, [open, group]);

  if (!group) return null;

  // Build candidate list from detected agents, filtered to this group's candidates
  const candidateSet = new Set(group.agent_ids);
  const candidates = allDetected.filter((a) => candidateSet.has(a.agent_id));

  const chooseAgent = (agentId: string) => {
    setSelectedId(agentId);
  };

  const handleConfirm = () => {
    if (!selectedId) return;
    onConfirm(selectedId);
  };

  return (
    <AnimatePresence>
      {open && (
        <>
          {/* Backdrop */}
          <motion.div
            initial={{ opacity: 0 }}
            animate={{ opacity: 1 }}
            exit={{ opacity: 0 }}
            transition={{ duration: prefersReducedMotion ? 0.01 : 0.3 }}
            className="fixed inset-0 z-50 bg-black/40 backdrop-blur-sm"
            onClick={onClose}
          />

          {/* Dialog */}
          <motion.div
            initial={
              prefersReducedMotion
                ? { opacity: 0 }
                : { opacity: 0, scale: 0.94, y: 20 }
            }
            animate={
              prefersReducedMotion
                ? { opacity: 1 }
                : { opacity: 1, scale: 1, y: 0 }
            }
            exit={
              prefersReducedMotion
                ? { opacity: 0 }
                : { opacity: 0, scale: 0.94, y: 15 }
            }
            transition={{
              duration: prefersReducedMotion ? 0.01 : 0.35,
              ease: [0.16, 1, 0.3, 1],
            }}
            className="fixed left-1/2 top-1/2 z-50 w-full max-w-[440px] -translate-x-1/2 -translate-y-1/2 p-4"
          >
            <div
              role="dialog"
              aria-modal="true"
              aria-label={t("projects.disambiguateTitle")}
              className="relative overflow-hidden rounded-[24px] border border-border bg-card/95 shadow-[0_0_80px_-20px_rgba(0,0,0,0.5)] backdrop-blur-3xl ring-1 ring-border-subtle"
            >
              {/* Ambient glows */}
              <div className="pointer-events-none absolute -left-20 -top-20 h-48 w-48 rounded-full bg-primary/20 blur-[60px] opacity-70" />
              <div className="pointer-events-none absolute -right-20 -top-20 h-48 w-48 rounded-full bg-blue-500/10 blur-[60px] opacity-70" />

              <div className="relative z-10">
                {/* Header */}
                <div className="flex items-center justify-between border-b border-border-subtle bg-card/70 px-6 py-4">
                  <div className="flex items-center gap-3">
                    <div className="relative flex h-10 w-10 shrink-0 items-center justify-center overflow-hidden rounded-xl border border-white/10 bg-gradient-to-br from-amber-500/20 to-amber-500/5 text-amber-400 shadow-inner">
                      <FolderSearch className="h-4 w-4 relative z-10" />
                    </div>
                    <div>
                      <h2 className="text-base font-bold tracking-tight text-foreground">
                        {t("projects.disambiguateTitle")}
                      </h2>
                      <p className="mt-0.5 text-xs text-muted-foreground max-w-[300px]">
                        {t("projects.disambiguateDesc", { path: group.path })}
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

                {/* Path indicator */}
                <div className="px-6 pt-4 pb-2">
                  <div className="flex items-center gap-2 rounded-xl bg-muted/40 border border-border px-3 py-2">
                    <ChevronRight className="w-3.5 h-3.5 text-muted-foreground/60 shrink-0" />
                    <code className="text-xs font-mono text-foreground/80 truncate">
                      {group.path}
                    </code>
                  </div>
                </div>

                {/* Candidates */}
                <div
                  role="radiogroup"
                  aria-label={t("projects.disambiguateTitle")}
                  className="px-6 pt-2 pb-4 space-y-2"
                >
                  {candidates.map((candidate, index) => {
                    const isSelected = selectedId === candidate.agent_id;
                    const isOpenCodeIcon = candidate.icon
                      .toLowerCase()
                      .includes("opencode");

                    return (
                      <motion.button
                        key={candidate.agent_id}
                        type="button"
                        initial={
                          prefersReducedMotion
                            ? false
                            : { opacity: 0, y: 8 }
                        }
                        animate={{ opacity: 1, y: 0 }}
                        transition={{
                          duration: 0.25,
                          delay: prefersReducedMotion
                            ? 0
                            : index * 0.04,
                          ease: [0.16, 1, 0.3, 1],
                        }}
                        role="radio"
                        aria-checked={isSelected}
                        onClick={() => chooseAgent(candidate.agent_id)}
                        className={cn(
                          "group w-full relative outline-none flex items-center gap-3 overflow-hidden rounded-[16px] border p-3.5 text-left transition duration-300 cursor-pointer",
                          isSelected
                            ? "border-primary/50 bg-primary/10 shadow-[0_4px_20px_-8px_rgba(var(--color-primary-rgb),0.3)]"
                            : "border-border bg-background/70 hover:border-primary/25 hover:bg-card hover:-translate-y-px"
                        )}
                      >
                        {/* Selection glow */}
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

                        {/* Agent icon */}
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
                            profile={candidate as any}
                            className={cn(
                              agentIconCls(candidate.icon, "w-6 h-6"),
                              "transition duration-500 drop-shadow-sm",
                              isSelected
                                ? "opacity-100 scale-110"
                                : "grayscale opacity-50 group-hover:grayscale-0 group-hover:opacity-80",
                              isOpenCodeIcon &&
                                "grayscale-0 invert brightness-200 contrast-125",
                              isOpenCodeIcon &&
                                !isSelected &&
                                "opacity-85 group-hover:opacity-100"
                            )}
                          />
                        </div>

                        {/* Agent info */}
                        <div className="min-w-0 flex-1 relative z-10">
                          <span
                            className={cn(
                              "text-sm font-bold transition-colors",
                              isSelected
                                ? "text-primary"
                                : "text-foreground"
                            )}
                          >
                            {candidate.display_name}
                          </span>
                          <p className="text-micro text-muted-foreground/70 font-mono tracking-tight flex items-center gap-1 mt-0.5">
                            <ChevronRight className="w-2.5 h-2.5 opacity-50 shrink-0" />
                            <span className="truncate">
                              {candidate.project_skills_rel}
                            </span>
                          </p>
                        </div>

                        {/* Checkmark */}
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

                {/* Footer */}
                <div className="flex items-center justify-end border-t border-border-subtle bg-card/70 px-6 py-4 rounded-b-[24px]">
                  <div className="flex gap-2.5">
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
                      className={cn(
                        "rounded-lg px-5 relative overflow-hidden transition",
                        selectedId &&
                          "shadow-[0_0_15px_rgba(var(--color-primary-rgb),0.3)] hover:shadow-[0_0_20px_rgba(var(--color-primary-rgb),0.4)]"
                      )}
                      onClick={handleConfirm}
                      disabled={!selectedId}
                    >
                      <span className="relative z-10 flex items-center gap-1.5 font-semibold text-xs text-white">
                        {t("projects.disambiguateConfirm")}
                        <Check className="h-3.5 w-3.5" />
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
