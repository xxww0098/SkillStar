import { useTranslation } from "react-i18next";
import { cn } from "@/lib/utils";
import { windowRendersOwnReset } from "../../lib/resetOwnership";
import { formatUsdCents, localizeWindowLabel } from "../../lib/usageLabels";
import type { CreditInfo, SubscriptionUsage, UsageWindow } from "../../types";
import { MeterFigure, UsageMeter } from "../card/primitives";
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
      {weekly && <GrokWeeklyBar window={weekly} compact={compact} />}
      {/* Prefer weekly as the gating bar; still surface legacy monthly absolute quota when both exist. */}
      {monthly && (!weekly || (monthly.total != null && monthly.total > 0 && monthly.used > 0)) && (
        <UsageWindowBar window={monthly} compact={compact} />
      )}
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

function GrokWeeklyBar({ window, compact }: { window: UsageWindow; compact?: boolean }) {
  const { t } = useTranslation();
  const usedPercent = clamp(window.percent ?? 0);
  const remainingPercent = Math.max(0, 100 - usedPercent);
  const label = localizeWindowLabel(window.label, t);

  // Percent-only soft limit: lead with remaining, never show a used-% twin badge.
  return (
    <UsageMeter
      label={label}
      tag={t("usage.grokWeeklyBadge")}
      dot="brand"
      usedPercent={usedPercent}
      showUsedBadge={false}
      figure={<MeterFigure value={`${remainingPercent}%`} unit={t("usage.grokRemaining")} />}
      caption={t("usage.rateLimitWindow")}
      tone="brand-urgency"
      resetAt={window.reset_at}
      showReset={windowRendersOwnReset(window)}
      resetMode="rateLimit"
      compact={compact}
    />
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
