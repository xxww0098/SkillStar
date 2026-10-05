import { motion } from "framer-motion";
import { Gauge } from "lucide-react";
import { useMemo } from "react";
import { useTranslation } from "react-i18next";
import { useUsageDataContext } from "../context/UsageDataContext";
import { FILTER_ALL, type CatalogFilter } from "../types";
import { TodaySessions } from "./TodaySessions";
import { UsageAlertBanner } from "./UsageAlertBanner";
import { UsageSpendSummary } from "./UsageSpendSummary";
import { UsagePageSkeleton } from "./UsageSkeleton";

interface UsagePanelProps {
  filter: CatalogFilter;
}

/**
 * The Usage mode: read-only consumption display — today's sessions, the
 * spend summary, and quota alerts. Account management (cards, switching,
 * the edit dialog) lives in the Accounts mode; this panel never mutates a
 * subscription.
 */
export function UsagePanel({ filter }: UsagePanelProps) {
  const { t } = useTranslation();
  const data = useUsageDataContext();
  const filtered = useMemo(() => {
    if (filter === FILTER_ALL) return data.subscriptions;
    return data.subscriptions.filter((s) => s.catalog_id === filter);
  }, [data.subscriptions, filter]);

  const settled = (op: Promise<unknown>) => {
    void op.catch(() => undefined);
  };

  return (
    <div className="flex h-full flex-col overflow-hidden">
      <motion.header
        initial={{ opacity: 0, y: -8 }}
        animate={{ opacity: 1, y: 0 }}
        transition={{ duration: 0.2 }}
        data-tauri-drag-region
        className="flex h-12 shrink-0 items-center gap-3 border-b border-border/70 bg-sidebar px-4"
      >
        <div className="flex shrink-0 items-center gap-3">
          <div className="flex h-8 w-8 items-center justify-center rounded-lg bg-primary/20 text-primary border border-primary/35 shadow-xs">
            <Gauge className="w-4 h-4" />
          </div>
          <div>
            <h1 className="text-sm font-bold text-foreground leading-tight tracking-tight">{t("sidebar.usage")}</h1>
            <p className="text-[11px] text-muted-foreground/80 font-medium">{t("usage.panelSubtitle")}</p>
          </div>
        </div>
        <div data-tauri-drag-region className="h-full min-w-[48px] flex-1" aria-hidden />
      </motion.header>
      <UsageAlertBanner alerts={data.alerts} onDismiss={(id) => settled(data.dismissAlert(id))} />
      {data.loading ? (
        <UsagePageSkeleton />
      ) : data.error ? (
        <div className="flex flex-1 items-center justify-center text-sm text-red-400">
          {t("usage.loadError", { error: data.error })}
        </div>
      ) : (
        <main className="ss-page-scroll flex min-h-0 flex-1 flex-col gap-4 p-4">
          <TodaySessions today={data.todayConsumption} />
          <UsageSpendSummary
            subscriptions={filtered}
            allSubscriptions={data.subscriptions}
            catalog={data.catalog}
            onReorder={() => undefined}
            className="min-w-0"
          />
        </main>
      )}
    </div>
  );
}
