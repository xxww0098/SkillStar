import { motion } from "framer-motion";
import { RefreshCw, Wallet } from "lucide-react";
import { useEffect, useMemo, useState } from "react";
import { Button } from "@/components/ui/button";
import { useNavigation } from "@/hooks/useNavigation";
import { useUsageDataContext } from "../context/UsageDataContext";
import { FILTER_ALL, type Subscription, type UsageSummary } from "../types";
import { SubscriptionEditDialog } from "./SubscriptionEditDialog";
import { UsageAlertBanner } from "./UsageAlertBanner";
import { UsageGrid } from "./UsageGrid";

export function UsagePanel() {
  const data = useUsageDataContext();
  const {
    usageCatalogFilter: filter,
    usageCreateRequest,
    clearUsageCreateRequest,
    openUsageCreate,
  } = useNavigation();
  const [dialogOpen, setDialogOpen] = useState(false);
  const [editingSub, setEditingSub] = useState<Subscription | null>(null);
  const [preselectId, setPreselectId] = useState<string | null>(null);
  const [refreshingAll, setRefreshingAll] = useState(false);

  const filtered = useMemo(() => {
    if (filter === FILTER_ALL) return data.subscriptions;
    return data.subscriptions.filter((s) => s.catalog_id === filter);
  }, [data.subscriptions, filter]);

  const openCreate = (catalogId?: string | null) => {
    setEditingSub(null);
    setPreselectId(catalogId ?? (filter === FILTER_ALL ? null : filter));
    setDialogOpen(true);
  };

  const openEdit = (id: string) => {
    const sub = data.subscriptions.find((s) => s.id === id);
    if (!sub) return;
    setEditingSub(sub);
    setPreselectId(null);
    setDialogOpen(true);
  };

  const closeDialog = () => {
    setDialogOpen(false);
    setEditingSub(null);
    setPreselectId(null);
  };

  useEffect(() => {
    if (!usageCreateRequest) return;
    openCreate(usageCreateRequest.preselectCatalogId);
    clearUsageCreateRequest();
  }, [usageCreateRequest, clearUsageCreateRequest]);

  const refreshAll = async () => {
    setRefreshingAll(true);
    try {
      await data.refreshAll();
    } finally {
      setRefreshingAll(false);
    }
  };

  return (
    <div className="flex h-full flex-col overflow-hidden">
      <Header summary={data.summary} onRefreshAll={refreshAll} refreshing={refreshingAll} />
      <UsageAlertBanner alerts={data.alerts} onDismiss={data.dismissAlert} />
      <main className="flex min-h-0 flex-1 flex-col overflow-hidden">
        {data.loading ? (
          <div className="flex flex-1 items-center justify-center text-sm text-muted-foreground">加载中…</div>
        ) : data.error ? (
          <div className="flex flex-1 items-center justify-center text-sm text-red-400">加载失败：{data.error}</div>
        ) : (
          <UsageGrid
            subscriptions={filtered}
            catalog={data.catalog}
            onRefresh={async (id) => {
              await data.refreshOne(id);
            }}
            onEdit={openEdit}
            onReauth={(id) => {
              openEdit(id);
            }}
            onAddNew={() => openUsageCreate(filter === FILTER_ALL ? null : filter)}
          />
        )}
      </main>
      <SubscriptionEditDialog
        open={dialogOpen}
        catalog={data.catalog}
        editing={editingSub}
        preselectCatalogId={preselectId}
        onClose={closeDialog}
        onCreated={() => {
          closeDialog();
          void data.reload();
        }}
        onUpdated={() => {
          closeDialog();
          void data.reload();
        }}
      />
    </div>
  );
}

interface HeaderProps {
  summary: UsageSummary | null;
  onRefreshAll: () => void;
  refreshing: boolean;
}

function Header({ summary, onRefreshAll, refreshing }: HeaderProps) {
  return (
    <motion.header
      initial={{ opacity: 0, y: -8 }}
      animate={{ opacity: 1, y: 0 }}
      transition={{ duration: 0.2 }}
      className="flex items-center justify-between border-b border-border/40 px-4 py-3"
    >
      <div className="flex items-center gap-3">
        <div className="flex h-8 w-8 items-center justify-center rounded-lg bg-primary/15 text-primary">
          <Wallet className="w-4 h-4" />
        </div>
        <div>
          <h1 className="text-base font-semibold leading-tight">用量</h1>
          <p className="text-[11px] text-muted-foreground">订阅 · 用量 · 续费提醒</p>
        </div>
      </div>
      <div className="flex items-center gap-3">
        {summary && summary.monthly_spend.length > 0 && (
          <div className="text-right">
            <div className="text-[10px] uppercase tracking-wide text-muted-foreground">本月支出</div>
            <div className="text-sm font-semibold tabular-nums">
              {summary.monthly_spend.map((e, i) => (
                <span key={e.currency}>
                  {i > 0 && <span className="mx-1 text-muted-foreground/50">·</span>}
                  {e.currency === "CNY" ? "¥" : e.currency === "USD" ? "$" : ""}
                  {e.amount.toFixed(2)}
                </span>
              ))}
            </div>
          </div>
        )}
        <Button onClick={onRefreshAll} disabled={refreshing} size="sm" variant="outline">
          <RefreshCw className={refreshing ? "animate-spin" : ""} />
          刷新全部
        </Button>
      </div>
    </motion.header>
  );
}
