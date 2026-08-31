import { motion } from "framer-motion";
import { Eye, EyeOff, Wallet } from "lucide-react";
import { useEffect, useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { cn } from "@/lib/utils";
import { useUsageDataContext } from "../context/UsageDataContext";
import { isDegradedCopyBinding } from "../lib/cliCustody";
import { readHideAccountEmails, writeHideAccountEmails } from "../lib/accountPrivacy";
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
  const [hideAccountEmails, setHideAccountEmails] = useState(() => readHideAccountEmails());
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

  // `remove` / `reorder` / `dismissAlert` toast their own failure and roll the
  // list back, then rethrow so a caller can react. Nothing here needs to, but
  // the rejection must still be consumed — a bare `void data.remove(id)` left
  // an unhandled rejection and zero user-visible feedback.
  const settled = (op: Promise<unknown>) => {
    void op.catch(() => undefined);
  };

  const refreshScopeCatalogId = filter === FILTER_ALL ? null : filter;
  const refreshScopeLabel = useMemo(() => {
    if (!refreshScopeCatalogId) return null;
    return data.catalog.find((c) => c.id === refreshScopeCatalogId)?.display_name ?? refreshScopeCatalogId;
  }, [data.catalog, refreshScopeCatalogId]);

  return (
    <div className="flex h-full flex-col overflow-hidden">
      <Header
        onRefresh={() => data.refreshAllWithUi(refreshScopeCatalogId)}
        refreshing={data.refreshingAll}
        refreshDisabled={data.refreshBusy}
        refreshLabel={
          refreshScopeLabel ? t("usage.refreshProvider", { provider: refreshScopeLabel }) : t("usage.refreshAll")
        }
        autoRefreshEnabled={data.autoRefresh.autoRefreshEnabled}
        intervalMs={data.autoRefresh.intervalMs}
        setAutoRefreshEnabled={data.autoRefresh.setAutoRefreshEnabled}
        setIntervalMs={data.autoRefresh.setIntervalMs}
        hideAccountEmails={hideAccountEmails}
        onToggleAccountEmails={(hidden) => {
          setHideAccountEmails(hidden);
          writeHideAccountEmails(hidden);
        }}
      />
      <UsageActionBar
        subscriptions={data.subscriptions}
        allSubscriptions={data.subscriptions}
        catalog={data.catalog}
        filter={filter}
        onReorder={(ids) => settled(data.reorder(ids))}
      />
      <UsageAlertBanner alerts={data.alerts} onDismiss={(id) => settled(data.dismissAlert(id))} />
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
            cliAccounts={data.cliAccounts}
            hideAccountEmails={hideAccountEmails}
            filter={filter}
            onReorder={(ids) => settled(data.reorder(ids))}
            onBrowseProviders={() => toast.info(t("usage.pickProviderFromSidebar"))}
            onRefresh={data.refreshOneWithUi}
            onResetQuota={data.resetQuotaWithUi}
            refreshDisabled={data.refreshBusy}
            onEdit={openEdit}
            onDelete={(id) => settled(data.remove(id))}
            onReauth={(id) => {
              openEdit(id);
            }}
            onSetActive={async (id) => {
              try {
                const updated = await data.setActive(id);
                const outcome = updated.switch_result ?? null;
                const isAntigravity = updated.catalog_id === "antigravity";
                const isCursor = updated.catalog_id === "cursor";
                if (outcome && !outcome.success && outcome.error) {
                  const message = updated.is_active
                    ? t(
                        isAntigravity
                          ? "usage.switchAntigravityFailed"
                          : isCursor
                            ? "usage.switchCursorFailed"
                            : "usage.switchCliFailed",
                      )
                    : t("usage.switchNotApplied");
                  toast.error(message, {
                    description: outcome.error,
                  });
                } else if (outcome && outcome.success) {
                  toast.success(
                    t(
                      isAntigravity
                        ? "usage.switchAntigravitySuccess"
                        : isCursor
                          ? "usage.switchCursorSuccess"
                          : "usage.switchCliSuccess",
                    ),
                    {
                      description: `${updated.display_name} → ${outcome.toolId} · ${t(
                        isAntigravity
                          ? "usage.switchAntigravityRestartHint"
                          : isCursor
                            ? "usage.switchCursorRestartHint"
                            : "usage.switchCliRestartHint",
                      )}`,
                    },
                  );
                  // A copy is a different deal from a link, and the user is the
                  // one who has to live with it.
                  if (isDegradedCopyBinding(outcome)) {
                    toast.warning(t("usage.switchCliCopyMode"), {
                      description: t("usage.switchCliCopyModeHint"),
                      duration: 8000,
                    });
                  }
                } else {
                  toast.success(t("usage.activeAccountSet"), {
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
                  const isAntigravity = catalogId === "antigravity";
                  const isCursor = catalogId === "cursor";
                  toast.success(
                    t(
                      isAntigravity
                        ? "usage.switchAntigravitySynced"
                        : isCursor
                          ? "usage.switchCursorSynced"
                          : "usage.switchCliSynced",
                    ),
                    {
                      description: `${outcome.toolId}: ${outcome.configPath} · ${t(
                        isAntigravity
                          ? "usage.switchAntigravityRestartHint"
                          : isCursor
                            ? "usage.switchCursorRestartHint"
                            : "usage.switchCliRestartHint",
                      )}`,
                    },
                  );
                  if (isDegradedCopyBinding(outcome)) {
                    toast.warning(t("usage.switchCliCopyMode"), {
                      description: t("usage.switchCliCopyModeHint"),
                      duration: 8000,
                    });
                  }
                } else if (outcome.error) {
                  toast.error(
                    t(
                      catalogId === "antigravity"
                        ? "usage.switchAntigravitySyncFailed"
                        : catalogId === "cursor"
                          ? "usage.switchCursorSyncFailed"
                          : "usage.switchCliSyncFailed",
                    ),
                    {
                      description: outcome.error,
                    },
                  );
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
        onDeleted={() => {
          // Close first: `remove` used to throw before `closeDialog()` ran,
          // leaving the dialog permanently open with no error shown.
          const target = editingSub;
          closeDialog();
          if (target) settled(data.remove(target.id));
        }}
      />
    </div>
  );
}

function Header({
  onRefresh,
  refreshing,
  refreshDisabled = false,
  refreshLabel,
  autoRefreshEnabled,
  intervalMs,
  setAutoRefreshEnabled,
  setIntervalMs,
  hideAccountEmails,
  onToggleAccountEmails,
}: {
  onRefresh: () => Promise<void>;
  refreshing: boolean;
  refreshDisabled?: boolean;
  refreshLabel?: string;
  autoRefreshEnabled: boolean;
  intervalMs: number;
  setAutoRefreshEnabled: (enabled: boolean) => void;
  setIntervalMs: (intervalMs: number) => void;
  hideAccountEmails: boolean;
  onToggleAccountEmails: (hidden: boolean) => void;
}) {
  const { t } = useTranslation();
  return (
    <motion.header
      initial={{ opacity: 0, y: -8 }}
      animate={{ opacity: 1, y: 0 }}
      transition={{ duration: 0.2 }}
      data-tauri-drag-region
      className="flex h-14 shrink-0 items-center gap-3 border-b border-border/70 bg-sidebar px-6"
    >
      <div className="flex shrink-0 items-center gap-3">
        <div className="flex h-8 w-8 items-center justify-center rounded-lg bg-primary/20 text-primary border border-primary/35 shadow-xs">
          <Wallet className="w-4 h-4" />
        </div>
        <div>
          <h1 className="text-sm font-bold text-foreground leading-tight tracking-tight">{t("sidebar.usage")}</h1>
          <p className="text-[11px] text-muted-foreground/80 font-medium">{t("usage.panelSubtitle")}</p>
        </div>
      </div>

      <div data-tauri-drag-region className="h-full min-w-[48px] flex-1" aria-hidden />

      <div className="flex shrink-0 items-center gap-1.5">
        <button
          type="button"
          aria-pressed={hideAccountEmails}
          aria-label={t(hideAccountEmails ? "usage.showAccountEmails" : "usage.hideAccountEmails")}
          title={t(hideAccountEmails ? "usage.showAccountEmails" : "usage.hideAccountEmails")}
          onClick={() => onToggleAccountEmails(!hideAccountEmails)}
          className={cn(
            "inline-flex size-8 items-center justify-center rounded-md border transition-colors",
            "focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-primary/50",
            hideAccountEmails
              ? "border-primary/40 bg-primary/10 text-primary"
              : "border-border/80 bg-background/50 text-muted-foreground hover:bg-accent/10 hover:text-foreground",
          )}
        >
          {hideAccountEmails ? <Eye className="size-4" /> : <EyeOff className="size-4" />}
        </button>
        <UsageRefreshControl
          onRefresh={onRefresh}
          refreshing={refreshing}
          refreshDisabled={refreshDisabled}
          refreshLabel={refreshLabel}
          autoRefreshEnabled={autoRefreshEnabled}
          intervalMs={intervalMs}
          setAutoRefreshEnabled={setAutoRefreshEnabled}
          setIntervalMs={setIntervalMs}
        />
      </div>
    </motion.header>
  );
}
