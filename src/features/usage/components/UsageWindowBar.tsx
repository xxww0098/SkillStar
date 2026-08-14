import type { ReactNode } from "react";
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
import { windowRendersOwnReset } from "../lib/resetOwnership";
import type { UsageWindow } from "../types";
import { MeterFigure, ProgressTrack, UsageMeter } from "./card/primitives";
import { ResetCountdown } from "./ResetCountdown";

interface UsageWindowBarProps {
  window: UsageWindow;
  compact?: boolean;
}

/**
 * Renders a usage quota bar. Every non-compact window flows through the shared
 * `UsageMeter` grammar so monetary, absolute, and percent-only quotas read as
 * one system across providers; compact windows stay a slim category row.
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

/** Nested per-category breakdown, shared by monetary + percent parent windows. */
function BreakdownBlock({ breakdown }: { breakdown: UsageWindow[] }) {
  const { t } = useTranslation();
  return (
    <div className="space-y-2 rounded-xl border border-zinc-200/70 bg-zinc-50/60 p-2.5">
      <div>
        <p className="text-[10px] font-bold text-zinc-800">{t("usage.usageByCategory")}</p>
        <p className="mt-0.5 text-[10px] leading-snug text-zinc-500">{t("usage.categoryUsageHint")}</p>
      </div>
      <div className="space-y-2.5">
        {breakdown.map((sub, i) => (
          <UsageWindowBar key={`${sub.label}-${i}`} window={sub} compact />
        ))}
      </div>
    </div>
  );
}

function UsageQuotaPanel({ window }: { window: UsageWindow }) {
  const { t } = useTranslation();
  const percent = clamp(window.percent ?? computePercent(window.used, window.total));
  const remainingPct = Math.max(0, 100 - percent);
  const remainingTone = pickRemainingTone(remainingPct, window.reset_at, percent);
  const total = window.total ?? 0;
  const remainingCents = Math.max(0, total - window.used);
  const hasBreakdown = (window.breakdown?.length ?? 0) > 0;

  return (
    <UsageMeter
      label={localizeWindowLabel(window.label, t)}
      dot="emerald"
      usedPercent={percent}
      figure={<MeterFigure value={formatUsdCents(window.used)} unit={`/ ${formatUsdCents(total)}`} />}
      caption={t("usage.includedQuota")}
      tone="billing-used"
      resetAt={window.reset_at}
      showReset={windowRendersOwnReset(window)}
      resetMode="billing"
      footNote={
        <>
          {t("usage.remaining")} {formatUsdCents(remainingCents)}
          <span className="ml-1 font-sans font-normal text-zinc-400">({remainingPct}%)</span>
        </>
      }
      footNoteClass={remainingTone.text}
    >
      {hasBreakdown ? <BreakdownBlock breakdown={window.breakdown!} /> : null}
    </UsageMeter>
  );
}

function UsageBreakdownQuotaPanel({ window }: { window: UsageWindow }) {
  const { t } = useTranslation();
  const percent = clamp(window.percent ?? computePercent(window.used, window.total));
  const remainingPct = Math.max(0, 100 - percent);

  return (
    <UsageMeter
      label={localizeWindowLabel(window.label, t)}
      usedPercent={percent}
      resetAt={window.reset_at}
      showReset={windowRendersOwnReset(window)}
      resetMode="billing"
      footNote={`${t("usage.remaining")} ${remainingPct}%`}
    >
      <BreakdownBlock breakdown={window.breakdown!} />
    </UsageMeter>
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
  const tone = pickConsumedTone(percent);

  return (
    <UsageMeter
      label={localizeWindowLabel(window.label, t)}
      usedPercent={percent}
      figure={<MeterFigure value={formatQuotaNumber(used)} unit={`/ ${formatQuotaNumber(total)}`} />}
      caption={t("usage.used")}
      tone="brand-urgency"
      resetAt={window.reset_at}
      showReset={windowRendersOwnReset(window)}
      resetMode="billing"
      footNote={
        remaining != null
          ? t("usage.quotaRemaining", { remaining: formatQuotaNumber(remaining) })
          : t("usage.remainingPercent", { percent: remainingPct })
      }
      footNoteClass={tone.text}
    />
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
  let figure: ReactNode = null;
  if (hasAbsolute) {
    figure = <MeterFigure value={formatQuotaNumber(window.used)} unit={`/ ${formatQuotaNumber(window.total ?? 0)}`} />;
  }

  return (
    <UsageMeter
      label={label}
      hint={isRateLimit ? t("usage.rateLimitWindow") : null}
      usedPercent={percent}
      figure={figure}
      caption={hasAbsolute ? t("usage.used") : null}
      tone="brand-urgency"
      resetAt={window.reset_at}
      showReset={windowRendersOwnReset(window)}
      resetMode={isRateLimit ? "rateLimit" : "billing"}
      footNote={
        remainingAbs != null
          ? t("usage.quotaRemaining", { remaining: formatQuotaNumber(remainingAbs) })
          : `${t("usage.remaining")} ${remainingPct}%`
      }
      footNoteClass={tone.text}
    />
  );
}

function clamp(p: number | null): number {
  if (p === null || Number.isNaN(p)) return 0;
  return Math.max(0, Math.min(100, Math.round(p)));
}

// `total` is `number | null | undefined`: the backend skips the key entirely
// when the quota ceiling is unknown, so it arrives as `undefined`, not `null`.
function computePercent(used: number, total: number | null | undefined): number | null {
  if (!total || total <= 0) return null;
  return Math.round((used / total) * 100);
}
