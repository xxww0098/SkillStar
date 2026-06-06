import { useTranslation } from "react-i18next";
import { cn } from "@/lib/utils";
import {
  formatQuotaNumber,
  formatUsdCents,
  isAbsoluteQuotaWindow,
  isMonetaryQuota,
  localizeCategoryLabel,
  localizeWindowLabel,
  pickConsumedTone,
  pickRemainingTone,
  pickUsedBarTone,
  canonicalizeAntigravityModelName,
} from "../lib/usageLabels";
import type { UsageWindow } from "../types";

interface UsageWindowBarProps {
  window: UsageWindow;
  compact?: boolean;
}

/**
 * Renders a usage quota bar. Monetary windows with breakdown (Cursor) use a
 * structured stat layout; everything else uses a labeled simple bar.
 */
export function UsageWindowBar({ window, compact }: UsageWindowBarProps) {
  if (compact) {
    return <UsageCategoryBar window={window} />;
  }

  if (isMonetaryQuota(window)) {
    return <UsageQuotaPanel window={window} />;
  }

  if (isAbsoluteQuotaWindow(window)) {
    return <UsageStatsWindow window={window} />;
  }

  return <UsageSimpleWindow window={window} />;
}

function UsageQuotaPanel({ window }: { window: UsageWindow }) {
  const { t } = useTranslation();
  const percent = clamp(window.percent ?? computePercent(window.used, window.total));
  const remainingPct = Math.max(0, 100 - percent);
  const remainingTone = pickRemainingTone(remainingPct, window.reset_at, percent);
  const total = window.total ?? 0;
  const remainingCents = Math.max(0, total - window.used);

  return (
    <div className="space-y-3">
      <div className="space-y-2.5">
        <div>
          <p className="text-xs font-bold text-zinc-800">{t("usage.currentPeriodUsage")}</p>
          <p className="text-[10px] leading-snug text-zinc-500 mt-0.5">{t("usage.includedUsageHint")}</p>
        </div>

        <dl className="grid grid-cols-[minmax(0,1fr)_auto] gap-x-3 gap-y-1.5 text-[11px]">
          <dt className="text-zinc-500 font-medium">{t("usage.used")}</dt>
          <dd className="font-bold tabular-nums text-zinc-800">{formatUsdCents(window.used)}</dd>
          <dt className="text-zinc-500 font-medium">{t("usage.includedQuota")}</dt>
          <dd className="font-medium tabular-nums text-zinc-700">{formatUsdCents(total)}</dd>
          <dt className="text-zinc-500 font-medium">{t("usage.remaining")}</dt>
          <dd className={cn("font-bold tabular-nums", remainingTone.text)}>
            {formatUsdCents(remainingCents)}
            <span className="ml-1 font-normal text-zinc-500">({remainingPct}%)</span>
          </dd>
        </dl>

        <div className="space-y-1">
          <div className="flex items-center justify-between text-[10px]">
            <span className="text-zinc-700 font-bold">{t("usage.usedPercent", { percent })}</span>
          </div>
          <div className="h-2 w-full overflow-hidden rounded-full bg-zinc-100 ring-1 ring-zinc-200/50">
            <div
              className={cn(
                "h-full rounded-full transition-[width] duration-300 bg-gradient-to-r from-emerald-500 to-emerald-400 shadow-[0_0_10px_rgba(16,185,129,0.4)]",
                pickUsedBarTone(percent, window.reset_at),
              )}
              style={{ width: `${Math.max(2, percent)}%` }}
            />
          </div>
        </div>
      </div>

      {(window.breakdown?.length ?? 0) > 0 && (
        <div className="space-y-2 rounded-2xl border border-zinc-200/80 bg-zinc-50/50 p-2.5">
          <div>
            <p className="text-[10px] font-bold text-zinc-800">{t("usage.usageByCategory")}</p>
            <p className="text-[10px] leading-snug text-zinc-500 mt-0.5">{t("usage.categoryUsageHint")}</p>
          </div>
          <div className="space-y-2.5">
            {window.breakdown!.map((sub, i) => (
              <UsageWindowBar key={`${sub.label}-${i}`} window={sub} compact />
            ))}
          </div>
        </div>
      )}
    </div>
  );
}

function UsageCategoryBar({ window }: { window: UsageWindow }) {
  const { t } = useTranslation();
  const percent = clamp(window.percent ?? computePercent(window.used, window.total));
  const rawLabel = localizeCategoryLabel(window.label, t);
  const label = canonicalizeAntigravityModelName(rawLabel);
  const tone = pickConsumedTone(percent);

  return (
    <div className="space-y-1">
      <div className="flex items-center justify-between gap-2 text-[10px]">
        <span className="truncate text-zinc-700 font-medium">{label}</span>
        <span className={cn("shrink-0 tabular-nums", tone.text)}>{t("usage.usedPercent", { percent })}</span>
      </div>
      <div className="h-1 w-full overflow-hidden rounded-full bg-zinc-200/60">
        <div
          className={cn("h-full rounded-full transition-[width] duration-300", tone.bar)}
          style={{ width: `${Math.max(2, percent)}%` }}
        />
      </div>
    </div>
  );
}

