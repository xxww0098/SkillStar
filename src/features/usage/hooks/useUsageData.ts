import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { usageApi } from "../api";
import { describeUsageFailure } from "../lib/usageErrors";
import type {
  CatalogEntry,
  CliAccountState,
  CreateSubscriptionInput,
  Subscription,
  SubscriptionAlert,
  UpdateSubscriptionInput,
  UsageSummary,
} from "../types";

export function mergeActiveSubscriptionUpdate(subscriptions: Subscription[], updated: Subscription): Subscription[] {
  return subscriptions.map((subscription) => {
    if (subscription.id === updated.id) return updated;
    if (updated.is_active && subscription.catalog_id === updated.catalog_id) {
      return subscription.is_active ? { ...subscription, is_active: false } : subscription;
    }
    return subscription;
  });
}

/**
 * Re-project `is_active` from the backend's `catalog_id -> subscription_id`
 * map. Used by the `usage://active-changed` listener: a floating card window
 * can flip the active account, and the main grid must follow without paying
 * for a full provider refresh. Returns the original array when nothing moved
 * so React can skip the re-render.
 */
export function applyActiveSubscriptionMap(
  subscriptions: Subscription[],
  activeByCatalog: Record<string, string>,
): Subscription[] {
  let changed = false;
  const next = subscriptions.map((subscription) => {
    const isActive = activeByCatalog[subscription.catalog_id] === subscription.id;
    if ((subscription.is_active ?? false) === isActive) return subscription;
    changed = true;
    return { ...subscription, is_active: isActive };
  });
  return changed ? next : subscriptions;
}

/**
 * Single orchestrating hook for the usage page.
 *
 * Wraps the Tauri invoke calls into a friendlier React surface with simple
 * loading/error tracking. We don't pull in `@tanstack/react-query` here on
 * purpose — the page has ~20 rows and one polling source, so manual state
 * keeps deps minimal.
 *
 * **Every** path that writes `subscriptions`/`alerts`/`cliAccounts` runs
 * through one FIFO queue (`enqueue`). Reads and writes used to race: an
 * in-flight `refreshAll` replaces the whole table on resolve, so a `setActive`
 * / `reorder` committed while it was on the wire got silently reverted (the
 * "badge jumps back" bug). Serializing them means a mutation only starts once
 * the refresh it would have fought with has already landed its state — and the
 * CLI reconcile that decides the badge is inside the same queue, so it can
 * never report a live state from before the switch that was already applied.
 */
