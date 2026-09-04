import { BadgeCheck, ExternalLink, Layers, Pencil, RefreshCw, RotateCcw, ShieldAlert, Trash2 } from "lucide-react";
import { useState } from "react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { Button } from "@/components/ui/button";
import { openExternalUrl } from "@/lib/externalOpen";
import { cn } from "@/lib/utils";
import { formatCurrencyAmount } from "../../lib/pricing";
import { formatRelativeSync } from "../../lib/usageLabels";
import type { Subscription } from "../../types";
import { UsageCardConfirmOverlay } from "./UsageCardConfirmOverlay";
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
  onOpenInstances?: () => void;
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
  onOpenInstances,
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
  const resyncLabel = t(sub.catalog_id === "antigravity" ? "usage.resyncAntigravity" : "usage.resyncCli");

  return (
    <>
      <footer className={usageCardSlotClassName.footer}>
        <div className="flex w-full items-center gap-2">
          {hasFacts ? (
            <p className="min-w-0 flex-1 truncate text-[11px] text-zinc-600" title={factsTitle}>
              {monthlyCost !== null && (
                <span className="font-semibold tabular-nums text-zinc-800">
                  {formatCurrencyAmount(monthlyCost, sub.currency)}
                  <span className="ml-0.5 font-normal text-zinc-500">{t("usage.perMonth")}</span>
                </span>
              )}
              {monthlyCost !== null && renewLabel ? <span className="mx-1.5 text-zinc-400">·</span> : null}
              {renewLabel ? (
                <span className={cn("tabular-nums", renewUrgent ? "font-semibold text-amber-700" : "text-zinc-700")}>
                  {renewLabel}
                </span>
              ) : null}
            </p>
          ) : (
            <span className="min-w-0 flex-1" />
          )}
          <div className="flex shrink-0 items-center justify-end gap-0.5">
            {onOpenInstances ? (
              <Button
                size="icon-sm"
                variant="ghost"
                title={t("usage.instances")}
                aria-label={t("usage.instances")}
                onClick={onOpenInstances}
                className="text-zinc-500 hover:text-zinc-800"
              >
                <Layers className="h-3.5 w-3.5" aria-hidden />
              </Button>
            ) : null}
            {onSetActive && !sub.is_active && (
              <Button
                size="icon-sm"
                variant="ghost"
                title={t("usage.setActive")}
                aria-label={t("usage.setActive")}
                onClick={() => void handleSetActive()}
                disabled={activating}
                aria-busy={activating}
                className="text-zinc-500 hover:text-emerald-600"
              >
                <BadgeCheck className={cn("h-3.5 w-3.5", activating && "motion-safe:animate-pulse")} aria-hidden />
              </Button>
            )}
            {onSwitchToCli && sub.is_active && sub.supports_cli_switch && (
              <Button
                size="icon-sm"
                variant="ghost"
                title={resyncLabel}
                aria-label={resyncLabel}
                onClick={() => void handleSwitchToCli()}
                disabled={cliSyncing}
                aria-busy={cliSyncing}
                className={
                  sub.switch_result && !sub.switch_result.success
                    ? "text-amber-600 hover:text-amber-700"
                    : "text-zinc-500 hover:text-zinc-800"
                }
              >
                <RefreshCw className={cn("h-3.5 w-3.5", cliSyncing && "motion-safe:animate-spin")} aria-hidden />
              </Button>
            )}
            {subscriptionUrl ? (
              <Button
                size="icon-sm"
                variant="ghost"
                title={t("usage.renewConsole")}
                aria-label={t("usage.renewConsole")}
                onClick={(e) => handleOpenConsole(e)}
                className="text-zinc-500 hover:text-zinc-800"
              >
                <ExternalLink className="h-3.5 w-3.5" aria-hidden />
              </Button>
            ) : null}
            {sub.requires_reauth ? (
              <Button
                size="icon-sm"
                variant="destructive"
                title={t("usage.requiresReauth")}
                aria-label={t("usage.requiresReauth")}
                onClick={() => onReauth?.()}
              >
                <ShieldAlert className="h-3.5 w-3.5" aria-hidden />
              </Button>
            ) : (
              <Button
                size="icon-sm"
                variant="ghost"
                title={refreshTitle}
                aria-label={refreshTitle}
                onClick={() => void handleRefresh()}
                disabled={refreshing || refreshDisabled}
                aria-busy={refreshing}
                className="text-zinc-500 hover:text-zinc-800"
              >
                <RefreshCw className={cn("h-3.5 w-3.5", refreshing && "motion-safe:animate-spin")} aria-hidden />
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
                  "text-amber-700 hover:text-amber-800",
                  resetCountLabel && "h-8 gap-1 px-1.5 text-[10px] font-semibold tabular-nums",
                )}
              >
                <RotateCcw className={cn("h-3.5 w-3.5", resetting && "motion-safe:animate-spin")} aria-hidden />
                {resetCountLabel}
              </Button>
            )}
            <Button
              size="icon-sm"
              variant="ghost"
              title={t("common.edit")}
              aria-label={t("common.edit")}
              onClick={onEdit}
              className="text-zinc-500 hover:text-zinc-800"
            >
              <Pencil className="h-3.5 w-3.5" aria-hidden />
            </Button>
            <Button
              size="icon-sm"
              variant="ghost"
              title={t("common.delete")}
              aria-label={t("common.delete")}
              onClick={() => setDeletePending(true)}
              className="text-red-600 hover:bg-red-50 hover:text-red-700"
            >
              <Trash2 className="h-3.5 w-3.5" aria-hidden />
            </Button>
          </div>
        </div>
      </footer>

      {deletePending && (
        <UsageCardConfirmOverlay
          title={t("usage.confirmDeleteTitle")}
          message={t("usage.confirmDeleteMsg", { name: sub.display_name })}
          confirmLabel={t("common.delete")}
          cancelLabel={t("common.cancel")}
          confirmVariant="destructive"
          onCancel={() => setDeletePending(false)}
          onConfirm={() => {
            setDeletePending(false);
            onDelete();
          }}
        />
      )}

      {resetPending && (
        <UsageCardConfirmOverlay
          title={t("usage.resetQuotaConfirmTitle")}
          message={t("usage.resetQuotaConfirmMsg", { name: sub.display_name })}
          confirmLabel={t("usage.resetQuota")}
          cancelLabel={t("common.cancel")}
          confirmVariant="default"
          confirmDisabled={resetting}
          onCancel={() => setResetPending(false)}
          onConfirm={() => {
            setResetPending(false);
            void handleResetQuota();
          }}
        />
      )}
    </>
  );
}
