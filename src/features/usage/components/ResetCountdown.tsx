import { AlertTriangle } from "lucide-react";
import { useTranslation } from "react-i18next";
import { cn } from "@/lib/utils";
import {
  getResetState,
  isPriorityResetUrgency,
  pickResetTone,
  type ResetUrgency,
  type ResetUrgencyMode,
} from "../lib/usageLabels";

interface ResetCountdownProps {
  resetAt: number;
  usedPercent?: number;
  mode?: ResetUrgencyMode;
  className?: string;
}

function spinDuration(urgency: ResetUrgency): string {
  switch (urgency) {
    case "critical":
      return "1.2s";
    case "urgent":
      return "2s";
    case "soon":
      return "3s";
    case "normal":
      return "5s";
    default:
      return "8s";
  }
}

function ResetSpinner({ urgency, className }: { urgency: ResetUrgency; className?: string }) {
  return (
    <svg
      viewBox="0 0 16 16"
      aria-hidden
      className={cn("h-3.5 w-3.5 shrink-0 animate-spin", className)}
      style={{ animationDuration: spinDuration(urgency) }}
    >
      <path d="M8 2a6 6 0 1 1-4.24 10.24" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" />
      <path
        d="M2 5V2h3"
        fill="none"
        stroke="currentColor"
        strokeWidth="1.5"
        strokeLinecap="round"
        strokeLinejoin="round"
      />
    </svg>
  );
}

export function ResetCountdown({ resetAt, usedPercent = 0, mode = "billing", className }: ResetCountdownProps) {
  const { t } = useTranslation();
  const state = getResetState(resetAt, { usedPercent, mode });
  const tone = pickResetTone(state.urgency);
  const timeLabel = state.urgency === "now" ? t("usage.resetsNowShort") : state.relative;

  return (
    <div
      className={cn(
        "inline-flex items-center gap-1 rounded-md px-1.5 py-0.5",
        tone.badge,
        state.urgency === "critical" || state.urgency === "urgent" ? "font-semibold" : "",
        className,
      )}
      title={t("usage.resetLabel")}
    >
      <ResetSpinner urgency={state.urgency} />
      <span className="text-[11px] tabular-nums leading-none">{timeLabel}</span>
    </div>
  );
}

/** Subtle card chrome when quota should be used before reset. */
export function priorityCardClass(resetAt: number, usedPercent: number, mode: ResetUrgencyMode = "billing"): string {
  const { urgency } = getResetState(resetAt, { usedPercent, mode });
  if (!isPriorityResetUrgency(urgency)) return "";
  if (urgency === "critical" || urgency === "urgent") return "border-orange-500/25";
  return "border-amber-500/20";
}

function priorityHintClass(urgency: ResetUrgency): string {
  if (urgency === "critical" || urgency === "urgent") {
    return "text-orange-600 dark:text-orange-400";
  }
  return "text-amber-700 dark:text-amber-400";
}

interface UsagePriorityHintProps {
  resetAt: number;
  usedPercent?: number;
  mode?: ResetUrgencyMode;
}

/** One-line reminder below the card header when reset is approaching. */
export function UsagePriorityHint({ resetAt, usedPercent = 0, mode = "billing" }: UsagePriorityHintProps) {
  const { t } = useTranslation();
  const state = getResetState(resetAt, { usedPercent, mode });
  if (!isPriorityResetUrgency(state.urgency)) return null;

  return (
    <p className={cn("flex items-start gap-1.5 text-[10px] leading-snug", priorityHintClass(state.urgency))}>
      <AlertTriangle className="h-3 w-3 shrink-0" aria-hidden />
      <span>{t("usage.priorityUseHint")}</span>
    </p>
  );
}
