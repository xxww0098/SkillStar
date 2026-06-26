import { motion } from "framer-motion";
import { Wallet } from "lucide-react";
import { useEffect, useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { useUsageDataContext } from "../context/UsageDataContext";
import { FILTER_ALL, type CatalogFilter, type Subscription } from "../types";
import { SubscriptionEditDialog } from "./SubscriptionEditDialog";
import { UsageActionBar } from "./UsageActionBar";
import { UsageAlertBanner } from "./UsageAlertBanner";
import { UsageGrid } from "./UsageGrid";
import { UsageRefreshControl } from "./UsageRefreshControl";

interface UsagePanelProps {
  filter: CatalogFilter;
  usageCreateRequest: { nonce: number; preselectCatalogId: string | null } | null;
  clearUsageCreateRequest: () => void;
}

export function UsagePanel({ filter, usageCreateRequest, clearUsageCreateRequest }: UsagePanelProps) {
  const { t } = useTranslation();
  const data = useUsageDataContext();
  const [dialogOpen, setDialogOpen] = useState(false);
  const [editingSub, setEditingSub] = useState<Subscription | null>(null);
  const [preselectId, setPreselectId] = useState<string | null>(null);
  const filtered = useMemo(() => {
    if (filter === FILTER_ALL) return data.subscriptions;
    return data.subscriptions.filter((s) => s.catalog_id === filter);
  }, [data.subscriptions, filter]);

  const openCreate = (catalogId?: string | null) => {
    const resolved = catalogId ?? (filter === FILTER_ALL ? null : filter);
    if (!resolved) {
      toast.info(t("usage.pickProviderFromSidebar"));
      return;
    }
    setEditingSub(null);
    setPreselectId(resolved);
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

  return (
    <div className="flex h-full flex-col overflow-hidden">
      <Header
        onRefresh={data.refreshAllWithUi}
        refreshing={data.refreshingAll}
        refreshDisabled={data.refreshBusy}
        autoRefreshEnabled={data.autoRefresh.autoRefreshEnabled}
        intervalMs={data.autoRefresh.intervalMs}
        setAutoRefreshEnabled={data.autoRefresh.setAutoRefreshEnabled}
        setIntervalMs={data.autoRefresh.setIntervalMs}
      />
      <UsageActionBar
        subscriptions={data.subscriptions}
        allSubscriptions={data.subscriptions}
        catalog={data.catalog}
        filter={filter}
        onReorder={data.reorder}
      />
      <UsageAlertBanner alerts={data.alerts} onDismiss={data.dismissAlert} />
      <main className="flex min-h-0 flex-1 flex-col overflow-hidden">
        {data.loading ? (
          <div className="flex flex-1 items-center justify-center text-sm text-muted-foreground">
            {t("usage.loading")}
          </div>
        ) : data.error ? (
          <div className="flex flex-1 items-center justify-center text-sm text-red-400">
            {t("usage.loadError", { error: data.error })}
          </div>
        ) : (
          <UsageGrid
            subscriptions={filtered}
            allSubscriptions={data.subscriptions}
            catalog={data.catalog}
            filter={filter}
            onReorder={data.reorder}
            onBrowseProviders={() => toast.info(t("usage.pickProviderFromSidebar"))}
            onRefresh={data.refreshOneWithUi}
            refreshDisabled={data.refreshBusy}
            onEdit={openEdit}
            onDelete={(id) => {
              void data.remove(id);
            }}
            onReauth={(id) => {
              openEdit(id);
            }}
            onSetActive={async (id) => {
              try {
                const updated = await data.setActive(id);
                const outcome = updated.switch_result ?? null;
                if (outcome && !outcome.success && outcome.error) {
                  // Active flag updated, but the real CLI config wasn't — tell
                  // the user why (e.g. missing id_token, keychain write fail).
                  toast.error(t("usage.switchCliFailed", "已切为当前账号，但同步到 CLI 失败"), {
                    description: outcome.error,
                  });
                } else if (outcome && outcome.success) {
                  toast.success(t("usage.switchCliSuccess", "已切为当前账号并同步到 CLI"), {
                    description: `${updated.display_name} → ${outcome.toolId}`,
                  });
                } else {
                  toast.success(t("usage.activeAccountSet", "已切为当前账号"), {
                    description: updated.display_name,
                  });
                }
              } catch (err) {
                toast.error(err instanceof Error ? err.message : String(err));
              }
            }}
            onSwitchToCli={async (catalogId) => {
              try {
                const outcome = await data.switchActiveToCli(catalogId);
                if (outcome.success) {
                  toast.success(t("usage.switchCliSuccess", "已同步到 CLI"), {
                    description: `${outcome.toolId}: ${outcome.configPath}`,
                  });
                } else if (outcome.error) {
                  toast.error(t("usage.switchCliFailed", "同步到 CLI 失败"), {
                    description: outcome.error,
                  });
                }
              } catch (err) {
                toast.error(err instanceof Error ? err.message : String(err));
              }
            }}
            onAddNew={(catalogId) => openCreate(catalogId ?? (filter === FILTER_ALL ? null : filter))}
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
        onDeleted={async () => {
          if (editingSub) {
            await data.remove(editingSub.id);
          }
          closeDialog();
        }}
      />
    </div>
  );
}

function Header({
  onRefresh,
  refreshing,
  refreshDisabled = false,
  autoRefreshEnabled,
  intervalMs,
  setAutoRefreshEnabled,
  setIntervalMs,
}: {
  onRefresh: () => Promise<void>;
  refreshing: boolean;
  refreshDisabled?: boolean;
  autoRefreshEnabled: boolean;
  intervalMs: number;
  setAutoRefreshEnabled: (enabled: boolean) => void;
  setIntervalMs: (intervalMs: number) => void;
}) {
  const { t } = useTranslation();
  return (
    <motion.header
      initial={{ opacity: 0, y: -8 }}
      animate={{ opacity: 1, y: 0 }}
      transition={{ duration: 0.2 }}
      data-tauri-drag-region
      className="flex h-14 shrink-0 items-center gap-3 border-b border-border/40 px-4"
    >
      <div className="flex shrink-0 items-center gap-3">
        <div className="flex h-8 w-8 items-center justify-center rounded-lg bg-primary/15 text-primary">
          <Wallet className="w-4 h-4" />
        </div>
        <div>
          <h1 className="text-base font-semibold leading-tight">{t("sidebar.usage")}</h1>
          <p className="text-[11px] text-muted-foreground">{t("usage.panelSubtitle")}</p>
        </div>
      </div>

      <div data-tauri-drag-region className="h-full min-w-[48px] flex-1" aria-hidden />

      <UsageRefreshControl
        onRefresh={onRefresh}
        refreshing={refreshing}
        refreshDisabled={refreshDisabled}
        autoRefreshEnabled={autoRefreshEnabled}
        intervalMs={intervalMs}
        setAutoRefreshEnabled={setAutoRefreshEnabled}
        setIntervalMs={setIntervalMs}
      />
    </motion.header>
  );
}
