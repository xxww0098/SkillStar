import type { CSSProperties } from "react";
import { cn } from "@/lib/utils";
import { pickConsumedTone, pickUsedBarTone, remainingBarWidth } from "../../../lib/usageLabels";

/**
 * Multi-tone progress track for usage quota bars.
 *
 * Unique *file* for track geometry + fill tone modes (design K17 / §3.5).
 * Does **not** collapse all fills to a single 90/75/brand formula.
 *
 * @see docs/design-usage-card.md §3.5
 */
export type ProgressTrackTone =
  | "brand-urgency" // Simple / Stats / Grok weekly / Breakdown outer
  | "billing-used" // Monetary UsageQuotaPanel (delegates pickUsedBarTone)
  | "consumed" // compact UsageCategoryBar (delegates pickConsumedTone)
  | "accent-static"; // Credits / Cursor OnDemand

export type ProgressTrackSize = "comfortable" | "compact" | "category";

export interface ProgressTrackProps {
  /** Consumed share 0–100; fill width is remaining-oriented. */
  usedPercent: number;
  size: ProgressTrackSize;
  tone: ProgressTrackTone;
  /** Required for billing-used reset urgency; ignored otherwise. */
  resetAt?: number | null;
  /** accent-static fill color; falls back to CSS var --brand-color. */
  accent?: string;
  className?: string;
  "data-testid"?: string;
}

const TRACK_CLASS: Record<ProgressTrackSize, string> = {
  comfortable: "h-2 w-full overflow-hidden rounded-full bg-zinc-100 ring-1 ring-zinc-200/20",
  compact: "h-1.5 w-full overflow-hidden rounded-full bg-zinc-100 ring-1 ring-zinc-200/20",
  category: "h-1 w-full overflow-hidden rounded-full bg-zinc-200/60",
};

/** Exported for unit tests + Style DoD grep anchors. */
export function brandUrgencyFillClass(usedPercent: number): string {
  if (usedPercent >= 90) {
    return "bg-gradient-to-r from-rose-500 to-rose-400 shadow-[0_0_10px_rgba(244,63,94,0.5)] animate-pulse";
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
  className,
  "data-testid": testId,
}: ProgressTrackProps) {
  const fillStyle: CSSProperties = {
    width: remainingBarWidth(usedPercent),
  };

  if (tone === "accent-static") {
    const color = accent ?? "var(--brand-color)";
    fillStyle.background = `linear-gradient(90deg, ${color}, ${color}cc)`;
    fillStyle.boxShadow = accent ? `0 0 8px ${accent}40` : undefined;
  }

  return (
    <div className={cn(TRACK_CLASS[size], className)} data-testid={testId} data-tone={tone} data-size={size}>
      <div
        className={cn(
          "h-full rounded-full transition-[width] duration-300 ease-out",
          tone !== "accent-static" && "transition-all duration-500",
          fillClassName({ usedPercent, size, tone, resetAt, accent }),
        )}
        style={fillStyle}
        data-testid={testId ? `${testId}-fill` : undefined}
      />
    </div>
  );
}

// Quota track call sites migrated (PR3). Non-quota share bars (e.g. DeepSeek model mix) stay local.
