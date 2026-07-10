import { useTranslation } from "react-i18next";
import { cn } from "@/lib/utils";
import {
  formatQuotaNumber,
  formatUsdCents,
  isAbsoluteQuotaWindow,
  isBreakdownQuotaWindow,
  isMonetaryQuota,
  localizeCategoryLabel,
  localizeWindowLabel,
  pickConsumedTone,
  pickRateLimitUsageTone,
  pickRemainingTone,
  canonicalizeAntigravityModelName,
} from "../lib/usageLabels";
import type { UsageWindow } from "../types";
import { ProgressTrack } from "./card/primitives";
import { ResetCountdown } from "./ResetCountdown";

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

  if (isBreakdownQuotaWindow(window)) {
    return <UsageBreakdownQuotaPanel window={window} />;
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
          <ProgressTrack usedPercent={percent} size="comfortable" tone="billing-used" resetAt={window.reset_at} />
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

function UsageBreakdownQuotaPanel({ window }: { window: UsageWindow }) {
  const { t } = useTranslation();
  const percent = clamp(window.percent ?? computePercent(window.used, window.total));
  const label = localizeWindowLabel(window.label, t);

  return (
    <div className="space-y-3">
      <div className="relative space-y-2 overflow-hidden rounded-2xl border border-zinc-200/50 bg-zinc-50/40 p-3 transition-colors hover:bg-zinc-50/80">
        <div className="flex items-center justify-between gap-2">
          <p className="text-[11px] leading-none font-bold text-zinc-700">{label}</p>
          <span
            className={cn(
              "rounded-md px-1.5 py-0.5 font-mono text-[9px] font-bold",
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
        <ProgressTrack usedPercent={percent} size="compact" tone="brand-urgency" />
      </div>

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
    </div>
  );
}

function UsageCategoryBar({ window }: { window: UsageWindow }) {
  const { t } = useTranslation();
  const percent = clamp(window.percent ?? computePercent(window.used, window.total));
  const rawLabel = localizeCategoryLabel(window.label, t);
  const label = canonicalizeAntigravityModelName(rawLabel);
  const tone = pickConsumedTone(percent);
  const monetary = isMonetaryQuota(window);
  const hasAbsolute = window.total != null && window.total > 0;
  const rightLabel = monetary
    ? `${formatUsdCents(window.used)}${window.total != null ? ` / ${formatUsdCents(window.total)}` : ""} · ${percent}%`
    : hasAbsolute
      ? `${formatQuotaNumber(window.used)} / ${formatQuotaNumber(window.total ?? 0)} · ${percent}%`
      : t("usage.usedPercent", { percent });

  return (
    <div className="space-y-1">
      <div className="flex items-center justify-between gap-2 text-[10px]">
        <span className="truncate font-medium text-zinc-700" title={label}>
          {label}
        </span>
        <span className={cn("shrink-0 tabular-nums", tone.text)}>{rightLabel}</span>
      </div>
      <ProgressTrack usedPercent={percent} size="category" tone="consumed" />
      {window.reset_at ? (
        <div className="flex justify-end">
          <ResetCountdown resetAt={window.reset_at} usedPercent={percent} mode="rateLimit" />
        </div>
      ) : null}
    </div>
  );
}

function UsageStatsWindow({ window }: { window: UsageWindow }) {
  const { t } = useTranslation();
  const total = window.total ?? 0;
  const used = window.used;
  const percent = clamp(window.percent ?? computePercent(used, window.total));
  const remaining = total > 0 ? Math.max(0, total - used) : null;
  const remainingPct = Math.max(0, 100 - percent);
  const label = localizeWindowLabel(window.label, t);
  const tone = pickConsumedTone(percent);

  return (
    <div className="relative space-y-2.5 overflow-hidden rounded-2xl border border-zinc-200/50 bg-zinc-50/40 p-3 transition-colors hover:bg-zinc-50/80">
      <div className="flex items-center justify-between gap-2">
        <span className="text-[11px] font-bold text-zinc-700">{label}</span>
        <div className="flex shrink-0 items-center gap-1.5">
          {window.reset_at ? <ResetCountdown resetAt={window.reset_at} usedPercent={percent} mode="billing" /> : null}
          <span
            className={cn(
              "rounded-md px-1.5 py-0.5 font-mono text-[9px] font-bold",
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
      </div>

      {/* Dashboard Mono Large Quota */}
      <div className="flex items-baseline gap-1.5 py-0.5">
        <span className="font-mono text-lg leading-none font-bold text-zinc-900">{formatQuotaNumber(used)}</span>
        <span className="text-[10px] text-zinc-300">/</span>
        <span className="font-mono text-[11px] font-semibold text-zinc-500">{formatQuotaNumber(total)}</span>
        <span className="ml-auto text-[10px] font-medium text-zinc-400">{t("usage.used")}</span>
      </div>

      <ProgressTrack usedPercent={percent} size="comfortable" tone="brand-urgency" />

      <div className="flex items-center justify-between gap-2 text-[9px]">
        <span className={cn("font-mono font-semibold tabular-nums", tone.text)}>
          {remaining != null
            ? t("usage.quotaRemaining", { remaining: formatQuotaNumber(remaining) })
            : t("usage.remainingPercent", { percent: remainingPct })}
        </span>
        <span className="tabular-nums text-zinc-400">
          {t("usage.used")} {percent}% · {t("usage.remaining")} {remainingPct}%
        </span>
      </div>
    </div>
  );
}

function UsageSimpleWindow({ window }: { window: UsageWindow }) {
  const { t } = useTranslation();
  const percent = clamp(window.percent ?? computePercent(window.used, window.total));
  const remainingPct = Math.max(0, 100 - percent);
  const label = localizeWindowLabel(window.label, t);
  const isRateLimit = window.label === "5h" || window.label === "7d";
  const hasAbsolute = window.total != null && window.total > 0 && !(window.total === 100 && window.used <= 100);
  const remainingAbs = hasAbsolute && window.total != null ? Math.max(0, window.total - window.used) : null;
  const tone = pickRateLimitUsageTone(percent);

  return (
    <div className="space-y-2 rounded-2xl bg-zinc-50/40 border border-zinc-200/50 p-3 hover:bg-zinc-50/80 transition-colors relative overflow-hidden">
      <div className="flex items-center justify-between gap-2">
        <div className="min-w-0">
          <p className="text-[11px] font-bold text-zinc-700 leading-none">{label}</p>
          {isRateLimit && <p className="text-[9px] text-zinc-400 mt-1 leading-none">{t("usage.rateLimitWindow")}</p>}
        </div>
        <div className="flex items-center gap-1.5 shrink-0">
          {window.reset_at ? (
            <ResetCountdown
              resetAt={window.reset_at}
              usedPercent={percent}
              mode={isRateLimit ? "rateLimit" : "billing"}
            />
          ) : null}
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
      </div>

      {hasAbsolute && (
        <div className="flex items-baseline gap-1.5">
          <span className="font-mono text-sm font-bold tabular-nums text-zinc-900">
            {formatQuotaNumber(window.used)}
          </span>
          <span className="text-[10px] text-zinc-300">/</span>
          <span className="font-mono text-[11px] font-semibold tabular-nums text-zinc-500">
            {formatQuotaNumber(window.total ?? 0)}
          </span>
          <span className="ml-auto text-[10px] font-medium text-zinc-400">{t("usage.used")}</span>
        </div>
      )}

      <ProgressTrack usedPercent={percent} size="compact" tone="brand-urgency" />

      <div className="flex items-center justify-between gap-2 text-[9px]">
        <span className={cn("font-mono font-semibold tabular-nums", tone.text)}>
          {remainingAbs != null
            ? t("usage.quotaRemaining", { remaining: formatQuotaNumber(remainingAbs) })
            : t("usage.remainingPercent", { percent: remainingPct })}
        </span>
        {!hasAbsolute && (
          <span className="tabular-nums text-zinc-400">
            {t("usage.used")} {percent}%
          </span>
        )}
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
