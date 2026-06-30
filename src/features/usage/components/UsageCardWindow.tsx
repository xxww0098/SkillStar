import { invoke, isTauri } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { BadgeCheck, Pin, PinOff, RefreshCw, X } from "lucide-react";
import { useCallback, useEffect, useMemo, useRef, useState, type MouseEvent } from "react";
import { useTranslation } from "react-i18next";
import { PlanBadge } from "./PlanBadge";
import { ProviderLogo, hasBrandIcon } from "./ProviderLogo";
import { ResetCountdown } from "./ResetCountdown";
import { UsageWindowBar } from "./UsageWindowBar";
import { getBrandTheme } from "../lib/brandThemes";
import { authModeLabel, formatQuotaNumber, getPrimaryResetInfo } from "../lib/usageLabels";
import { usageApi } from "../api";
import type { CatalogEntry, Subscription } from "../types";
import { cn } from "@/lib/utils";

/** Payload of the `usage://active-changed` event broadcast by the backend. */
interface ActiveChangedPayload {
  catalogId: string;
  subscriptionId: string;
}

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

  const handleSwitch = useCallback(async () => {
    if (!subscriptionId || switching || subscription?.is_active) return;
    setSwitching(true);
    try {
      await usageApi.setActiveSubscription(subscriptionId);
      await loadData(subscriptionId);
    } finally {
      setSwitching(false);
    }
  }, [loadData, subscription?.is_active, subscriptionId, switching]);

  const handleResyncCli = useCallback(async () => {
    if (!subscription?.catalog_id || syncing) return;
    setSyncing(true);
    try {
      await usageApi.switchActiveSubscriptionToCli(subscription.catalog_id);
      await loadData(subscriptionId ?? "");
    } finally {
      setSyncing(false);
    }
  }, [loadData, subscription?.catalog_id, subscriptionId, syncing]);

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
        {t("usage.cardMissingId", "缺少订阅 id")}
      </div>
    );
  }

  if (!subscription) {
    return (
      <div className="usage-card-root flex h-screen items-center justify-center p-4 text-sm text-muted-foreground">
        {error ?? t("common.loading", "加载中...")}
      </div>
    );
  }

  const usage = subscription.usage ?? null;
  const planName = (usage?.plan_name ?? subscription.plan_tier ?? null) || null;
  const resetInfo = getPrimaryResetInfo(usage);
  const brandColor = catalog?.brand_color ?? "6B7280";
  const theme = getBrandTheme(subscription.catalog_id, brandColor);
  const hasIcon = hasBrandIcon(subscription.catalog_id);
  const cliFailed =
    subscription.supports_cli_switch && subscription.switch_result && !subscription.switch_result.success;

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
          {subscription.display_name || catalog?.display_name || subscription.catalog_id}
        </span>
        {subscription.is_active && (
          <span className="rounded-full bg-emerald-500/15 px-1.5 py-0.5 text-[10px] font-medium text-emerald-500">
            {t("usage.cardActive", "当前")}
          </span>
        )}
        <button
          type="button"
          onClick={() => void handleTogglePin()}
          className="rounded p-1 text-muted-foreground hover:bg-foreground/10"
          title={alwaysOnTop ? t("usage.cardUnpin", "取消置顶") : t("usage.cardPin", "置顶")}
        >
          {alwaysOnTop ? <PinOff size={13} /> : <Pin size={13} />}
        </button>
        <button
          type="button"
          onClick={() => void handleClose()}
          className="rounded p-1 text-muted-foreground hover:bg-foreground/10"
          title={t("common.close", "关闭")}
        >
          <X size={13} />
        </button>
      </div>

      {/* Body: quota */}
      <div className="flex min-h-0 flex-1 flex-col gap-3 overflow-y-auto p-3">
        <div className="flex items-center justify-between gap-2">
          <div className="min-w-0">
            <div className="truncate text-xs text-muted-foreground">
              {catalog?.display_name ?? subscription.catalog_id}
              {" · "}
              {authModeLabel(subscription.auth_mode, t)}
            </div>
            {planName && (
              <div className="mt-0.5 flex items-center gap-1.5">
                <PlanBadge plan={planName} />
              </div>
            )}
          </div>
        </div>

        {usage?.hourly && <UsageWindowBar window={usage.hourly} compact />}
        {usage?.weekly && <UsageWindowBar window={usage.weekly} compact />}
        {usage?.monthly && <UsageWindowBar window={usage.monthly} compact />}
        {usage?.balance && (
          <div className="rounded-md border border-border/40 bg-muted/30 px-2.5 py-1.5 text-xs">
            <span className="text-muted-foreground">{t("usage.cardBalance", "余额")}：</span>
            <span className="font-medium">
              {formatQuotaNumber(usage.balance.total)} {usage.balance.currency}
            </span>
          </div>
        )}
        {!usage?.hourly && !usage?.weekly && !usage?.monthly && !usage?.balance && (
          <div className="flex flex-1 items-center justify-center text-xs text-muted-foreground">
            {usage?.error ?? t("usage.cardNoData", "暂无用量数据")}
          </div>
        )}

        {resetInfo && (
          <ResetCountdown
            resetAt={resetInfo.resetAt}
            usedPercent={resetInfo.usedPercent}
            mode={resetInfo.mode}
            className="text-[10px]"
          />
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
            {t("usage.setActive", "切为当前账号")}
          </button>
        )}
        {subscription.is_active && subscription.supports_cli_switch && cliFailed && (
          <button
            type="button"
            onClick={() => void handleResyncCli()}
            disabled={syncing}
            className="flex flex-1 items-center justify-center gap-1 rounded-md border border-amber-500/40 px-2 py-1.5 text-xs font-medium text-amber-600 hover:bg-amber-500/10 disabled:opacity-50"
          >
            {syncing ? <RefreshCw size={12} className="animate-spin" /> : <RefreshCw size={12} />}
            {t("usage.resyncCli", "重新同步到 CLI")}
          </button>
        )}
        <button
          type="button"
          onClick={() => void handleRefresh()}
          disabled={refreshing}
          className="rounded-md border border-border/50 p-1.5 text-muted-foreground hover:bg-foreground/10 disabled:opacity-50"
          title={t("common.refresh", "刷新")}
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
    listen<ActiveChangedPayload>("usage://active-changed", (e) => {
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
