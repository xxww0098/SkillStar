import { useState, useEffect } from "react";
import { motion, AnimatePresence } from "framer-motion";
import { X, Sparkles, Check, Loader2 } from "lucide-react";
import { invoke } from "@tauri-apps/api/core";
import { useTranslation } from "react-i18next";
import { Button } from "../ui/button";
import { cn, navigateToAiSettings } from "../../lib/utils";
import { toast } from "../../lib/toast";
import type { Skill } from "../../types";

interface AiConfigLike {
  enabled: boolean;
  api_key: string;
}

interface AiPickSkillsModalProps {
  open: boolean;
  onClose: () => void;
  skills: Skill[];
  onResult: (selectedNames: string[]) => void;
}

type Phase = "input" | "loading" | "result";

export function AiPickSkillsModal({
  open,
  onClose,
  skills,
  onResult,
}: AiPickSkillsModalProps) {
  const { t } = useTranslation();
  const [phase, setPhase] = useState<Phase>("input");
  const [prompt, setPrompt] = useState("");
  const [recommended, setRecommended] = useState<string[]>([]);
  const [selected, setSelected] = useState<Set<string>>(new Set());
  const [error, setError] = useState<string | null>(null);
  const [aiConfigured, setAiConfigured] = useState<boolean | null>(null);

  useEffect(() => {
    if (open) {
      setPhase("input");
      setPrompt("");
      setRecommended([]);
      setSelected(new Set());
      setError(null);
      setAiConfigured(null);

      const loadAiConfig = async () => {
        try {
          const config = await invoke<AiConfigLike>("get_ai_config");
          setAiConfigured(config.enabled && config.api_key.trim().length > 0);
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

      const result = await invoke<string[]>("ai_pick_skills", {
        prompt: prompt.trim(),
        skills: skillMetas,
      });

      // Filter to only names that actually exist locally
      const validNames = result.filter((name) =>
        skills.some((s) => s.name === name)
      );

      setRecommended(validNames);
      setSelected(new Set(validNames));
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
            transition={{ duration: 0.15 }}
            className="fixed inset-0 z-50 bg-black/40 backdrop-blur-sm"
            onClick={onClose}
          />

          <motion.div
            initial={{ opacity: 0, scale: 0.96, y: 12 }}
            animate={{ opacity: 1, scale: 1, y: 0 }}
            exit={{ opacity: 0, scale: 0.96, y: 12 }}
            transition={{ type: "spring", bounce: 0.1, duration: 0.35 }}
            className="fixed left-1/2 top-1/2 -translate-x-1/2 -translate-y-1/2 w-full max-w-lg z-50"
          >
            <div className="relative overflow-hidden rounded-[24px] border border-white/10 bg-card/95 shadow-[0_0_80px_-20px_rgba(0,0,0,0.5)] backdrop-blur-3xl ring-1 ring-white/5">
              {/* Ambient glow */}
              <div className="pointer-events-none absolute -left-20 -top-20 h-48 w-48 rounded-full bg-violet-500/20 blur-[60px] opacity-70" />
              <div className="pointer-events-none absolute -right-20 -top-20 h-48 w-48 rounded-full bg-blue-500/15 blur-[60px] opacity-70" />

              <div className="relative z-10">
                {/* Header */}
                <div className="flex items-center justify-between px-6 pt-4 pb-0 shrink-0">
                  <h2 className="text-heading-sm flex items-center gap-2">
                    <Sparkles className="w-4 h-4 text-violet-400" />
                    {t("aiPickModal.title")}
                  </h2>
                  <button
                    onClick={onClose}
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
                      <div className="flex-1 text-sm text-warning/90">
                        {t("aiPickModal.aiNotConfigured")}
                      </div>
                      <button
                        onClick={() => {
                          onClose();
                          navigateToAiSettings();
                        }}
                        className="text-xs cursor-pointer shrink-0 font-medium text-warning hover:text-warning/80 underline underline-offset-2"
                      >
                        {t("skillEditor.configureAI")}
                      </button>
                    </motion.div>
                  )}

                  {phase === "input" && (
                    <motion.div
                      initial={{ opacity: 0 }}
                      animate={{ opacity: 1 }}
                      className="space-y-3"
                    >
                      <p className="text-xs text-muted-foreground">
                        {t("aiPickModal.description")}
                      </p>
                      <textarea
                        value={prompt}
                        onChange={(e) => setPrompt(e.target.value)}
                        placeholder={t("aiPickModal.placeholder")}
                        className="w-full h-28 resize-none rounded-xl border border-border bg-sidebar/50 px-3 py-2.5 text-sm text-foreground placeholder:text-muted-foreground focus:outline-none focus:ring-1 focus:ring-primary/40 transition-all"
                        autoFocus
                        onKeyDown={(e) => {
                          if (e.key === "Enter" && (e.metaKey || e.ctrlKey)) {
                            handlePick();
                          }
                        }}
                      />
                      {error && (
                        <p className="text-xs text-destructive">{error}</p>
                      )}
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
                      <p className="text-sm text-muted-foreground">
                        {t("aiPickModal.picking")}
                      </p>
                      <p className="text-[11px] text-muted-foreground/60">
                        {t("aiPickModal.pickingDetail")}
                      </p>
                    </motion.div>
                  )}

                  {phase === "result" && (
                    <motion.div
                      initial={{ opacity: 0 }}
                      animate={{ opacity: 1 }}
                      className="space-y-3"
                    >
                      <div className="flex items-center justify-between">
                        <p className="text-xs text-muted-foreground">
                          {t("aiPickModal.resultTitle")}
                        </p>
                        <span className="text-[11px] text-muted-foreground tabular-nums">
                          {selected.size} / {recommended.length}
                        </span>
                      </div>

                      {recommended.length === 0 ? (
                        <div className="py-8 text-center text-sm text-muted-foreground">
                          {t("aiPickModal.resultEmpty")}
                        </div>
                      ) : (
                        <div className="max-h-52 overflow-y-auto rounded-xl border border-border/50 bg-sidebar/30">
                          <div className="space-y-0.5 p-1">
                            {recommended.map((name) => {
                              const skill = skills.find(
                                (s) => s.name === name
                              );
                              const isSelected = selected.has(name);
                              return (
                                <button
                                  key={name}
                                  onClick={() => toggleSkill(name)}
                                  className={cn(
                                    "w-full flex items-center gap-2.5 px-2.5 py-2 rounded-lg text-left transition-all cursor-pointer",
                                    isSelected
                                      ? "bg-violet-500/8 hover:bg-violet-500/12"
                                      : "hover:bg-muted/50"
                                  )}
                                >
                                  <div
                                    className={cn(
                                      "w-4 h-4 rounded border-[1.5px] flex items-center justify-center shrink-0 transition-all",
                                      isSelected
                                        ? "bg-violet-500 border-violet-500"
                                        : "border-muted-foreground/30"
                                    )}
                                  >
                                    {isSelected && (
                                      <Check
                                        className="w-2.5 h-2.5 text-white"
                                        strokeWidth={3}
                                      />
                                    )}
                                  </div>
                                  <div className="flex-1 min-w-0">
                                    <div
                                      className={cn(
                                        "text-[13px] truncate",
                                        isSelected
                                          ? "text-violet-300 font-medium"
                                          : "text-foreground"
                                      )}
                                    >
                                      {name}
                                    </div>
                                    {skill?.description && (
                                      <div className="text-[11px] text-muted-foreground truncate mt-0.5">
                                        {skill.description}
                                      </div>
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
                      <Button
                        variant="ghost"
                        size="sm"
                        onClick={() => setPhase("input")}
                      >
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
                        disabled={
                          !prompt.trim() ||
                          phase === "loading" ||
                          skills.length === 0 ||
                          aiConfigured !== true
                        }
                        className="bg-violet-600 hover:bg-violet-500 text-white"
                      >
                        <span className="flex items-center gap-1.5">
                          <Sparkles className="w-3.5 h-3.5" />
                          {phase === "loading"
                            ? t("aiPickModal.picking")
                            : t("aiPickModal.pick")}
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
