import { ChevronDown, ChevronUp } from "lucide-react";
import { useMemo } from "react";
import { useTranslation } from "react-i18next";
import { cn } from "@/lib/utils";
import { useUsageSpendExpanded } from "../hooks/useUsageSpendExpanded";
import { subscriptionHasSpend } from "../lib/pricing";
import { FILTER_ALL, type CatalogEntry, type CatalogFilter, type Subscription } from "../types";
import { UsageSpendSummary } from "./UsageSpendSummary";

interface UsageActionBarProps {
  subscriptions: Subscription[];
  allSubscriptions: Subscription[];
  catalog: CatalogEntry[];
  filter: CatalogFilter;
  onReorder: (orderedIds: string[]) => void;
}

export function UsageActionBar({ subscriptions, allSubscriptions, catalog, filter, onReorder }: UsageActionBarProps) {
  const { t } = useTranslation();
  const { expanded, toggle } = useUsageSpendExpanded();

  const visibleSubs = useMemo(() => {
    if (filter === FILTER_ALL) return subscriptions;
    return subscriptions.filter((sub) => sub.catalog_id === filter);
  }, [subscriptions, filter]);

  const visible = useMemo(() => visibleSubs.some(subscriptionHasSpend), [visibleSubs]);

  if (!visible) return null;

  return (
    <div
      className={cn("flex shrink-0 items-center gap-1.5 border-b border-border/40 px-4", expanded ? "py-2" : "py-1")}
    >
      {expanded && (
        <UsageSpendSummary
          subscriptions={visibleSubs}
          allSubscriptions={allSubscriptions}
          catalog={catalog}
          onReorder={onReorder}
          className="min-w-0 flex-1"
        />
      )}

      <button
        type="button"
        onClick={toggle}
        aria-expanded={expanded}
        aria-label={expanded ? t("usage.spendSummaryCollapse") : t("usage.spendSummaryExpand")}
        className={cn(
          "inline-flex size-6 shrink-0 items-center justify-center rounded-md text-muted-foreground transition-colors",
          "hover:bg-accent/10 hover:text-foreground",
          "focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-primary/50",
          !expanded && "ml-auto",
        )}
      >
        {expanded ? <ChevronUp className="size-3.5" /> : <ChevronDown className="size-3.5" />}
      </button>
    </div>
  );
}
