import { useTranslation } from "react-i18next";
import { cn } from "@/lib/utils";
import { formatUsdCents, localizeWindowLabel, pickRateLimitUsageTone } from "../../lib/usageLabels";
import type { CreditInfo, SubscriptionUsage, UsageWindow } from "../../types";
import { ProgressTrack } from "../card/primitives";
import { ResetCountdown } from "../ResetCountdown";
import { UsageWindowBar } from "../UsageWindowBar";

/** Backend credit types from `skillstar-usage` xai fetcher. */
export const GROK_ON_DEMAND_CAP = "grok-on-demand-cap";
export const GROK_MONTH_SPEND = "grok-month-spend";

interface GrokUsagePanelProps {
  usage: SubscriptionUsage;
  brandColorHex?: string;
  brandColor?: string;
  density?: "comfortable" | "compact";
  catalogId?: string;
  context?: { hasPlatformToken?: boolean };
}

/**
 * Grok-specific usage body:
 * - Weekly plans: percent-first bar (remaining emphasized) + reset badge +
 *   secondary calendar-month spend line (not a second quota bar).
 * - Legacy monthly plans: monetary monthly bar via shared `UsageWindowBar`.
 * - On-demand cap chip only when present (backend omits $0).
 */
export function GrokUsagePanel({
  usage,
  brandColorHex,
  brandColor = "52525B",
  density = "comfortable",
}: GrokUsagePanelProps) {
  const { t } = useTranslation();
  const hex = brandColorHex ?? brandColor;
  const accent = hex.startsWith("#") ? hex : `#${hex}`;
  const compact = density === "compact";
  const weekly = usage.weekly;
  const monthly = usage.monthly;
  const monthSpend = findCredit(usage.credits, GROK_MONTH_SPEND);
  const onDemandCap = findCredit(usage.credits, GROK_ON_DEMAND_CAP);
  const parsedSpend = monthSpend ? parseMonthSpendCents(monthSpend.credit_amount) : null;

  if (!weekly && !monthly && !parsedSpend && !onDemandCap) {
    return null;
  }

  return (
    <div className={cn("space-y-3", compact && "space-y-2")}>
      {weekly && <GrokWeeklyBar window={weekly} accent={accent} compact={compact} />}
      {!weekly && monthly && <UsageWindowBar window={monthly} compact={compact} />}
      {(parsedSpend || onDemandCap) && (
        <div
          className={cn("space-y-2 rounded-2xl border p-3", compact && "p-2")}
          style={{ backgroundColor: `${accent}06`, borderColor: `${accent}14` }}
        >
          {parsedSpend && (
            <div className="flex items-baseline justify-between gap-2">
              <span className="text-[10px] font-semibold uppercase tracking-wider text-zinc-500">
                {t("usage.grokMonthSpend")}
              </span>
              <span className="font-mono text-[12px] font-bold tabular-nums text-zinc-800">
                {formatUsdCents(parsedSpend.used)}
                {parsedSpend.total != null && (
                  <span className="ml-1 text-[10px] font-medium text-zinc-400">
                    / {t("usage.grokMonthSpendCap", { cap: formatUsdCents(parsedSpend.total) })}
                  </span>
                )}
              </span>
            </div>
          )}
          {onDemandCap?.credit_amount && (
            <div
              className={cn(
                "flex items-center justify-between gap-2 text-[10px]",
                parsedSpend && "pt-1.5 border-t border-zinc-200/50",
              )}
            >
              <span className="font-medium text-zinc-500">{t("usage.grokOnDemandCap")}</span>
              <span className="font-mono font-semibold tabular-nums text-zinc-700">{onDemandCap.credit_amount}</span>
            </div>
          )}
        </div>
      )}
    </div>
  );
}

function GrokWeeklyBar({ window, accent, compact }: { window: UsageWindow; accent: string; compact?: boolean }) {
  const { t } = useTranslation();
  const usedPercent = clamp(window.percent ?? 0);
  const remainingPercent = Math.max(0, 100 - usedPercent);
  const label = localizeWindowLabel(window.label, t);
  const tone = pickRateLimitUsageTone(usedPercent);

  return (
    <div
      className={cn(
        "relative overflow-hidden rounded-2xl border border-zinc-200/50 bg-zinc-50/40 transition-colors hover:bg-zinc-50/80",
        compact ? "space-y-1.5 p-2" : "space-y-2.5 p-3",
      )}
    >
      {/* Title only — keep one calm line so the label never wraps against chips. */}
      <div className="flex items-center gap-2">
        <p className="text-[11px] leading-none font-bold whitespace-nowrap text-zinc-700">{label}</p>
        <span
          className="shrink-0 rounded-md px-1.5 py-0.5 text-[8px] font-bold tracking-wider uppercase"
          style={{
            color: accent,
            backgroundColor: `${accent}12`,
            boxShadow: `inset 0 0 0 1px ${accent}28`,
          }}
        >
          {t("usage.grokWeeklyBadge")}
        </span>
      </div>

      <ProgressTrack usedPercent={usedPercent} size={compact ? "compact" : "comfortable"} tone="brand-urgency" />

      {/* Meta under the bar: remaining left, reset right. */}
      <div className="flex items-center justify-between gap-2">
        <span className={cn("font-mono text-[10px] font-semibold tabular-nums", tone.text)}>
          {remainingPercent}% {t("usage.grokRemaining")}
        </span>
        {window.reset_at ? (
          <ResetCountdown resetAt={window.reset_at} usedPercent={usedPercent} mode="rateLimit" />
        ) : null}
      </div>
    </div>
  );
}

function findCredit(credits: CreditInfo[] | undefined, type: string): CreditInfo | undefined {
  return (credits ?? []).find((c) => c.credit_type === type);
}

/** Parse `used` or `used/total` USD cents from the Grok month-spend credit. */
export function parseMonthSpendCents(amount?: string | null): { used: number; total: number | null } | null {
  if (!amount) return null;
  const trimmed = amount.trim();
  const slash = trimmed.match(/^(\d+)\s*\/\s*(\d+)$/);
  if (slash) {
    const used = Number(slash[1]);
    const total = Number(slash[2]);
    if (!Number.isFinite(used) || !Number.isFinite(total)) return null;
    return { used, total };
  }
  if (/^\d+$/.test(trimmed)) {
    const used = Number(trimmed);
    if (!Number.isFinite(used)) return null;
    return { used, total: null };
  }
  return null;
}

function clamp(n: number): number {
  if (!Number.isFinite(n)) return 0;
  return Math.max(0, Math.min(100, Math.round(n)));
}
