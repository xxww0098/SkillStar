import { invoke, isTauri } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { BadgeCheck, Pin, PinOff, RefreshCw, TriangleAlert, Unplug, X } from "lucide-react";
import { useCallback, useEffect, useMemo, useRef, useState, type MouseEvent } from "react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { PlanBadge } from "./PlanBadge";
import { ProviderLogo, hasBrandIcon } from "./ProviderLogo";
import { ResetCountdown } from "./ResetCountdown";
import { LightBodySurface, UsageCardBody, resolveUsageBodyRegistration } from "./card";
import { getBrandTheme } from "../lib/brandThemes";
import { cliAccountBadgeFor, isDegradedCopyBinding } from "../lib/cliCustody";
import { computeBodyOwnsPrimaryReset } from "../lib/resetOwnership";
import { authModeLabel, getPrimaryResetInfo, subscriptionCardTitle } from "../lib/usageLabels";
import { usageApi } from "../api";
import {
  USAGE_ACTIVE_CHANGED_EVENT,
  type ActiveChangedPayload,
  type CatalogEntry,
  type CliAccountState,
  type Subscription,
  type SwitchOutcome,
} from "../types";
import { cn } from "@/lib/utils";

const AUTO_REFRESH_MS = 60_000;

/**
 * Resolve the subscription id this card window is bound to.
 *
 * The backend opens the window with `?window=usage-card&id=<sub_id>`; we read
 * it from the URL search params (works in both Tauri webview and vitest).
 */
function readSubscriptionId(): string | null {
  if (typeof window === "undefined") return null;
  const params = new URLSearchParams(window.location.search);
  return params.get("id");
}

/**
 * Floating usage card window — a stripped-down root rendered when the Tauri
 * window label starts with `usage-card-`. Shows one subscription's quota with
 * switch / re-sync actions, subscribes to active-account changes, and
 * auto-refreshes. Deliberately does NOT use UsageDataContext (it lives outside
 * the usage-mode provider tree).
 */
