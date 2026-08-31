import { BadgeCheck, ExternalLink, Pencil, RefreshCw, RotateCcw, ShieldAlert, Trash2 } from "lucide-react";
import { useState } from "react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { Button } from "@/components/ui/button";
import { openExternalUrl } from "@/lib/externalOpen";
import { cn } from "@/lib/utils";
import { formatCurrencyAmount } from "../../lib/pricing";
import { formatRelativeSync } from "../../lib/usageLabels";
import type { Subscription } from "../../types";
import { usageCardSlotClassName } from "./usageCardShell";

export interface UsageCardFooterProps {
  subscription: Subscription;
  monthlyCost: number | null;
  showRenewFooter: boolean;
  renewDays: number | null;
  fetchedAt?: number;
  subscriptionUrl?: string | null;
  onRefresh: () => void | Promise<void>;
  onResetQuota?: () => void | Promise<void>;
  /** Authoritative provider-side reset credits; null means not loaded. */
  resetCreditsRemaining?: number | null;
  onEdit: () => void;
  onDelete: () => void;
  onReauth?: () => void;
  onSetActive?: () => Promise<void>;
  onSwitchToCli?: () => Promise<void>;
  refreshDisabled?: boolean;
}

/** Optional cost/renew line + action icons + delete confirm overlay. */
export function UsageCardFooter({
  subscription: sub,
  monthlyCost,
  showRenewFooter,
  renewDays,
  fetchedAt = 0,
  subscriptionUrl,
  onRefresh,
  onResetQuota,
  resetCreditsRemaining = null,
  onEdit,
  onDelete,
  onReauth,
  onSetActive,
  onSwitchToCli,
  refreshDisabled = false,
}: UsageCardFooterProps) {
  const { t } = useTranslation();
  const [refreshing, setRefreshing] = useState(false);
  const [resetting, setResetting] = useState(false);
  const [activating, setActivating] = useState(false);
  const [cliSyncing, setCliSyncing] = useState(false);
  const [deletePending, setDeletePending] = useState(false);
  const [resetPending, setResetPending] = useState(false);

  const handleRefresh = async () => {
    setRefreshing(true);
    try {
      await onRefresh();
    } finally {
      setRefreshing(false);
    }
  };

  const handleResetQuota = async () => {
    if (!onResetQuota) return;
    setResetting(true);
    try {
      await onResetQuota();
    } finally {
      setResetting(false);
    }
  };

  const handleSetActive = async () => {
    if (!onSetActive || sub.is_active) return;
    setActivating(true);
    try {
      await onSetActive();
    } finally {
      setActivating(false);
    }
  };

  const handleSwitchToCli = async () => {
    if (!onSwitchToCli) return;
    setCliSyncing(true);
    try {
      await onSwitchToCli();
    } finally {
      setCliSyncing(false);
    }
  };

  /** Open this vendor's official console / renew page in the system browser. */
  const handleOpenConsole = (e?: { stopPropagation: () => void }) => {
    e?.stopPropagation();
    if (!subscriptionUrl) {
      toast.error(t("usage.openConsoleFailed"), {
        description: t("usage.openConsoleMissingUrl"),
      });
      return;
    }
    void openExternalUrl(subscriptionUrl).then((ok) => {
      if (!ok) {
        toast.error(t("usage.openConsoleFailed"), { description: subscriptionUrl });
      }
    });
  };

  const renewLabel =
    showRenewFooter && renewDays !== null
      ? renewDays < 0
        ? t("usage.expired", { days: -renewDays })
        : renewDays === 0
          ? t("usage.expiresToday")
          : t("usage.renewInDays", { days: renewDays })
      : null;
  const renewUrgent = renewDays !== null && renewDays <= 7;
  const hasFacts = monthlyCost !== null || renewLabel !== null;
  const factsTitle = [
    monthlyCost !== null ? `${formatCurrencyAmount(monthlyCost, sub.currency)} ${t("usage.perMonth")}` : null,
    renewLabel,
  ]
    .filter(Boolean)
    .join(" · ");
  const refreshTitle = `${t("usage.syncUsage")} · ${formatRelativeSync(fetchedAt, t)}`;
  const resetCountLabel =
    resetCreditsRemaining === null ? null : t("usage.resetQuotaRemaining", { count: resetCreditsRemaining });
  const resetTitle = resetCreditsRemaining === 0 ? t("usage.resetQuotaUnavailable") : t("usage.resetQuota");

  return (
    <>
      <footer className={usageCardSlotClassName.footer}>
        <div className="flex w-full items-center gap-2">
          {hasFacts ? (
            <p className="min-w-0 flex-1 truncate text-[11px] text-zinc-500" title={factsTitle}>
              {monthlyCost !== null && (
                <span className="font-semibold tabular-nums text-zinc-700">
                  {formatCurrencyAmount(monthlyCost, sub.currency)}
                  <span className="ml-0.5 font-normal text-zinc-400">{t("usage.perMonth")}</span>
                </span>
              )}
              {monthlyCost !== null && renewLabel ? <span className="mx-1.5 text-zinc-300">·</span> : null}
              {renewLabel ? (
                <span className={cn("tabular-nums", renewUrgent ? "font-semibold text-amber-600" : "text-zinc-600")}>
                  {renewLabel}
                </span>
              ) : null}
            </p>
          ) : (
            <span className="min-w-0 flex-1" />
          )}
          <div className="flex shrink-0 items-center justify-end gap-0.5">
            {onSetActive && !sub.is_active && (
              <Button
                size="icon-sm"
                variant="ghost"
                title={t("usage.setActive")}
                onClick={() => void handleSetActive()}
                disabled={activating}
                className="text-zinc-500 hover:text-emerald-600"
              >
                <BadgeCheck className={cn("h-3.5 w-3.5", activating && "animate-pulse")} />
              </Button>
            )}
            {onSwitchToCli && sub.is_active && sub.supports_cli_switch && (
              <Button
                size="icon-sm"
                variant="ghost"
                title={t(sub.catalog_id === "antigravity" ? "usage.resyncAntigravity" : "usage.resyncCli")}
                onClick={() => void handleSwitchToCli()}
                disabled={cliSyncing}
                className={
                  sub.switch_result && !sub.switch_result.success
                    ? "text-amber-500 hover:text-amber-600"
                    : "text-zinc-500 hover:text-zinc-800"
                }
              >
                <RefreshCw className={cn("h-3.5 w-3.5", cliSyncing && "animate-spin")} />
              </Button>
            )}
            {subscriptionUrl ? (
              <Button
                size="icon-sm"
                variant="ghost"
                title={t("usage.renewConsole")}
                onClick={(e) => handleOpenConsole(e)}
                className="text-zinc-500 hover:text-zinc-800"
              >
                <ExternalLink className="h-3.5 w-3.5" />
              </Button>
            ) : null}
            {sub.requires_reauth ? (
              <Button
                size="icon-sm"
                variant="destructive"
                title={t("usage.requiresReauth")}
                onClick={() => onReauth?.()}
              >
                <ShieldAlert className="h-3.5 w-3.5" />
              </Button>
            ) : (
              <Button
                size="icon-sm"
                variant="ghost"
                title={refreshTitle}
                onClick={() => void handleRefresh()}
                disabled={refreshing || refreshDisabled}
                className="text-zinc-500 hover:text-zinc-800"
              >
                <RefreshCw className={cn("h-3.5 w-3.5", refreshing && "animate-spin")} />
              </Button>
            )}
            {onResetQuota && (
              <Button
                size={resetCountLabel ? "sm" : "icon-sm"}
                variant="ghost"
                title={resetCountLabel ? `${resetTitle} · ${resetCountLabel}` : resetTitle}
                aria-label={resetCountLabel ? `${resetTitle}，${resetCountLabel}` : resetTitle}
                onClick={() => setResetPending(true)}
                disabled={resetting || refreshing || refreshDisabled || resetCreditsRemaining === 0}
                className={cn(
                  "text-amber-600 hover:text-amber-700",
                  resetCountLabel && "h-8 gap-1 px-1.5 text-[10px] font-semibold tabular-nums",
                )}
              >
                <RotateCcw className={cn("h-3.5 w-3.5", resetting && "animate-spin")} />
                {resetCountLabel}
              </Button>
            )}
            <Button
              size="icon-sm"
              variant="ghost"
              title={t("common.edit")}
              onClick={onEdit}
              className="text-zinc-500 hover:text-zinc-800"
            >
              <Pencil className="h-3.5 w-3.5" />
            </Button>
            <Button
              size="icon-sm"
              variant="ghost"
              title={t("common.delete")}
              onClick={() => setDeletePending(true)}
              className="text-red-500 hover:bg-red-50 hover:text-red-600"
            >
              <Trash2 className="h-3.5 w-3.5" />
            </Button>
          </div>
        </div>
      </footer>

      {deletePending && (
        <div className="absolute inset-0 z-20 flex items-center justify-center rounded-3xl bg-white/90 backdrop-blur-sm">
          <div className="mx-4 rounded-2xl border border-red-200 bg-white p-5 shadow-xl">
            <p className="mb-1 text-sm font-semibold text-zinc-900">{t("usage.confirmDeleteTitle")}</p>
            <p className="mb-4 text-xs text-zinc-500">{t("usage.confirmDeleteMsg", { name: sub.display_name })}</p>
            <div className="flex justify-end gap-2">
              <Button size="sm" variant="ghost" onClick={() => setDeletePending(false)}>
                {t("common.cancel")}
              </Button>
              <Button
                size="sm"
                variant="destructive"
                onClick={() => {
                  setDeletePending(false);
                  onDelete();
                }}
              >
                {t("common.delete")}
              </Button>
            </div>
          </div>
        </div>
      )}

      {resetPending && (
        <div className="absolute inset-0 z-20 flex items-center justify-center rounded-3xl bg-white/90 backdrop-blur-sm">
          <div className="mx-4 rounded-2xl border border-amber-200 bg-white p-5 shadow-xl">
            <p className="mb-1 text-sm font-semibold text-zinc-900">{t("usage.resetQuotaConfirmTitle")}</p>
            <p className="mb-4 text-xs text-zinc-500">{t("usage.resetQuotaConfirmMsg", { name: sub.display_name })}</p>
            <div className="flex justify-end gap-2">
              <Button size="sm" variant="ghost" onClick={() => setResetPending(false)}>
                {t("common.cancel")}
              </Button>
              <Button
                size="sm"
                variant="default"
                disabled={resetting}
                onClick={() => {
                  setResetPending(false);
                  void handleResetQuota();
                }}
              >
                {t("usage.resetQuota")}
              </Button>
            </div>
          </div>
        </div>
      )}
    </>
  );
}
