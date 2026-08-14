import type { ReactNode } from "react";
import { cn } from "@/lib/utils";
import type { ResetUrgencyMode } from "../../../lib/usageLabels";
import { ResetCountdown } from "../../ResetCountdown";
import { ProgressTrack, type ProgressTrackTone } from "./ProgressTrack";

/**
 * Unified usage "meter" — the single quota-bar grammar shared across every
 * provider card (design: usage-card unification).
 *
 * One consistent rhythm regardless of provider or data shape:
 *
 *   ┌ header ─ [dot] label [tag] ················· [已用 N%] ┐
 *   │ figure ─ <big mono headline>  <caption> ················ │
 *   │ track  ─ ▓▓▓▓▓▓▓▓▓░░░░░░░░ (remaining-oriented)          │
 *   └ foot   ─ <footNote> ······················ [⟳ reset]    ┘
 *
 * Renderers compose this instead of hand-rolling boxes, so the monetary
 * monthly panel, the percent-only weekly bar, and the absolute token quota
 * all read as one system. Breakdown / secondary content goes in `children`.
 */
export interface UsageMeterProps {
  /** Localized window label (left of header). */
  label: string;
  /** Optional uppercase mini-tag next to the label (e.g. 周重置 / 限流配额 / 按月). */
  tag?: string | null;
  /** Consumed share 0–100. Drives the used-badge tone and the track fill. */
  usedPercent: number;
  /** Show the `已用 N%` badge in the header (default true). */
  showUsedBadge?: boolean;
  /** Big mono headline — the number that leads (e.g. `$84.34 / $200`, `100% 剩余`). */
  figure?: ReactNode;
  /** Muted note trailing the figure (e.g. 包含额度 / 软限流额度). */
  caption?: ReactNode;
  /** Sub-line under the label (e.g. rate-limit window hint). */
  hint?: string | null;
  /** Track fill tone. Defaults to the brand-urgency ramp. */
  tone?: ProgressTrackTone;
  /** Fill color for the `accent-static` tone. */
  accent?: string;
  /** Reset timestamp — drives track urgency and (when `showReset`) the footer chip. */
  resetAt?: number | null;
  /**
   * Whether to render the reset countdown chip in the footer. Gate this on
   * `windowRendersOwnReset(window)` so the meter and the card MetaStrip stay
   * complementary and never show the same reset twice. `resetAt` still colors
   * the track regardless.
   */
  showReset?: boolean;
  /** Urgency model for the reset chip. */
  resetMode?: ResetUrgencyMode;
  /** Left side of the footer (e.g. 剩余 $115.66). */
  footNote?: ReactNode;
  /** Text tone class for `footNote`. */
  footNoteClass?: string;
  /** Leading square dot. `brand` picks up the card's `--brand-color`. */
  dot?: "brand" | "emerald" | null;
  compact?: boolean;
  /** Breakdown / secondary rows rendered below the meter, inside the same box. */
  children?: ReactNode;
  "data-testid"?: string;
}

function usedBadgeTone(percent: number): string {
  if (percent >= 90) return "bg-rose-500/10 text-rose-600";
  if (percent >= 75) return "bg-amber-500/10 text-amber-600";
  return "bg-zinc-100 text-zinc-600";
}

export function UsageMeter({
  label,
  tag,
  usedPercent,
  showUsedBadge = true,
  figure,
  caption,
  hint,
  tone = "brand-urgency",
  accent,
  resetAt,
  showReset = true,
  resetMode = "rateLimit",
  footNote,
  footNoteClass,
  dot,
  compact,
  children,
  "data-testid": testId,
}: UsageMeterProps) {
  const resetChip = showReset && resetAt ? resetAt : null;
  return (
    <div
      className={cn(
        "relative overflow-hidden rounded-2xl border border-zinc-200/50 bg-zinc-50/40 transition-colors hover:bg-zinc-50/80",
        compact ? "space-y-2 p-2.5" : "space-y-2.5 p-3",
      )}
      data-testid={testId}
    >
      <div className="flex items-center justify-between gap-2">
        <div className="flex min-w-0 items-center gap-2">
          {dot ? (
            <span
              className={cn("h-1.5 w-1.5 shrink-0 rounded-[2px]", dot === "emerald" && "bg-emerald-500")}
              style={dot === "brand" ? { backgroundColor: "var(--brand-color)" } : undefined}
              aria-hidden
            />
          ) : null}
          <p className="truncate text-[11px] leading-none font-bold text-zinc-700" title={label}>
            {label}
          </p>
          {tag ? (
            <span className="shrink-0 rounded-md bg-zinc-100 px-1.5 py-0.5 text-[8px] font-bold tracking-wider text-zinc-500 uppercase">
              {tag}
            </span>
          ) : null}
        </div>
        {showUsedBadge ? (
          <span
            className={cn(
              "shrink-0 rounded-md px-1.5 py-0.5 font-mono text-[9px] font-bold tabular-nums",
              usedBadgeTone(usedPercent),
            )}
          >
            {usedPercent}%
          </span>
        ) : null}
      </div>

      {hint ? <p className="text-[9px] leading-none text-zinc-400">{hint}</p> : null}

      {figure ? (
        <div className="flex items-baseline gap-1.5">
          {figure}
          {caption ? <span className="ml-auto text-[10px] font-medium text-zinc-400">{caption}</span> : null}
        </div>
      ) : null}

      <ProgressTrack
        usedPercent={usedPercent}
        size={compact ? "compact" : "comfortable"}
        tone={tone}
        resetAt={resetAt}
        accent={accent}
      />

      {footNote || resetChip ? (
        <div className="flex items-center justify-between gap-2">
          <span className={cn("font-mono text-[10px] font-semibold tabular-nums text-zinc-400", footNoteClass)}>
            {footNote}
          </span>
          {resetChip ? <ResetCountdown resetAt={resetChip} usedPercent={usedPercent} mode={resetMode} /> : null}
        </div>
      ) : null}

      {children}
    </div>
  );
}

/** Shared big mono headline used inside `figure`. */
export function MeterFigure({
  value,
  unit,
  emphasis = "ink",
}: {
  value: ReactNode;
  unit?: ReactNode;
  emphasis?: "ink" | "muted";
}) {
  return (
    <>
      <span
        className={cn(
          "font-mono text-lg leading-none font-bold tabular-nums",
          emphasis === "muted" ? "text-zinc-700" : "text-zinc-900",
        )}
      >
        {value}
      </span>
      {unit ? <span className="font-mono text-[11px] font-semibold tabular-nums text-zinc-400">{unit}</span> : null}
    </>
  );
}
