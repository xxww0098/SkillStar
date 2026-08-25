import type { ReactNode } from "react";
import { useTranslation } from "react-i18next";
import { cn } from "@/lib/utils";
import {
  formatQuotaNumber,
  formatUsdCents,
  formatAntigravityQuotaLabel,
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
  catalogId?: string;
  showCategoryReset?: boolean;
}

/**
 * Renders a usage quota bar. Every non-compact window flows through the shared
 * `UsageMeter` grammar so monetary, absolute, and percent-only quotas read as
 * one system across providers; compact windows stay a slim category row.
 */
export function UsageWindowBar({ window, compact, catalogId, showCategoryReset = true }: UsageWindowBarProps) {
  if (compact) {
    return <UsageCategoryBar window={window} catalogId={catalogId} showReset={showCategoryReset} />;
  }

  if (isMonetaryQuota(window)) {
    return <UsageQuotaPanel window={window} catalogId={catalogId} showCategoryReset={showCategoryReset} />;
  }

  if (isBreakdownQuotaWindow(window)) {
    return <UsageBreakdownQuotaPanel window={window} catalogId={catalogId} showCategoryReset={showCategoryReset} />;
  }

  if (isAbsoluteQuotaWindow(window)) {
    return <UsageStatsWindow window={window} />;
  }

  return <UsageSimpleWindow window={window} />;
}

/** Nested per-category breakdown, shared by monetary + percent parent windows. */
function BreakdownBlock({
  breakdown,
  catalogId,
  showCategoryReset,
}: {
  breakdown: UsageWindow[];
  catalogId?: string;
  showCategoryReset: boolean;
}) {
  const { t } = useTranslation();
  return (
    <div className="space-y-2 pt-1">
      <p className="text-[11px] font-semibold text-zinc-600">{t("usage.usageByCategory")}</p>
      <div className="space-y-2">
        {breakdown.map((sub, i) => (
          <UsageWindowBar
            key={`${sub.label}-${i}`}
            window={sub}
            compact
            catalogId={catalogId}
            showCategoryReset={showCategoryReset}
          />
        ))}
      </div>
    </div>
  );
}

function UsageQuotaPanel({
  window,
  catalogId,
  showCategoryReset,
}: {
  window: UsageWindow;
  catalogId?: string;
  showCategoryReset: boolean;
}) {
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
      {hasBreakdown ? (
        <BreakdownBlock breakdown={window.breakdown!} catalogId={catalogId} showCategoryReset={showCategoryReset} />
      ) : null}
    </UsageMeter>
  );
}

function UsageBreakdownQuotaPanel({
  window,
  catalogId,
  showCategoryReset,
}: {
  window: UsageWindow;
  catalogId?: string;
  showCategoryReset: boolean;
}) {
  const { t } = useTranslation();
  const percent = clamp(window.percent ?? computePercent(window.used, window.total));
  const remainingPct = Math.max(0, 100 - percent);
  const showRemainingBadge = catalogId === "antigravity";

  return (
    <UsageMeter
      label={localizeWindowLabel(window.label, t)}
      usedPercent={percent}
      badgePercent={showRemainingBadge ? remainingPct : undefined}
      badgeTitle={showRemainingBadge ? t("usage.remainingPercent", { percent: remainingPct }) : undefined}
      resetAt={window.reset_at}
      showReset={windowRendersOwnReset(window)}
      resetMode="billing"
      footNote={showRemainingBadge ? null : `${t("usage.remaining")} ${remainingPct}%`}
    >
      <BreakdownBlock breakdown={window.breakdown!} catalogId={catalogId} showCategoryReset={showCategoryReset} />
    </UsageMeter>
  );
}

function UsageCategoryBar({
  window,
  catalogId,
  showReset,
}: {
  window: UsageWindow;
  catalogId?: string;
  showReset: boolean;
}) {
  const { t } = useTranslation();
  const percent = clamp(window.percent ?? computePercent(window.used, window.total));
  const rawLabel = localizeCategoryLabel(window.label, t);
  const canonicalLabel = canonicalizeAntigravityModelName(rawLabel);
  const { display: label, title } =
    catalogId === "antigravity"
      ? formatAntigravityQuotaLabel(canonicalLabel, t)
      : { display: canonicalLabel, title: rawLabel };
  const tone = pickConsumedTone(percent);
  const monetary = isMonetaryQuota(window);
  const hasAbsolute = isAbsoluteQuotaWindow(window);
  const rightLabel = monetary
    ? `${formatUsdCents(window.used)}${window.total != null ? ` / ${formatUsdCents(window.total)}` : ""} · ${percent}%`
    : hasAbsolute
      ? `${formatQuotaNumber(window.used)} / ${formatQuotaNumber(window.total ?? 0)} · ${percent}%`
      : `${percent}%`;

  const hasInlineReset = catalogId === "antigravity" && showReset && Boolean(window.reset_at);
  const track = (
    <ProgressTrack
      className={hasInlineReset ? "w-full" : undefined}
      usedPercent={percent}
      size="category"
      tone="consumed"
    />
  );

  return (
    <div
      className={cn("space-y-1.5", hasInlineReset && "grid grid-cols-[minmax(0,1fr)_max-content] gap-x-2 gap-y-1.5")}
    >
      <div
        className={cn(
          "flex min-w-0 items-baseline justify-between gap-2 text-[10px] leading-tight",
          hasInlineReset && "col-span-2",
        )}
      >
        <span className="min-w-0 flex-1 truncate font-medium text-zinc-700" title={title}>
          {label}
        </span>
        <span className={cn("shrink-0 whitespace-nowrap text-right font-mono tabular-nums", tone.text)}>
          {rightLabel}
        </span>
      </div>
      {hasInlineReset ? (
        <>
          {track}
          <ResetCountdown
            resetAt={window.reset_at!}
            usedPercent={percent}
            mode="rateLimit"
            className="justify-self-end"
          />
        </>
      ) : (
        <>
          {track}
          {showReset && window.reset_at ? (
            <div className="flex justify-end">
              <ResetCountdown resetAt={window.reset_at} usedPercent={percent} mode="rateLimit" />
            </div>
          ) : null}
        </>
      )}
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
  const hasAbsolute = isAbsoluteQuotaWindow(window);
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