export function useUsageData() {
  const { t } = useTranslation();
  const [catalog, setCatalog] = useState<CatalogEntry[]>([]);
  const [subscriptions, setSubscriptions] = useState<Subscription[]>([]);
  /** `catalog_id -> which account that CLI is actually serving`. Empty until
   *  the first reconcile lands, which the badge reads as "fall back to the
   *  pin" rather than as "nothing is current". */
  const [cliAccounts, setCliAccounts] = useState<Record<string, CliAccountState>>({});
  const [summary, setSummary] = useState<UsageSummary | null>(null);
  const [alerts, setAlerts] = useState<SubscriptionAlert[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  // Latest committed lists, readable synchronously from an event handler so an
  // optimistic mutation can roll back to exactly what the user saw.
  const subscriptionsRef = useRef<Subscription[]>(subscriptions);
  const alertsRef = useRef<SubscriptionAlert[]>(alerts);
  useEffect(() => {
    subscriptionsRef.current = subscriptions;
  }, [subscriptions]);
  useEffect(() => {
    alertsRef.current = alerts;
  }, [alerts]);

  const queueRef = useRef<Promise<unknown>>(Promise.resolve());

  /** Run `task` after every previously queued list/refresh/mutation settled. */
  const enqueue = useCallback(<T>(task: () => Promise<T>): Promise<T> => {
    const run = queueRef.current.then(task);
    // Keep the chain non-rejecting so one failure can't wedge the queue.
    queueRef.current = run.then(
      () => undefined,
      () => undefined,
    );
    return run;
  }, []);

  /**
   * Re-read which account each CLI is actually serving.
   *
   * Deliberately **not** enqueued: every caller below is already running
   * inside a queue slot, and enqueueing from there would await a slot that
   * cannot start until the caller returns. `refreshCliAccounts` is the queued
   * entry point for callers outside the queue.
   *
   * Best-effort by design — a failure leaves the last known map in place, and
   * the badge falls back to the pin rather than to a louder claim.
   */
  const loadCliAccounts = useCallback(async () => {
    try {
      setCliAccounts(await usageApi.reconcileCliAccounts());
    } catch (err) {
      if (import.meta.env.DEV) console.warn("[usage] CLI account reconcile failed", err);
    }
  }, []);

  const refreshCliAccounts = useCallback(() => enqueue(loadCliAccounts), [enqueue, loadCliAccounts]);

  const refreshSummary = useCallback(async () => {
    try {
      const [s, a] = await Promise.all([usageApi.getUsageSummary(), usageApi.getSubscriptionAlerts()]);
      setSummary(s);
      setAlerts(a);
    } catch (err) {
      if (import.meta.env.DEV) console.warn("[usage] summary fetch failed", err);
    }
  }, []);

  const reload = useCallback(
    () =>
      enqueue(async () => {
        setLoading(true);
        setError(null);
        try {
          // Fire all four IPCs in one slot. Result-wrap list so a catalog/subs
          // reject cannot fail-fast and return while reconcile is still on the
          // wire (that would leak the FIFO past a live CLI repair).
          const catalogP = usageApi.listCatalog().then(
            (value) => ({ ok: true as const, value }),
            (error) => ({ ok: false as const, error }),
          );
          const subsP = usageApi.listSubscriptions().then(
            (value) => ({ ok: true as const, value }),
            (error) => ({ ok: false as const, error }),
          );
          const cliP = usageApi.reconcileCliAccounts().then(
            (value) => value,
            (err) => {
              if (import.meta.env.DEV) console.warn("[usage] CLI account reconcile failed", err);
              return null;
            },
          );
          const summaryP = Promise.all([usageApi.getUsageSummary(), usageApi.getSubscriptionAlerts()]).then(
            (pair) => pair,
            (err) => {
              if (import.meta.env.DEV) console.warn("[usage] summary fetch failed", err);
              return null;
            },
          );

          const [catR, subsR, cli, pair] = await Promise.all([catalogP, subsP, cliP, summaryP]);
          if (!catR.ok || !subsR.ok) {
            const err = !catR.ok ? catR.error : subsR.error;
            setError(err instanceof Error ? err.message : String(err));
            return;
          }
          setCatalog(catR.value);
          setSubscriptions(subsR.value);
          if (cli !== null) setCliAccounts(cli);
          if (pair !== null) {
            setSummary(pair[0]);
            setAlerts(pair[1]);
          }
        } catch (err) {
          const msg = err instanceof Error ? err.message : String(err);
          setError(msg);
        } finally {
          setLoading(false);
        }
      }),
    [enqueue],
  );

  useEffect(() => {
    void reload();
  }, [reload]);

  const create = useCallback(
    (input: CreateSubscriptionInput) =>
      enqueue(async () => {
        try {
          const created = await usageApi.createSubscription(input);
          setSubscriptions((prev) => [...prev, created]);
          await refreshSummary();
          toast.success(t("usage.subscriptionAdded"));
          return created;
        } catch (err) {
          const msg = err instanceof Error ? err.message : String(err);
          toast.error(t("usage.subscriptionAddFailed", { message: msg }));
          throw err;
        }
      }),
    [enqueue, refreshSummary, t],
  );

  const update = useCallback(
    (id: string, input: UpdateSubscriptionInput) =>
      enqueue(async () => {
        try {
          const updated = await usageApi.updateSubscription(id, input);
          setSubscriptions((prev) => prev.map((s) => (s.id === id ? updated : s)));
          await refreshSummary();
          return updated;
        } catch (err) {
          toast.error(t("usage.updateFailed", { error: describeUsageFailure(err, t) }));
          throw err;
        }
      }),
    [enqueue, refreshSummary, t],
  );

  const remove = useCallback(
    (id: string) =>
      enqueue(async () => {
        try {
          await usageApi.deleteSubscription(id);
        } catch (err) {
          // The row stays put; the user needs to be told why it did.
          toast.error(t("usage.deleteFailed", { error: describeUsageFailure(err, t) }));
          throw err;
        }
        setSubscriptions((prev) => prev.filter((s) => s.id !== id));
        // Deleting an account drops its snapshot and may hand the CLI back a
        // plain file, so the live state has moved.
        await loadCliAccounts();
        await refreshSummary();
      }),
    [enqueue, loadCliAccounts, refreshSummary, t],
  );

  const refreshOne = useCallback(
    (id: string) =>
      enqueue(async () => {
        const updated = await usageApi.refreshSubscriptionUsage(id);
        setSubscriptions((prev) => prev.map((s) => (s.id === id ? updated : s)));
        // A refresh can rotate the active account's credentials into the CLI.
        await loadCliAccounts();
        await refreshSummary();
        return updated;
      }),
    [enqueue, loadCliAccounts, refreshSummary],
  );

  const resetQuota = useCallback(
    (id: string) =>
      enqueue(async () => {
        const updated = await usageApi.resetSubscriptionQuota(id);
        setSubscriptions((prev) => prev.map((s) => (s.id === id ? updated : s)));
        await loadCliAccounts();
        await refreshSummary();
        return updated;
      }),
    [enqueue, loadCliAccounts, refreshSummary],
  );

  /** Refresh all subscriptions, or only those for `catalogId` when scoped to a
   *  provider page (e.g. `"xai"` on the Grok sidebar). Backend still returns
   *  the full list so local state can be replaced in one shot. */
  const refreshAll = useCallback(
    (catalogId?: string | null) =>
      enqueue(async () => {
        const fresh = await usageApi.refreshAllSubscriptions(catalogId);
        setSubscriptions(fresh);
        await loadCliAccounts();
        await refreshSummary();
      }),
    [enqueue, loadCliAccounts, refreshSummary],
  );

  const reorder = useCallback(
    (orderedIds: string[]) =>
      enqueue(async () => {
        // Optimistic: dragging must not wait on a round trip. `previous` is the
        // exact list the drop started from, so a rejected persist restores it
        // instead of leaving the UI claiming an order that was never saved.
        const previous = subscriptionsRef.current;
        setSubscriptions(() => {
          const map = new Map(previous.map((s) => [s.id, s]));
          return orderedIds.flatMap((id, index) => {
            const found = map.get(id);
            return found ? [{ ...found, sort_index: index }] : [];
          });
        });
        try {
          await usageApi.reorderSubscriptions(orderedIds);
        } catch (err) {
          setSubscriptions(previous);
          toast.error(t("usage.reorderFailed", { error: describeUsageFailure(err, t) }));
          throw err;
        }
      }),
    [enqueue, t],
  );

  const dismissAlert = useCallback(
    (alertId: string) =>
      enqueue(async () => {
        const previous = alertsRef.current;
        setAlerts((prev) => prev.filter((a) => a.id !== alertId));
        try {
          await usageApi.dismissSubscriptionAlert(alertId);
        } catch (err) {
          setAlerts(previous);
          toast.error(t("usage.dismissAlertFailed", { error: describeUsageFailure(err, t) }));
          throw err;
        }
      }),
    [enqueue, t],
  );

  /** Switch a subscription to be the active account for its catalog
   *  (Phase 7). The returned row is the backend's verdict — only when it comes
   *  back `is_active` do we demote the catalog's sibling — and the reconcile
   *  that follows re-reads what the CLI is now serving, so a refused switch
   *  leaves the old badge exactly where it was. */
  const setActive = useCallback(
    (id: string) =>
      enqueue(async () => {
        const updated = await usageApi.setActiveSubscription(id);
        setSubscriptions((prev) => mergeActiveSubscriptionUpdate(prev, updated));
        await loadCliAccounts();
        return updated;
      }),
    [enqueue, loadCliAccounts],
  );

  /** Re-read which row is active per catalog. Cheap (storage only, no provider
   *  network calls) — the `usage://active-changed` path, so a switch made in a
   *  floating card window updates the main grid's badge immediately. */
  const syncActiveAccounts = useCallback(
    () =>
      enqueue(async () => {
        try {
          const activeByCatalog = await usageApi.getActiveSubscriptions();
          setSubscriptions((prev) => applyActiveSubscriptionMap(prev, activeByCatalog));
        } catch (err) {
          if (import.meta.env.DEV) console.warn("[usage] active-account sync failed", err);
        }
        await loadCliAccounts();
      }),
    [enqueue, loadCliAccounts],
  );

  /** Re-push the active account's credentials to its CLI config (retry path
   *  for when a previous switch failed — e.g. missing id_token that has since
   *  been refreshed via OAuth). Returns the switch outcome for the caller to
   *  toast. */
  const switchActiveToCli = useCallback(
    (catalogId: string) =>
      enqueue(async () => {
        const outcome = await usageApi.switchActiveSubscriptionToCli(catalogId);
        await loadCliAccounts();
        return outcome;
      }),
    [enqueue, loadCliAccounts],
  );

  return useMemo(
    () => ({
      catalog,
      subscriptions,
      cliAccounts,
      summary,
      alerts,
      loading,
      error,
      reload,
      create,
      update,
      remove,
      refreshOne,
      resetQuota,
      refreshAll,
      reorder,
      dismissAlert,
      setActive,
      syncActiveAccounts,
      switchActiveToCli,
      refreshCliAccounts,
    }),
    [
      catalog,
      subscriptions,
      cliAccounts,
      summary,
      alerts,
      loading,
      error,
      reload,
      create,
      update,
      remove,
      refreshOne,
      resetQuota,
      refreshAll,
      reorder,
      dismissAlert,
      setActive,
      syncActiveAccounts,
      switchActiveToCli,
      refreshCliAccounts,
    ],
  );
}
