import { useTranslation } from "react-i18next";
import { cn } from "@/lib/utils";
import { formatQuotaNumber } from "../../lib/usageLabels";
import type { ConsumptionTotals } from "../../types";

/**
 * The card's「today」line (slice 09).
 *
 * Operate discipline: the quota meter stays the hero — this is one quiet
 * line at the bottom of the body, never a second KPI tile. Big mono digits
 * for the tokens, the cost always carries the「estimate」marker, the fixed
 *「no records yet」sentence when nothing landed, and no animation at all so
 * `prefers-reduced-motion` needs no branch here.
 *
 * Scope note baked into the copy: one card's numbers cover gateway-metered
 * calls only (`by_catalog` attribution); bypass traffic counts toward the
 * page-wide total, not toward any provider.
 */
export function TodayConsumptionLine({
  today,
  className,
  onNavigate,
}: {
  /** `null` = the read landed but this provider has no gateway traffic
   *  today; `undefined` (prop omitted) hides the line entirely. */
  today: ConsumptionTotals | null;
  className?: string;
  /** When set (and the line has traffic), the whole line becomes the quiet
   *  button into the cross-view: which agents route to this provider. */
  onNavigate?: () => void;
}) {
  const { t } = useTranslation();
  const empty = today === null || today.calls === 0;
  // Reasoning tokens are billed (and counted) inside output — do not add
  // them twice.
  const tokens = today === null ? 0 : today.input + today.output + today.cache_read + today.cache_write;
  const cost = today === null ? 0 : today.cost_usd;
  const interactive = onNavigate !== undefined && !empty;
  const body = (
    <>
      <span className="shrink-0 text-[10px] font-semibold tracking-wide text-zinc-500 uppercase">
        {t("usage.todayConsumptionLabel")}
      </span>
      {empty ? (
        <span className="text-[10px] font-medium text-zinc-400">{t("usage.todayConsumptionEmpty")}</span>
      ) : (
        <span className="flex min-w-0 items-baseline justify-end gap-1.5">
          <span className="font-mono text-base leading-none font-bold tabular-nums text-zinc-900">
            {formatQuotaNumber(tokens)}
          </span>
          <span className="font-mono text-[10px] font-semibold tabular-nums text-zinc-500">
            {t("usage.todayConsumptionCost", { cost: formatEstimateUsd(cost) })}
          </span>
        </span>
      )}
    </>
  );
  const shell = cn(
    "flex items-baseline justify-between gap-2 border-t border-zinc-100/80 px-1 pt-1.5",
    interactive && "w-full cursor-pointer text-left transition-colors hover:bg-muted/30",
    className,
  );

  if (interactive) {
    return (
      <button type="button" onClick={onNavigate} className={shell} data-testid="today-consumption-line">
        {body}
      </button>
    );
  }
  return (
    <div className={shell} data-testid="today-consumption-line" title={t("usage.todayConsumptionHint")}>
      {body}
    </div>
  );
}

/** `$x.xxxx` under a dollar, `$x.xx` above, `$0` for nothing — one quiet
 *  estimate figure. */
export function formatEstimateUsd(cost: number): string {
  if (!Number.isFinite(cost) || cost === 0) return "$0";
  return cost < 1 ? `$${cost.toFixed(4)}` : `$${cost.toFixed(2)}`;
}