export function UsageCardWindow() {
  const { t } = useTranslation();
  const subscriptionId = useMemo(readSubscriptionId, []);
  const [subscription, setSubscription] = useState<Subscription | null>(null);
  const [catalog, setCatalog] = useState<CatalogEntry | null>(null);
  const [allCatalog, setAllCatalog] = useState<CatalogEntry[]>([]);
  /** What the CLIs are actually serving. This window shows the same badge as
   *  the grid, so it has to read the same truth — the pin alone would let it
   *  claim "current" for an account the CLI had already been moved off. */
  const [cliAccounts, setCliAccounts] = useState<Record<string, CliAccountState>>({});
  const [refreshing, setRefreshing] = useState(false);
  const [switching, setSwitching] = useState(false);
  const [syncing, setSyncing] = useState(false);
  const [alwaysOnTop, setAlwaysOnTop] = useState(true);
  const [error, setError] = useState<string | null>(null);

  const loadData = useCallback(
    async (subId: string) => {
      try {
        const subs = await usageApi.listSubscriptions();
        const target = subs.find((s) => s.id === subId) ?? null;
        setSubscription(target);
        try {
          setCliAccounts(await usageApi.reconcileCliAccounts());
        } catch {
          // Best-effort: the badge falls back to the pin rather than to a
          // louder claim, and the rest of the card still loads.
        }
        if (allCatalog.length === 0) {
          const cat = await usageApi.listCatalog();
          setAllCatalog(cat);
          setCatalog(target ? (cat.find((c) => c.id === target.catalog_id) ?? null) : null);
        } else {
          setCatalog(target ? (allCatalog.find((c) => c.id === target.catalog_id) ?? null) : null);
        }
        setError(null);
      } catch (e) {
        setError(e instanceof Error ? e.message : String(e));
      }
    },
    [allCatalog],
  );

  // Initial load + focus reload.
  useEffect(() => {
    if (!subscriptionId) return;
    void loadData(subscriptionId);
    const win = getCurrentWindow();
    let unlisten: (() => void) | null = null;
    win
      .onFocusChanged(({ payload: focused }) => {
        if (focused) void loadData(subscriptionId);
      })
      .then((fn) => {
        unlisten = fn;
      })
      .catch(() => {});
    return () => {
      unlisten?.();
    };
  }, [loadData, subscriptionId]);

  // Auto-refresh usage every 60s (silent).
  useEffect(() => {
    if (!subscriptionId) return;
    const timer = window.setInterval(() => {
      void (async () => {
        try {
          await usageApi.refreshSubscriptionUsage(subscriptionId);
          await loadData(subscriptionId);
        } catch {
          // silent
        }
      })();
    }, AUTO_REFRESH_MS);
    return () => window.clearInterval(timer);
  }, [loadData, subscriptionId]);

  // Refresh own is_active when any catalog's active account changes.
  useActiveChangedListener(subscription?.catalog_id ?? null, () => {
    if (subscriptionId) void loadData(subscriptionId);
  });

  const handleRefresh = useCallback(async () => {
    if (!subscriptionId || refreshing) return;
    setRefreshing(true);
    try {
      await usageApi.refreshSubscriptionUsage(subscriptionId);
      await loadData(subscriptionId);
    } finally {
      setRefreshing(false);
    }
  }, [loadData, refreshing, subscriptionId]);

  const reportSwitchOutcome = useCallback(
    (outcome: SwitchOutcome | null | undefined, displayName: string, isActive: boolean) => {
      if (!outcome) {
        toast.success(t("usage.activeAccountSet"), {
          description: displayName,
        });
        return;
      }
      if (outcome.success) {
        toast.success(t("usage.switchCliSuccess"), {
          description: `${displayName} → ${outcome.toolId} · ${t("usage.switchCliRestartHint")}`,
        });
        if (isDegradedCopyBinding(outcome)) {
          toast.warning(t("usage.switchCliCopyMode"), {
            description: t("usage.switchCliCopyModeHint"),
            duration: 8000,
          });
        }
        return;
      }
      if (outcome.error) {
        const message = isActive ? t("usage.switchCliFailed") : t("usage.switchNotApplied");
        toast.error(message, {
          description: outcome.error,
        });
      }
    },
    [t],
  );

  const handleSwitch = useCallback(async () => {
    if (!subscriptionId || switching || subscription?.is_active) return;
    setSwitching(true);
    try {
      const updated = await usageApi.setActiveSubscription(subscriptionId);
      // Keep local state in sync immediately (don't wait solely on list reload).
      setSubscription(updated);
      reportSwitchOutcome(updated.switch_result, updated.display_name, updated.is_active === true);
      await loadData(subscriptionId);
    } catch (err) {
      const msg = err instanceof Error ? err.message : String(err);
      setError(msg);
      toast.error(t("usage.switchAccountFailed"), { description: msg });
    } finally {
      setSwitching(false);
    }
  }, [loadData, reportSwitchOutcome, subscription?.is_active, subscriptionId, switching, t]);

  const handleResyncCli = useCallback(async () => {
    if (!subscription?.catalog_id || syncing) return;
    setSyncing(true);
    try {
      const outcome = await usageApi.switchActiveSubscriptionToCli(subscription.catalog_id);
      if (outcome.success) {
        toast.success(t("usage.switchCliSynced"), {
          description: `${outcome.toolId}: ${outcome.configPath} · ${t("usage.switchCliRestartHint")}`,
        });
        if (isDegradedCopyBinding(outcome)) {
          toast.warning(t("usage.switchCliCopyMode"), {
            description: t("usage.switchCliCopyModeHint"),
            duration: 8000,
          });
        }
      } else if (outcome.error) {
        toast.error(t("usage.switchCliSyncFailed"), {
          description: outcome.error,
        });
      }
      // Stash outcome on the local card so the failure banner updates without a full reload.
      setSubscription((prev) => (prev ? { ...prev, switch_result: outcome } : prev));
      await loadData(subscriptionId ?? "");
    } catch (err) {
      const msg = err instanceof Error ? err.message : String(err);
      toast.error(t("usage.switchCliSyncFailed"), { description: msg });
    } finally {
      setSyncing(false);
    }
  }, [loadData, subscription?.catalog_id, subscriptionId, syncing, t]);

  const handleClose = useCallback(async () => {
    try {
      await invoke("close_usage_card_window", { subscriptionId });
    } catch {
      await getCurrentWindow().close();
    }
  }, [subscriptionId]);

  const handleTogglePin = useCallback(async () => {
    const next = !alwaysOnTop;
    setAlwaysOnTop(next);
    try {
      await getCurrentWindow().setAlwaysOnTop(next);
    } catch {
      setAlwaysOnTop(!next);
    }
  }, [alwaysOnTop]);

  const handleDragStart = useCallback((event: MouseEvent<HTMLElement>) => {
    if (event.button !== 0) return;
    const target = event.target as Element | null;
    if (target?.closest("button, select, input, a, [data-no-drag]")) return;
    event.preventDefault();
    void getCurrentWindow()
      .startDragging()
      .catch(() => {});
  }, []);

  if (!subscriptionId) {
    return (
      <div className="usage-card-root flex h-screen items-center justify-center p-4 text-sm text-muted-foreground">
        {t("usage.cardMissingId")}
      </div>
    );
  }

  if (!subscription) {
    return (
      <div className="usage-card-root flex h-screen items-center justify-center p-4 text-sm text-muted-foreground">
        {error ?? t("common.loading")}
      </div>
    );
  }

  const usage = subscription.usage ?? null;
  const planName = (usage?.plan_name ?? subscription.plan_tier ?? null) || null;
  const resetInfo = getPrimaryResetInfo(usage);
  const reg = resolveUsageBodyRegistration(subscription.catalog_id);
  const bodyOwnsPrimaryReset = computeBodyOwnsPrimaryReset(usage, resetInfo, reg.ownsPrimaryReset);
  const brandColor = catalog?.brand_color ?? "6B7280";
  const theme = getBrandTheme(subscription.catalog_id, brandColor);
  const hasIcon = hasBrandIcon(subscription.catalog_id);
  const cliFailed =
    subscription.supports_cli_switch && subscription.switch_result && !subscription.switch_result.success;
  const cliBadge = cliAccountBadgeFor(subscription, cliAccounts);
  const copyBound = isDegradedCopyBinding(subscription.switch_result);

  return (
    <div
      className={cn(
        "usage-card-root flex h-screen flex-col overflow-hidden rounded-xl border bg-card text-card-foreground shadow-2xl",
      )}
      style={{ borderColor: `${theme.glow}40` }}
    >
      {/* Drag handle header */}
      <div
        onMouseDown={handleDragStart}
        className="flex items-center gap-2 border-b border-border/40 px-3 py-2 select-none"
        style={{ background: `linear-gradient(135deg, ${theme.glow}22, transparent)` }}
      >
        <span className="flex h-6 w-6 items-center justify-center rounded-md" style={{ background: `${theme.glow}30` }}>
          {hasIcon ? (
            <ProviderLogo
              catalogId={subscription.catalog_id}
              displayName={catalog?.display_name ?? subscription.display_name}
              brandColor={brandColor}
              size="sm"
            />
          ) : (
            <span className="text-xs font-bold" style={{ color: theme.glow }}>
              {(catalog?.display_name ?? subscription.display_name).charAt(0)}
            </span>
          )}
        </span>
        <span className="flex-1 truncate text-sm font-semibold">
          {subscriptionCardTitle(
            subscription.display_name || catalog?.display_name || subscription.catalog_id,
            catalog?.display_name,
          )}
        </span>
        {cliBadge === "current" && (
          <span className="inline-flex items-center gap-0.5 rounded-full bg-emerald-500/15 px-1.5 py-0.5 text-[10px] font-medium text-emerald-500">
            <BadgeCheck size={10} />
            {t("usage.cardActive")}
          </span>
        )}
        {cliBadge === "diverged" && (
          <span
            className="inline-flex items-center gap-0.5 rounded-full bg-amber-500/15 px-1.5 py-0.5 text-[10px] font-medium text-amber-600"
            title={t("usage.cardCliDivergedTitle")}
          >
            <TriangleAlert size={10} />
            {t("usage.cardCliDiverged")}
          </span>
        )}
        {cliBadge === "missing" && (
          <span
            className="inline-flex items-center gap-0.5 rounded-full bg-muted px-1.5 py-0.5 text-[10px] font-medium text-muted-foreground"
            title={t("usage.cardCliMissingTitle")}
          >
            <Unplug size={10} />
            {t("usage.cardCliMissing")}
          </span>
        )}
        <button
          type="button"
          onClick={() => void handleTogglePin()}
          className="rounded p-1 text-muted-foreground hover:bg-foreground/10"
          title={alwaysOnTop ? t("usage.cardUnpin") : t("usage.cardPin")}
        >
          {alwaysOnTop ? <PinOff size={13} /> : <Pin size={13} />}
        </button>
        <button
          type="button"
          onClick={() => void handleClose()}
          className="rounded p-1 text-muted-foreground hover:bg-foreground/10"
          title={t("common.close")}
        >
          <X size={13} />
        </button>
      </div>

      {/* Body: dark chrome hosts light island with shared UsageCardBody (compact). */}
      <div className="flex min-h-0 flex-1 flex-col gap-2 overflow-y-auto p-3">
        <div className="flex flex-wrap items-center gap-x-1.5 gap-y-0.5 text-xs text-muted-foreground">
          <span className="truncate">{catalog?.display_name ?? subscription.catalog_id}</span>
          <span>·</span>
          <span>{authModeLabel(subscription.auth_mode, t)}</span>
          {subscription.requires_reauth && (
            <span className="rounded bg-amber-500/15 px-1 py-0.5 text-[10px] font-medium text-amber-600">
              {t("usage.reauthRequired")}
            </span>
          )}
          {subscription.note?.trim() ? (
            <span className="line-clamp-1 w-full text-[10px] text-muted-foreground/80" title={subscription.note}>
              {subscription.note.trim()}
            </span>
          ) : null}
        </div>

        <LightBodySurface theme={theme} className="min-h-0 flex-1 space-y-2.5 overflow-y-auto">
          {/* Island-top Meta: PlanBadge + primary Reset when body does not own it (K13b). */}
          <div className="flex flex-wrap items-center justify-between gap-2">
            <div className="min-w-0">{planName ? <PlanBadge plan={planName} /> : null}</div>
            {resetInfo && !bodyOwnsPrimaryReset && (
              <ResetCountdown
                resetAt={resetInfo.resetAt}
                usedPercent={resetInfo.usedPercent}
                mode={resetInfo.mode}
                className="text-[10px]"
              />
            )}
          </div>

          <UsageCardBody subscription={subscription} brandColorHex={brandColor} density="compact" surface="window" />
        </LightBodySurface>

        {cliBadge === "diverged" && (
          <div className="rounded-md border border-amber-500/30 bg-amber-500/10 px-2 py-1.5 text-[11px] text-amber-600">
            {t("usage.cardCliDivergedHint")}
          </div>
        )}
        {copyBound && (
          <div className="rounded-md border border-amber-500/20 bg-amber-500/5 px-2 py-1.5 text-[11px] text-amber-600">
            {t("usage.switchCliCopyModeHint")}
          </div>
        )}
        {cliFailed && subscription.switch_result?.error && (
          <div className="rounded-md border border-amber-500/30 bg-amber-500/10 px-2 py-1.5 text-[11px] text-amber-600">
            {subscription.switch_result.error}
          </div>
        )}
        {error && <div className="text-[11px] text-red-400">{error}</div>}
      </div>

      {/* Footer: actions */}
      <div className="flex items-center gap-1.5 border-t border-border/40 p-2" data-no-drag>
        {!subscription.is_active && (
          <button
            type="button"
            onClick={() => void handleSwitch()}
            disabled={switching}
            className="flex flex-1 items-center justify-center gap-1 rounded-md bg-emerald-600/90 px-2 py-1.5 text-xs font-medium text-white hover:bg-emerald-600 disabled:opacity-50"
          >
            {switching ? <RefreshCw size={12} className="animate-spin" /> : <BadgeCheck size={12} />}
            {t("usage.setActive")}
          </button>
        )}
        {/* Always offer CLI re-sync for active CLI-backed accounts (not only after a
            failed switch). Grok especially: a live `grok` process can overwrite
            auth.json, so users need a reliable re-push path. */}
        {subscription.is_active && subscription.supports_cli_switch && (
          <button
            type="button"
            onClick={() => void handleResyncCli()}
            disabled={syncing}
            className={cn(
              "flex flex-1 items-center justify-center gap-1 rounded-md px-2 py-1.5 text-xs font-medium disabled:opacity-50",
              cliFailed
                ? "border border-amber-500/40 text-amber-600 hover:bg-amber-500/10"
                : "border border-border/50 text-muted-foreground hover:bg-foreground/10",
            )}
          >
            {syncing ? <RefreshCw size={12} className="animate-spin" /> : <RefreshCw size={12} />}
            {t("usage.resyncCli")}
          </button>
        )}
        <button
          type="button"
          onClick={() => void handleRefresh()}
          disabled={refreshing}
          className="rounded-md border border-border/50 p-1.5 text-muted-foreground hover:bg-foreground/10 disabled:opacity-50"
          title={t("common.refresh")}
        >
          <RefreshCw size={13} className={refreshing ? "animate-spin" : undefined} />
        </button>
      </div>
    </div>
  );
}

/** Subscribe to `usage://active-changed` and refresh when our catalog changes. */
function useActiveChangedListener(catalogId: string | null, onActiveChanged: () => void) {
  const handlerRef = useRef(onActiveChanged);
  handlerRef.current = onActiveChanged;
  useEffect(() => {
    if (!isTauri() || !catalogId) return;
    let disposed = false;
    let unlisten: UnlistenFn | null = null;
    listen<ActiveChangedPayload>(USAGE_ACTIVE_CHANGED_EVENT, (e) => {
      if (e.payload?.catalogId === catalogId) handlerRef.current();
    })
      .then((fn) => {
        if (disposed) fn();
        else unlisten = fn;
      })
      .catch(() => {});
    return () => {
      disposed = true;
      unlisten?.();
    };
  }, [catalogId]);
}
