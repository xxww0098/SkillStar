import { useCallback, useEffect, useState } from "react";
import { toast } from "sonner";
import { usageApi } from "../api";
import type {
  CatalogEntry,
  CreateSubscriptionInput,
  Subscription,
  SubscriptionAlert,
  UpdateSubscriptionInput,
  UsageSummary,
} from "../types";

/**
 * Single orchestrating hook for the usage page.
 *
 * Wraps the Tauri invoke calls into a friendlier React surface with simple
 * loading/error tracking. We don't pull in `@tanstack/react-query` here on
 * purpose — the page has ~20 rows and one polling source, so manual state
 * keeps deps minimal.
 */
export function useUsageData() {
  const [catalog, setCatalog] = useState<CatalogEntry[]>([]);
  const [subscriptions, setSubscriptions] = useState<Subscription[]>([]);
  const [summary, setSummary] = useState<UsageSummary | null>(null);
  const [alerts, setAlerts] = useState<SubscriptionAlert[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  const refreshSummary = useCallback(async () => {
    try {
      const [s, a] = await Promise.all([usageApi.getUsageSummary(), usageApi.getSubscriptionAlerts()]);
      setSummary(s);
      setAlerts(a);
    } catch (err) {
      console.warn("[usage] summary fetch failed", err);
    }
  }, []);

  const reload = useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      const [cat, subs] = await Promise.all([usageApi.listCatalog(), usageApi.listSubscriptions()]);
      setCatalog(cat);
      setSubscriptions(subs);
      await refreshSummary();
    } catch (err) {
      const msg = err instanceof Error ? err.message : String(err);
      setError(msg);
    } finally {
      setLoading(false);
    }
  }, [refreshSummary]);

  useEffect(() => {
    void reload();
  }, [reload]);

  const create = useCallback(
    async (input: CreateSubscriptionInput) => {
      try {
        const created = await usageApi.createSubscription(input);
        setSubscriptions((prev) => [...prev, created]);
        await refreshSummary();
        toast.success("订阅已添加");
        return created;
      } catch (err) {
        const msg = err instanceof Error ? err.message : String(err);
        toast.error(`添加失败：${msg}`);
        throw err;
      }
    },
    [refreshSummary],
  );

  const update = useCallback(
    async (id: string, input: UpdateSubscriptionInput) => {
      const updated = await usageApi.updateSubscription(id, input);
      setSubscriptions((prev) => prev.map((s) => (s.id === id ? updated : s)));
      await refreshSummary();
      return updated;
    },
    [refreshSummary],
  );

  const remove = useCallback(
    async (id: string) => {
      await usageApi.deleteSubscription(id);
      setSubscriptions((prev) => prev.filter((s) => s.id !== id));
      await refreshSummary();
    },
    [refreshSummary],
  );

  const refreshOne = useCallback(
    async (id: string) => {
      const updated = await usageApi.refreshSubscriptionUsage(id);
      setSubscriptions((prev) => prev.map((s) => (s.id === id ? updated : s)));
      await refreshSummary();
      return updated;
    },
    [refreshSummary],
  );

  const refreshAll = useCallback(async () => {
    const fresh = await usageApi.refreshAllSubscriptions();
    setSubscriptions(fresh);
    await refreshSummary();
  }, [refreshSummary]);

  const reorder = useCallback(async (orderedIds: string[]) => {
    // Optimistically reorder locally; backend persistence follows.
    setSubscriptions((prev) => {
      const map = new Map(prev.map((s) => [s.id, s]));
      return orderedIds.flatMap((id) => {
        const found = map.get(id);
        return found ? [found] : [];
      });
    });
    try {
      await usageApi.reorderSubscriptions(orderedIds);
    } catch (err) {
      console.warn("[usage] reorder persist failed", err);
    }
  }, []);

  const dismissAlert = useCallback(async (alertId: string) => {
    await usageApi.dismissSubscriptionAlert(alertId);
    setAlerts((prev) => prev.filter((a) => a.id !== alertId));
  }, []);

  return {
    catalog,
    subscriptions,
    summary,
    alerts,
    loading,
    error,
    reload,
    create,
    update,
    remove,
    refreshOne,
    refreshAll,
    reorder,
    dismissAlert,
  };
}