function UsageStatsWindow({ window }: { window: UsageWindow }) {
  const { t } = useTranslation();
  const total = window.total ?? 0;
  const used = window.used;
  const percent = clamp(window.percent ?? computePercent(used, window.total));
  const label = localizeWindowLabel(window.label, t);

  const barBgClass =
    percent >= 90
      ? "bg-gradient-to-r from-rose-500 to-rose-400 shadow-[0_0_10px_rgba(244,63,94,0.5)] animate-pulse"
      : percent >= 75
        ? "bg-gradient-to-r from-amber-500 to-amber-400 shadow-[0_0_10px_rgba(245,158,11,0.5)]"
        : "bg-gradient-to-r from-[var(--brand-color)] to-[var(--brand-color)]/80 shadow-[0_0_10px_rgba(var(--brand-rgb),0.4)]";

  return (
    <div className="space-y-2.5 rounded-2xl bg-zinc-50/40 border border-zinc-200/50 p-3 hover:bg-zinc-50/80 transition-colors relative overflow-hidden">
      <div className="flex items-center justify-between gap-2">
        <span className="text-[11px] font-bold text-zinc-700">{label}</span>
        <span
          className={cn(
            "text-[9px] font-bold font-mono px-1.5 py-0.5 rounded-md",
            percent >= 90
              ? "bg-rose-500/10 text-rose-600"
              : percent >= 75
                ? "bg-amber-500/10 text-amber-600"
                : "bg-zinc-100 text-zinc-600",
          )}
        >
          {percent}%
        </span>
      </div>

      {/* Dashboard Mono Large Quota */}
      <div className="flex items-baseline gap-1.5 py-0.5">
        <span className="text-lg font-bold font-mono text-zinc-900 leading-none">{formatQuotaNumber(used)}</span>
        <span className="text-[10px] text-zinc-300">/</span>
        <span className="text-[11px] font-semibold font-mono text-zinc-500">{formatQuotaNumber(total)}</span>
        <span className="text-[10px] text-zinc-400 ml-auto font-medium">{t("usage.used")}</span>
      </div>

      <div className="h-2 w-full overflow-hidden rounded-full bg-zinc-100 ring-1 ring-zinc-200/20">
        <div
          className={cn("h-full rounded-full transition-all duration-500 ease-out", barBgClass)}
          style={{ width: `${Math.max(2, percent)}%` }}
        />
      </div>
    </div>
  );
}

function UsageSimpleWindow({ window }: { window: UsageWindow }) {
  const { t } = useTranslation();
  const percent = clamp(window.percent ?? computePercent(window.used, window.total));
  const label = localizeWindowLabel(window.label, t);
  const isRateLimit = window.label === "5h" || window.label === "7d";

  const barBgClass =
    percent >= 90
      ? "bg-gradient-to-r from-rose-500 to-rose-400 shadow-[0_0_10px_rgba(244,63,94,0.5)] animate-pulse"
      : percent >= 75
        ? "bg-gradient-to-r from-amber-500 to-amber-400 shadow-[0_0_10px_rgba(245,158,11,0.5)]"
        : "bg-gradient-to-r from-[var(--brand-color)] to-[var(--brand-color)]/80 shadow-[0_0_10px_rgba(var(--brand-rgb),0.4)]";

  return (
    <div className="space-y-2 rounded-2xl bg-zinc-50/40 border border-zinc-200/50 p-3 hover:bg-zinc-50/80 transition-colors relative overflow-hidden">
      <div className="flex items-center justify-between gap-2">
        <div className="min-w-0">
          <p className="text-[11px] font-bold text-zinc-700 leading-none">{label}</p>
          {isRateLimit && <p className="text-[9px] text-zinc-400 mt-1 leading-none">{t("usage.rateLimitWindow")}</p>}
        </div>
        <span
          className={cn(
            "text-[9px] font-bold font-mono px-1.5 py-0.5 rounded-md",
            percent >= 90
              ? "bg-rose-500/10 text-rose-600"
              : percent >= 75
                ? "bg-amber-500/10 text-amber-600"
                : "bg-zinc-100 text-zinc-600",
          )}
        >
          {percent}%
        </span>
      </div>

      <div className="h-1.5 w-full overflow-hidden rounded-full bg-zinc-100 ring-1 ring-zinc-200/20">
        <div
          className={cn("h-full rounded-full transition-all duration-500 ease-out", barBgClass)}
          style={{ width: `${Math.max(2, percent)}%` }}
        />
      </div>
    </div>
  );
}

function clamp(p: number | null): number {
  if (p === null || Number.isNaN(p)) return 0;
  return Math.max(0, Math.min(100, Math.round(p)));
}

function computePercent(used: number, total: number | null): number | null {
  if (!total || total <= 0) return null;
  return Math.round((used / total) * 100);
}
