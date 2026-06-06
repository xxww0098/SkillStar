import { createContext, useCallback, useContext, useMemo, useRef, useState, type ReactNode } from "react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { useUsageAutoRefreshRunner, useUsageAutoRefreshSettings } from "../hooks/useUsageAutoRefresh";
import { useUsageData } from "../hooks/useUsageData";
import { formatUsageErrorForDisplay, truncateUsageError } from "../lib/usageErrors";
import type { SubscriptionUsage } from "../types";

type UsageDataContextValue = ReturnType<typeof useUsageData> & {
  refreshBusy: boolean;
  refreshingAll: boolean;
  refreshAllWithUi: () => Promise<void>;
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
  const queueRef = useRef(Promise.resolve());
  const pendingRef = useRef(0);

  const withRefreshLock = useCallback(async <T,>(run: () => Promise<T>): Promise<T> => {
    pendingRef.current += 1;
    setRefreshBusy(true);

    const task = queueRef.current.then(run);
    queueRef.current = task.then(
      () => undefined,
      () => undefined,
    );

    try {
      return await task;
    } finally {
      pendingRef.current -= 1;
      if (pendingRef.current === 0) {
        setRefreshBusy(false);
      }
    }
  }, []);

  const refreshAllWithUi = useCallback(async () => {
    await withRefreshLock(async () => {
      setRefreshingAll(true);
      try {
        await value.refreshAll();
      } finally {
        setRefreshingAll(false);
      }
    });
  }, [value.refreshAll, withRefreshLock]);

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
  const message = err instanceof Error ? err.message : String(err);
  return formatUsageErrorForDisplay(message, t) ?? "Unknown error";
}
