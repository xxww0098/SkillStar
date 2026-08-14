import { createContext, useCallback, useContext, useMemo, useRef, useState, type ReactNode } from "react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { useTauriEvent } from "@/hooks/useTauriEvent";
import { useUsageAutoRefreshRunner, useUsageAutoRefreshSettings } from "../hooks/useUsageAutoRefresh";
import { useUsageData } from "../hooks/useUsageData";
import { describeUsageFailure, formatUsageErrorForDisplay, truncateUsageError } from "../lib/usageErrors";
import { USAGE_ACTIVE_CHANGED_EVENT, type ActiveChangedPayload, type SubscriptionUsage } from "../types";

type UsageDataContextValue = ReturnType<typeof useUsageData> & {
  refreshBusy: boolean;
  refreshingAll: boolean;
  /** Optional `catalogId` scopes the batch to one provider (sidebar category). */
  refreshAllWithUi: (catalogId?: string | null) => Promise<void>;
  refreshOneWithUi: (id: string) => Promise<void>;
  autoRefresh: ReturnType<typeof useUsageAutoRefreshSettings>;
};

const UsageDataContext = createContext<UsageDataContextValue | null>(null);

export function UsageDataProvider({ children }: { children: ReactNode }) {
  const { t } = useTranslation();
  const value = useUsageData();
  const autoRefresh = useUsageAutoRefreshSettings();
  const [refreshBusy, setRefreshBusy] = useState(false);
  const [refreshingAll, setRefreshingAll] = useState(false);
  const pendingRef = useRef(0);

  /**
   * Track "a refresh is in flight" for the toolbar/card spinners.
   *
   * Serialization itself lives in `useUsageData` — one queue covering refresh
   * *and* the mutations, so `setActive`/`remove`/`reorder` can no longer be
   * overwritten by a `refreshAll` that was already on the wire. Queueing here
   * as well would deadlock (the outer slot would await an inner enqueue that
   * can never start).
   */
  const withRefreshLock = useCallback(async <T,>(run: () => Promise<T>): Promise<T> => {
    pendingRef.current += 1;
    setRefreshBusy(true);
    try {
      return await run();
    } finally {
      pendingRef.current -= 1;
      if (pendingRef.current === 0) {
        setRefreshBusy(false);
      }
    }
  }, []);

  const refreshAllWithUi = useCallback(
    async (catalogId?: string | null) => {
      await withRefreshLock(async () => {
        setRefreshingAll(true);
        try {
          await value.refreshAll(catalogId);
        } finally {
          setRefreshingAll(false);
        }
      });
    },
    [value.refreshAll, withRefreshLock],
  );

  const refreshOneWithUi = useCallback(
    async (id: string) => {
      const current = value.subscriptions.find((sub) => sub.id === id);
      const name = current?.display_name ?? t("usage.subscriptionFallbackName");
      const toastId = `usage-refresh-${id}`;
      toast.loading(t("usage.refreshOneInProgress", { name }), { id: toastId });

      try {
        const updated = await withRefreshLock(async () => value.refreshOne(id));
        const issue = getRefreshIssue(updated.usage, updated.catalog_id, t);

        if (updated.requires_reauth) {
          toast.warning(t("usage.refreshOneRequiresReauth", { name }), { id: toastId, duration: 6000 });
        } else if (issue?.kind === "error") {
          toast.warning(t("usage.refreshOneCompletedWithIssue", { name, error: issue.message }), {
            id: toastId,
            duration: 7000,
          });
        } else if (issue?.kind === "empty") {
          toast.info(
            issue.message
              ? t("usage.refreshOneNoVisibleDataWithReason", { name, reason: issue.message })
              : t("usage.refreshOneNoVisibleData", { name }),
            { id: toastId, duration: 6000 },
          );
        } else {
          toast.success(t("usage.refreshOneDone", { name }), { id: toastId });
        }
      } catch (err) {
        toast.error(t("usage.refreshOneFailed", { name, error: formatRefreshError(err, t) }), {
          id: toastId,
          duration: 7000,
        });
        throw err;
      }
    },
    [t, value.refreshOne, value.subscriptions, withRefreshLock],
  );

  useUsageAutoRefreshRunner(
    { enabled: autoRefresh.autoRefreshEnabled, intervalMs: autoRefresh.intervalMs },
    refreshAllWithUi,
    refreshBusy,
  );

  // A floating card window can pin a different account; without this the main
  // grid kept showing a stale "current" badge until the next manual refresh.
  const syncActiveAccounts = value.syncActiveAccounts;
  useTauriEvent<ActiveChangedPayload>(USAGE_ACTIVE_CHANGED_EVENT, () => {
    void syncActiveAccounts();
  });

  const ctx = useMemo(
    () => ({
      ...value,
      refreshBusy,
      refreshingAll,
      refreshAllWithUi,
      refreshOneWithUi,
      autoRefresh,
    }),
    [value, refreshBusy, refreshingAll, refreshAllWithUi, refreshOneWithUi, autoRefresh],
  );

  return <UsageDataContext.Provider value={ctx}>{children}</UsageDataContext.Provider>;
}

/** Usage page + sidebar nav share one data source while Usage mode is active. */
export function useUsageDataContext(): UsageDataContextValue {
  const ctx = useContext(UsageDataContext);
  if (!ctx) {
    throw new Error("useUsageDataContext must be used within UsageDataProvider");
  }
  return ctx;
}

function getRefreshIssue(
  usage: SubscriptionUsage | null,
  catalogId: string,
  t: ReturnType<typeof useTranslation>["t"],
): { kind: "empty" | "error"; message?: string } | null {
  if (!usage) {
    return { kind: "empty", message: emptyRefreshMessage(catalogId, t) };
  }

  const error = usage.error?.trim();
  if (error) {
    return { kind: "error", message: formatUsageErrorForDisplay(error, t) ?? truncateUsageError(error) };
  }

  const creditsCount = usage.credits?.length ?? 0;
  const apiKeysCount = usage.api_keys?.length ?? 0;

  if (!usage.hourly && !usage.weekly && !usage.monthly && !usage.balance && creditsCount === 0 && apiKeysCount === 0) {
    return { kind: "empty", message: emptyRefreshMessage(catalogId, t, usage.plan_name) };
  }

  return null;
}

function emptyRefreshMessage(
  catalogId: string,
  t: ReturnType<typeof useTranslation>["t"],
  planName?: string | null,
): string {
  if (catalogId === "cursor") {
    return planName ? t("usage.refreshCursorPlanOnlyData", { plan: planName }) : t("usage.refreshCursorNoReadableData");
  }

  return planName ? t("usage.refreshPlanOnlyData", { plan: planName }) : t("usage.refreshOneNoVisibleData");
}

function formatRefreshError(err: unknown, t: ReturnType<typeof useTranslation>["t"]): string {
  return describeUsageFailure(err, t);
}
