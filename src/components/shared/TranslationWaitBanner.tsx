import { motion, useReducedMotion } from "framer-motion";
import { Loader2 } from "lucide-react";
import { useTranslation } from "react-i18next";
import type { AiTranslatePipelineProgress } from "../../types";

type TranslationWaitBannerProps = {
  elapsedSec: number;
  /** Client-side invoke budget (same heuristic as `useAiStream` safety timer). */
  budgetMs: number;
  /** When set, bar reflects mdtx pipeline phases (bundle-level), not time alone. */
  pipelineProgress?: AiTranslatePipelineProgress | null;
};

/** Map backend phases to a single 0–100 bar (honest: bundle / phase granularity only). */
function pipelineProgressPercent(p: AiTranslatePipelineProgress): number {
  const t = Math.max(1, p.total);
  const c = Math.min(Math.max(0, p.current), p.total);
  switch (p.phase) {
    case "prepare":
      return 6;
    case "translate":
      return 6 + Math.round((c / t) * 72);
    case "finalize":
      return 78 + Math.round((c / t) * 12);
    case "guard":
      return 90 + Math.round((c / t) * 10);
    default:
      return 10;
  }
}

/**
 * Calm loading state for long-running SKILL.md translation: soft motion + honest time ceiling,
 * with optional determinate fill when the backend reports pipeline progress.
 */
export function TranslationWaitBanner({ elapsedSec, budgetMs, pipelineProgress = null }: TranslationWaitBannerProps) {
  const { t } = useTranslation();
  const prefersReducedMotion = useReducedMotion();
  const budgetSec = Math.max(1, budgetMs / 1000);
  const budgetMin = Math.max(1, Math.ceil(budgetMs / 60_000));
  const linearPct = Math.min(100, (elapsedSec / budgetSec) * 100);
  const determinateWidth = `${Math.min(96, linearPct * 0.92 + 4)}%`;

  const backendPct = pipelineProgress != null ? pipelineProgressPercent(pipelineProgress) : null;
  const showDeterminateFill = backendPct != null;
  const barWidthStyle = showDeterminateFill
    ? { width: `${Math.min(96, backendPct * 0.92 + 4)}%` }
    : { width: determinateWidth };

  const ariaNow = showDeterminateFill ? Math.round(Math.min(100, backendPct)) : Math.round(Math.min(100, linearPct));

  return (
    <div
      className="space-y-2.5 px-4 py-3 border-b border-border/70 bg-gradient-to-b from-muted/30 to-muted/10 shrink-0"
      role="status"
      aria-live="polite"
      aria-busy="true"
      aria-label={t("skillEditor.translationProgressAria")}
    >
      <div className="flex items-start gap-3">
        <Loader2
          className="w-4 h-4 mt-0.5 shrink-0 text-primary/85 animate-spin motion-reduce:animate-none"
          aria-hidden
        />
        <div className="min-w-0 flex-1 space-y-1">
          <div className="flex flex-wrap items-baseline gap-x-2 gap-y-0.5">
            <span className="text-xs font-medium text-foreground/95">
              {t("skillEditor.translationInProgress", { seconds: elapsedSec })}
            </span>
            <span className="text-[11px] text-muted-foreground/90 tabular-nums">
              {t("skillEditor.translationBudgetHint", { minutes: budgetMin })}
            </span>
          </div>
          <p className="text-[11px] leading-snug text-muted-foreground/85">{t("skillEditor.translationMayTakeLong")}</p>
        </div>
      </div>

      <div
        className="relative h-2 w-full overflow-hidden rounded-full bg-muted/45 ring-1 ring-border/40"
        role="progressbar"
        aria-valuemin={0}
        aria-valuemax={100}
        aria-valuenow={ariaNow}
        aria-valuetext={
          showDeterminateFill && pipelineProgress
            ? `${ariaNow}% (${pipelineProgress.phase})`
            : `${Math.round(Math.min(100, linearPct))}%`
        }
      >
        {showDeterminateFill && !prefersReducedMotion ? (
          <div
            className="h-full rounded-full bg-primary/55 transition-[width] duration-500 ease-out"
            style={barWidthStyle}
          />
        ) : null}

        {showDeterminateFill && prefersReducedMotion ? (
          <div
            className="h-full rounded-full bg-primary/45 transition-[width] duration-700 ease-out"
            style={barWidthStyle}
          />
        ) : null}

        {!showDeterminateFill && prefersReducedMotion ? (
          <div
            className="h-full rounded-full bg-primary/45 transition-[width] duration-700 ease-out"
            style={{ width: determinateWidth }}
          />
        ) : null}

        {!showDeterminateFill && !prefersReducedMotion ? (
          <>
            <motion.div
              className="absolute inset-y-0 w-[38%] rounded-full bg-gradient-to-r from-primary/15 via-primary/75 to-primary/15 shadow-[0_0_12px_rgba(59,130,246,0.25)]"
              initial={{ left: "-38%" }}
              animate={{ left: ["-38%", "105%"] }}
              transition={{ duration: 2.6, repeat: Infinity, ease: "linear" }}
            />
            <motion.div
              className="pointer-events-none absolute inset-0 rounded-full bg-gradient-to-r from-transparent via-white/5 to-transparent"
              initial={{ opacity: 0.35 }}
              animate={{ opacity: [0.2, 0.45, 0.2] }}
              transition={{ duration: 3.2, repeat: Infinity, ease: "easeInOut" }}
            />
          </>
        ) : null}
      </div>
    </div>
  );
}
