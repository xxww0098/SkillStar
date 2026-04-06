import { invoke } from "@tauri-apps/api/core";
import { AnimatePresence, motion, useReducedMotion } from "framer-motion";
import { Check, Loader2, Sparkles, X } from "lucide-react";
import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { Button } from "../../../components/ui/button";
import { getAiConfigCached } from "../../../hooks/useAiConfig";
import { toast } from "../../../lib/toast";
import { cn, navigateToAiSettings } from "../../../lib/utils";
import type { AiPickRecommendation, AiPickResponse, Skill } from "../../../types";

interface AiPickSkillsModalProps {
  open: boolean;
  onClose: () => void;
  skills: Skill[];
  onResult: (selectedNames: string[]) => void;
}

type Phase = "input" | "loading" | "result";

export function AiPickSkillsModal({ open, onClose, skills, onResult }: AiPickSkillsModalProps) {
  const { t } = useTranslation();
  const prefersReducedMotion = useReducedMotion();
  const [phase, setPhase] = useState<Phase>("input");
  const [prompt, setPrompt] = useState("");
  const [recommended, setRecommended] = useState<AiPickRecommendation[]>([]);
  const [selected, setSelected] = useState<Set<string>>(new Set());
  const [error, setError] = useState<string | null>(null);
  const [aiConfigured, setAiConfigured] = useState<boolean | null>(null);
  const [fallbackUsed, setFallbackUsed] = useState(false);
  const [roundsSucceeded, setRoundsSucceeded] = useState(0);

  useEffect(() => {
    if (open) {
      setPhase("input");
      setPrompt("");
      setRecommended([]);
      setSelected(new Set());
      setError(null);
      setAiConfigured(null);
      setFallbackUsed(false);
      setRoundsSucceeded(0);

      const loadAiConfig = async () => {
        try {
          const config = await getAiConfigCached();
          setAiConfigured(config.enabled && (config.api_format === "local" || config.api_key.trim().length > 0));
        } catch {
          setAiConfigured(false);
        }
      };
      loadAiConfig();
    }
  }, [open]);

  const handlePick = async () => {
    if (!prompt.trim()) return;
    setPhase("loading");
    setError(null);

    try {
      const skillMetas = skills.map((s) => ({
        name: s.name,
        description: s.description,
      }));

      const result = await invoke<AiPickResponse>("ai_pick_skills", {
        prompt: prompt.trim(),
        skills: skillMetas,
      });

      const validRecommendations = result.recommendations.filter((item) => skills.some((s) => s.name === item.name));

      setRecommended(validRecommendations);
      setSelected(new Set(validRecommendations.map((item) => item.name)));
      setFallbackUsed(result.fallbackUsed);
      setRoundsSucceeded(result.roundsSucceeded);
      setPhase("result");
    } catch (e) {
      setError(String(e));
      setPhase("input");
      toast.error(String(e));
    }
  };

  const toggleSkill = (name: string) => {
    setSelected((prev) => {
      const next = new Set(prev);
      if (next.has(name)) next.delete(name);
      else next.add(name);
      return next;
    });
  };

  const handleConfirm = () => {
    onResult(Array.from(selected));
    onClose();
  };

  return (
    <AnimatePresence>
      {open && (
        <>
          <motion.div
            initial={{ opacity: 0 }}
            animate={{ opacity: 1 }}
            exit={{ opacity: 0 }}
            transition={{ duration: prefersReducedMotion ? 0.01 : 0.15 }}
            className="fixed inset-0 z-50 bg-black/40 backdrop-blur-sm"
            onClick={onClose}
          />

          <motion.div
            initial={{ opacity: 0, scale: 0.96, y: 12 }}
            animate={{ opacity: 1, scale: 1, y: 0 }}
            exit={{ opacity: 0, scale: 0.96, y: 12 }}
            transition={{ duration: prefersReducedMotion ? 0.01 : 0.3, ease: [0.16, 1, 0.3, 1] }}
            className="fixed left-1/2 top-1/2 -translate-x-1/2 -translate-y-1/2 w-full max-w-lg z-50"
          >
            <div role="dialog" aria-modal="true" aria-label={t("aiPickModal.title")} className="modal-surface">
              {/* Ambient glow */}
              <div className="pointer-events-none absolute -left-20 -top-20 h-48 w-48 rounded-full bg-primary/20 blur-[60px] opacity-70" />
              <div className="pointer-events-none absolute -right-20 -top-20 h-48 w-48 rounded-full bg-accent/10 blur-[60px] opacity-70" />

              <div className="relative z-10">
                {/* Header */}
                <div className="flex items-center justify-between px-6 pt-4 pb-0 shrink-0">
                  <h2 className="text-heading-sm flex items-center gap-2">
                    <Sparkles className="w-4 h-4 text-primary" />
                    {t("aiPickModal.title")}
                  </h2>
                  <button
                    onClick={onClose}
                    aria-label={t("common.close")}
                    className="p-1.5 rounded-lg hover:bg-muted text-muted-foreground transition-colors cursor-pointer"
                  >
                    <X className="w-4 h-4" />
                  </button>
                </div>

                {/* Body */}
                <div className="px-6 py-4">
                  {aiConfigured === false && (
                    <motion.div
                      initial={{ opacity: 0, scale: 0.95 }}
                      animate={{ opacity: 1, scale: 1 }}
                      className="mb-4 rounded-xl border border-warning/20 bg-warning/10 px-4 py-3 flex items-start gap-3"
                    >
                      <div className="flex-1 text-sm text-warning/90">{t("aiPickModal.aiNotConfigured")}</div>
                      <button
                        onClick={() => {
                          onClose();
                          navigateToAiSettings();
                        }}
                        className="text-xs cursor-pointer shrink-0 font-medium text-warning hover:text-warning/80 underline underline-offset-2 px-2 py-1 rounded focus-ring"
                      >
                        {t("skillEditor.configureAI")}
                      </button>
                    </motion.div>
                  )}

                  {phase === "input" && (
                    <motion.div initial={{ opacity: 0 }} animate={{ opacity: 1 }} className="space-y-3">
                      <p className="text-xs text-muted-foreground">{t("aiPickModal.description")}</p>
                      <textarea
                        value={prompt}
                        onChange={(e) => setPrompt(e.target.value)}
                        placeholder={t("aiPickModal.placeholder")}
                        className="w-full h-28 resize-none rounded-xl border border-border bg-sidebar/50 px-3 py-2.5 text-sm text-foreground placeholder:text-muted-foreground focus:outline-none focus:ring-1 focus:ring-primary/40 transition"
                        autoFocus
                        onKeyDown={(e) => {
                          if (e.key === "Enter" && (e.metaKey || e.ctrlKey)) {
                            handlePick();
                          }
                        }}
                      />
                      {error && <p className="text-xs text-destructive">{error}</p>}
                    </motion.div>
                  )}

                  {phase === "loading" && (
                    <motion.div
                      initial={{ opacity: 0 }}
                      animate={{ opacity: 1 }}
                      className="flex flex-col items-center justify-center py-10 gap-3"
                    >
                      <div className="relative">
                        <div className="w-10 h-10 rounded-full bg-violet-500/10 flex items-center justify-center">
                          <Loader2 className="w-5 h-5 text-violet-400 animate-spin" />
                        </div>
                        <div className="absolute inset-0 rounded-full bg-violet-500/20 animate-ping" />
                      </div>
                      <p className="text-sm text-muted-foreground">{t("aiPickModal.picking")}</p>
                      <p className="text-micro text-muted-foreground/60">{t("aiPickModal.pickingDetail")}</p>
                    </motion.div>
                  )}

                  {phase === "result" && (
                    <motion.div initial={{ opacity: 0 }} animate={{ opacity: 1 }} className="space-y-3">
                      <div className="space-y-1">
                        <div className="flex items-center justify-between">
                          <p className="text-xs text-muted-foreground">{t("aiPickModal.resultTitle")}</p>
                          <span className="text-micro text-muted-foreground tabular-nums">
                            {selected.size} / {recommended.length}
                          </span>
                        </div>
                        <div className="flex flex-wrap items-center gap-2 text-micro text-muted-foreground/80">
                          <span>{t("aiPickModal.consensusMeta", { count: roundsSucceeded })}</span>
                          {fallbackUsed && <span>{t("aiPickModal.fallbackMeta")}</span>}
                        </div>
                      </div>

                      {recommended.length === 0 ? (
                        <div className="py-8 text-center text-sm text-muted-foreground">
                          {t("aiPickModal.resultEmpty")}
                        </div>
                      ) : (
                        <div className="max-h-52 overflow-y-auto rounded-xl border border-border/50 bg-sidebar/30">
                          <div className="space-y-0.5 p-1">
                            {recommended.map((item) => {
                              const skill = skills.find((s) => s.name === item.name);
                              const isSelected = selected.has(item.name);
                              const reason = item.reason.trim();
                              const displayDesc = skill?.localized_description || skill?.description;
                              const showDescription = !!displayDesc && displayDesc !== reason;
                              return (
                                <button
                                  key={item.name}
                                  onClick={() => toggleSkill(item.name)}
                                  className={cn(
                                    "w-full flex items-start gap-2.5 px-2.5 py-2 rounded-lg text-left transition cursor-pointer",
                                    isSelected ? "bg-violet-500/8 hover:bg-violet-500/12" : "hover:bg-muted/50",
                                  )}
                                >
                                  <div
                                    className={cn(
                                      "w-4 h-4 rounded border-[1.5px] flex items-center justify-center shrink-0 transition",
                                      isSelected ? "bg-violet-500 border-violet-500" : "border-muted-foreground/30",
                                    )}
                                  >
                                    {isSelected && <Check className="w-2.5 h-2.5 text-white" strokeWidth={3} />}
                                  </div>
                                  <div className="flex-1 min-w-0">
                                    <div className="flex items-center gap-2">
                                      <div
                                        className={cn(
                                          "text-caption truncate",
                                          isSelected ? "text-violet-300 font-medium" : "text-foreground",
                                        )}
                                      >
                                        {item.name}
                                      </div>
                                      <span className="shrink-0 rounded-full border border-violet-500/40 bg-violet-500/18 px-2 py-0.5 text-[11px] font-semibold leading-none text-violet-800 dark:border-violet-400/45 dark:bg-violet-400/20 dark:text-violet-100">
                                        {item.score}
                                      </span>
                                    </div>
                                    {reason && (
                                      <div className="text-[11px] leading-4 text-violet-100/85 mt-0.5">{reason}</div>
                                    )}
                                    {showDescription && (
                                      <div className="text-micro text-muted-foreground mt-0.5">{displayDesc}</div>
                                    )}
                                  </div>
                                </button>
                              );
                            })}
                          </div>
                        </div>
                      )}
                    </motion.div>
                  )}
                </div>

                {/* Footer */}
                <div className="flex items-center justify-between gap-2 px-6 py-3.5 border-t border-border/60 shrink-0">
                  {phase === "result" ? (
                    <>
                      <Button variant="ghost" size="sm" onClick={() => setPhase("input")}>
                        {t("common.back")}
                      </Button>
                      <Button
                        size="sm"
                        onClick={handleConfirm}
                        disabled={selected.size === 0}
                        className="bg-violet-600 hover:bg-violet-500 text-white"
                      >
                        <span className="flex items-center gap-1.5">
                          <Check className="w-3.5 h-3.5" />
                          {t("aiPickModal.confirm")} ({selected.size})
                        </span>
                      </Button>
                    </>
                  ) : (
                    <>
                      <Button variant="ghost" size="sm" onClick={onClose}>
                        {t("common.cancel")}
                      </Button>
                      <Button
                        size="sm"
                        onClick={handlePick}
                        disabled={!prompt.trim() || phase === "loading" || skills.length === 0 || aiConfigured !== true}
                        className="bg-violet-600 hover:bg-violet-500 text-white"
                      >
                        <span className="flex items-center gap-1.5">
                          <Sparkles className="w-3.5 h-3.5" />
                          {phase === "loading" ? t("aiPickModal.picking") : t("aiPickModal.pick")}
                        </span>
                      </Button>
                    </>
                  )}
                </div>
              </div>
            </div>
          </motion.div>
        </>
      )}
    </AnimatePresence>
  );
}
