import { Reorder } from "framer-motion";
import { useMemo } from "react";
import { useTranslation } from "react-i18next";
import { HScrollRow } from "@/components/ui/HScrollRow";
import { cn } from "@/lib/utils";
import {
  formatCurrencyAmount,
  mergeSubscriptionOrder,
  monthlyEquivalentPrice,
  subscriptionHasSpend,
  totalSpendForSubscription,
} from "../lib/pricing";
import type { CatalogEntry, Subscription } from "../types";
import { ProviderLogo } from "./ProviderLogo";

const CHIP_MIN_WIDTH = 188;
const CHIP_GAP = 10;

interface UsageSpendSummaryProps {
  subscriptions: Subscription[];
  allSubscriptions: Subscription[];
  catalog: CatalogEntry[];
  onReorder: (orderedIds: string[]) => void;
  className?: string;
}

export function UsageSpendSummary({
  subscriptions,
  allSubscriptions,
  catalog,
  onReorder,
  className,
}: UsageSpendSummaryProps) {
  const catalogById = useMemo(() => new Map(catalog.map((entry) => [entry.id, entry])), [catalog]);

  const spendSubs = useMemo(() => {
    return [...subscriptions].filter(subscriptionHasSpend).sort((a, b) => a.sort_index - b.sort_index);
  }, [subscriptions]);

  const handleReorder = (reordered: Subscription[]) => {
    const movableIds = spendSubs.map((sub) => sub.id);
    const newMovableOrder = reordered.map((sub) => sub.id);
    const allIds = allSubscriptions.map((sub) => sub.id);
    onReorder(mergeSubscriptionOrder(allIds, movableIds, newMovableOrder));
  };

  if (spendSubs.length === 0) return null;

  return (
    <HScrollRow
      count={spendSubs.length}
      itemWidth={CHIP_MIN_WIDTH}
      gap={CHIP_GAP}
      className={cn("min-w-0 flex-1 gap-2.5", className)}
    >
      <Reorder.Group
        axis="x"
        layoutScroll
        values={spendSubs}
        onReorder={handleReorder}
        className="flex items-stretch gap-2.5"
      >
        {spendSubs.map((sub) => (
          <Reorder.Item
            key={sub.id}
            value={sub}
            className="shrink-0 cursor-grab touch-none active:cursor-grabbing"
            whileDrag={{ scale: 1.02, zIndex: 20 }}
            aria-label={sub.display_name || catalogById.get(sub.catalog_id)?.display_name || sub.catalog_id}
          >
            <SubscriptionSpendChip
              sub={sub}
              catalog={catalogById.get(sub.catalog_id)}
              total={totalSpendForSubscription(sub)}
              monthly={monthlyEquivalentPrice(sub)}
              showLabel={spendSubs.length > 1}
            />
          </Reorder.Item>
        ))}
      </Reorder.Group>
    </HScrollRow>
  );
}

interface SubscriptionSpendChipProps {
  sub: Subscription;
  catalog: CatalogEntry | undefined;
  total: number;
  monthly: number | null;
  showLabel: boolean;
}

function SubscriptionSpendChip({ sub, catalog, total, monthly, showLabel }: SubscriptionSpendChipProps) {
  const { t } = useTranslation();
  const title = sub.display_name || catalog?.display_name || sub.catalog_id;

  return (
    <div className="flex min-w-[188px] select-none items-center gap-2.5 rounded-2xl border border-border/55 bg-card/55 px-3 py-2 shadow-sm backdrop-blur-sm">
      <ProviderLogo
        catalogId={sub.catalog_id}
        displayName={title}
        brandColor={catalog?.brand_color ?? "6B7280"}
        size="md"
      />

      <div className="flex min-w-0 flex-1 items-stretch">
        {total > 0 && (
          <SpendStat
            label={t("usage.totalSpendShort")}
            value={formatCurrencyAmount(total, sub.currency)}
            className={monthly == null || monthly <= 0 ? "flex-1" : undefined}
          />
        )}
        {monthly != null && monthly > 0 && (
          <>
            {total > 0 && <div className="my-1 w-px shrink-0 bg-border/45" aria-hidden />}
            <SpendStat
              label={t("usage.monthlySpendShort")}
              value={formatCurrencyAmount(monthly, sub.currency)}
              accent
              className="flex-1"
            />
          </>
        )}
      </div>

      {showLabel && <span className="sr-only">{title}</span>}
    </div>
  );
}

function SpendStat({
  label,
  value,
  accent,
  className,
}: {
  label: string;
  value: string;
  accent?: boolean;
  className?: string;
}) {
  return (
    <div className={cn("flex min-w-[72px] flex-col justify-center px-2 py-0.5", className)}>
      <span className="text-[10px] leading-none text-muted-foreground">{label}</span>
      <span
        className={cn(
          "mt-1 truncate text-[13px] font-semibold tabular-nums leading-tight tracking-tight",
          accent ? "text-primary" : "text-foreground",
        )}
      >
        {value}
      </span>
    </div>
  );
}
