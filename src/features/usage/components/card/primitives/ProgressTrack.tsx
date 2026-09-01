import type { CSSProperties } from "react";
import { cn } from "@/lib/utils";
import { pickConsumedTone, pickUsedBarTone, remainingBarPercent, remainingBarWidth } from "../../../lib/usageLabels";

/**
 * Multi-tone progress track for usage quota bars.
 *
 * Unique *file* for track geometry + fill tone modes (design K17 / §3.5).
 * Does **not** collapse all fills to a single 90/75/brand formula.
 *
 * Remaining-oriented fill + threshold ticks (10% / 25% remaining) encode
 * urgency without relying on color alone. Critical pulse is `motion-safe`.
 *
 * @see docs/design-usage-card.md §3.5
 */
export type ProgressTrackTone =
  | "brand-urgency" // Simple / Stats / Grok weekly / Breakdown outer
  | "billing-used" // Monetary UsageQuotaPanel (delegates pickUsedBarTone)
  | "consumed" // compact UsageCategoryBar (delegates pickConsumedTone)
  | "accent-static"; // Credits / Cursor OnDemand

export type ProgressTrackSize = "comfortable" | "compact" | "category";

export type UsageFillUrgency = "ok" | "warn" | "critical";

export interface ProgressTrackProps {
  /** Consumed share 0–100; fill width is remaining-oriented. */
  usedPercent: number;
  size: ProgressTrackSize;
  tone: ProgressTrackTone;
  /** Required for billing-used reset urgency; ignored otherwise. */
  resetAt?: number | null;
  /** accent-static fill color; falls back to CSS var --brand-color. */
  accent?: string;
  /** Accessible name for the remaining-quota bar. */
  ariaLabel?: string;
  /** Spoken remaining summary; defaults to the remaining percent. */
  ariaValueText?: string;
  className?: string;
  "data-testid"?: string;
}

const TRACK_CLASS: Record<ProgressTrackSize, string> = {
  comfortable: "relative h-2 w-full overflow-hidden rounded-full bg-zinc-100 ring-1 ring-zinc-200/20",
  compact: "relative h-1.5 w-full overflow-hidden rounded-full bg-zinc-100 ring-1 ring-zinc-200/20",
  category: "relative h-1 w-full overflow-hidden rounded-full bg-zinc-200/60",
};

export function usageFillUrgency(usedPercent: number): UsageFillUrgency {
  if (usedPercent >= 90) return "critical";
  if (usedPercent >= 75) return "warn";
  return "ok";
}

/** Exported for unit tests + Style DoD grep anchors. */
export function brandUrgencyFillClass(usedPercent: number): string {
  if (usedPercent >= 90) {
    return "bg-gradient-to-r from-rose-500 to-rose-400 shadow-[0_0_10px_rgba(244,63,94,0.5)] motion-safe:animate-pulse";
  }
  if (usedPercent >= 75) {
    return "bg-gradient-to-r from-amber-500 to-amber-400 shadow-[0_0_10px_rgba(245,158,11,0.5)]";
  }
  return "bg-gradient-to-r from-[var(--brand-color)] to-[var(--brand-color-2)] shadow-[0_0_10px_rgba(var(--brand-rgb),0.4)]";
}

function fillClassName(props: ProgressTrackProps): string {
  const { usedPercent, tone, resetAt } = props;
  switch (tone) {
    case "brand-urgency":
      return brandUrgencyFillClass(usedPercent);
    case "billing-used":
      // Preserve monetary panel base gradient + tone override (matches UsageQuotaPanel).
      return cn(
        "bg-gradient-to-r from-emerald-500 to-emerald-400 shadow-[0_0_10px_rgba(16,185,129,0.4)]",
        pickUsedBarTone(usedPercent, resetAt),
      );
    case "consumed":
      return pickConsumedTone(usedPercent).bar;
    case "accent-static":
      return "";
  }
}

export function ProgressTrack({
  usedPercent,
  size,
  tone,
  resetAt = null,
  accent,
  ariaLabel,
  ariaValueText,
  className,
  "data-testid": testId,
}: ProgressTrackProps) {
  const remaining = remainingBarPercent(usedPercent);
  const fillStyle: CSSProperties = {
    width: remainingBarWidth(usedPercent),
  };

  if (tone === "accent-static") {
    const color = accent ?? "var(--brand-color)";
    fillStyle.background = `linear-gradient(90deg, ${color}, ${color}cc)`;
    fillStyle.boxShadow = accent ? `0 0 8px ${accent}40` : undefined;
  }

  const showTicks = size !== "category";

  return (
    <div
      role="progressbar"
      aria-label={ariaLabel}
      aria-valuemin={0}
      aria-valuemax={100}
      aria-valuenow={remaining}
      aria-valuetext={ariaValueText ?? `${remaining}%`}
      className={cn(TRACK_CLASS[size], className)}
      data-testid={testId}
      data-tone={tone}
      data-size={size}
      data-urgency={usageFillUrgency(usedPercent)}
    >
      <div
        className={cn(
          "relative h-full rounded-full motion-safe:transition-[width] motion-safe:duration-300 motion-safe:ease-out motion-reduce:transition-none",
          fillClassName({ usedPercent, size, tone, resetAt, accent, ariaLabel, ariaValueText }),
        )}
        style={fillStyle}
        data-testid={testId ? `${testId}-fill` : undefined}
      />
      {showTicks ? (
        <>
          <span
            aria-hidden
            data-threshold="warn"
            className="pointer-events-none absolute inset-y-0 left-[25%] w-px bg-amber-500/55"
          />
          <span
            aria-hidden
            data-threshold="critical"
            className="pointer-events-none absolute inset-y-0 left-[10%] w-px bg-rose-500/65"
          />
        </>
      ) : null}
    </div>
  );
}

// Quota track call sites migrated (PR3). Non-quota share bars (e.g. DeepSeek model mix) stay local.
