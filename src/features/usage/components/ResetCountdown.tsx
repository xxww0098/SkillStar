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
    case "now":
    case "critical":
      return "1.2s";
    case "urgent":
      return "2s";
    default:
      return "3s";
  }
}

function shouldSpin(urgency: ResetUrgency): boolean {
  return urgency === "now" || urgency === "critical" || urgency === "urgent";
}

function ResetSpinner({ urgency, className }: { urgency: ResetUrgency; className?: string }) {
  const spinning = shouldSpin(urgency);
  return (
    <svg
      viewBox="0 0 16 16"
      aria-hidden
      className={cn("h-3.5 w-3.5 shrink-0", spinning && "motion-safe:animate-spin", className)}
      style={spinning ? { animationDuration: spinDuration(urgency) } : undefined}
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
  const fullLabel = `${t("usage.resetLabel")} ${timeLabel}`;

  return (
    <div
      className={cn(
        "inline-flex items-center gap-1 rounded-md px-1.5 py-0.5",
        tone.badge,
        state.urgency === "critical" || state.urgency === "urgent" ? "font-semibold" : "",
        className,
      )}
      title={fullLabel}
      data-urgency={state.urgency}
    >
      <ResetSpinner urgency={state.urgency} />
      <span className="sr-only">{t("usage.resetLabel")}</span>
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
