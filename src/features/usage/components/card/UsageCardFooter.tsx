import { BadgeCheck, ExternalLink, Pencil, RefreshCw, ShieldAlert, Trash2 } from "lucide-react";
import { useState } from "react";
import { useTranslation } from "react-i18next";
import { isTauri } from "@tauri-apps/api/core";
import { Button } from "@/components/ui/button";
import { ExternalAnchor } from "@/components/ui/ExternalAnchor";
import { cn } from "@/lib/utils";
import { usageApi } from "../../api";
import { formatCurrencyAmount } from "../../lib/pricing";
import type { Subscription } from "../../types";

export interface UsageCardFooterProps {
  subscription: Subscription;
  monthlyCost: number | null;
  showRenewFooter: boolean;
  renewDays: number | null;
  subscriptionUrl?: string | null;
  onRefresh: () => void | Promise<void>;
  onEdit: () => void;
  onDelete: () => void;
  onReauth?: () => void;
  onSetActive?: () => Promise<void>;
  onSwitchToCli?: () => Promise<void>;
  refreshDisabled?: boolean;
}

/** Cost / renew cells + action icons + delete confirm overlay host (overlay stays in card root). */
export function UsageCardFooter({
  subscription: sub,
  monthlyCost,
  showRenewFooter,
  renewDays,
  subscriptionUrl,
  onRefresh,
  onEdit,
  onDelete,
  onReauth,
  onSetActive,
  onSwitchToCli,
  refreshDisabled = false,
}: UsageCardFooterProps) {
  const { t } = useTranslation();
  const [refreshing, setRefreshing] = useState(false);
  const [activating, setActivating] = useState(false);
  const [cliSyncing, setCliSyncing] = useState(false);
  const [deletePending, setDeletePending] = useState(false);

  const handleRefresh = async () => {
    setRefreshing(true);
    try {
      await onRefresh();
    } finally {
      setRefreshing(false);
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

  return (
    <>
      <footer className="relative z-10 flex flex-col gap-2.5 border-t border-zinc-100 bg-zinc-50/50 px-4 py-3">
        {(monthlyCost !== null || showRenewFooter) && (
          <div
            className={cn(
              "grid gap-2.5 text-[10px]",
              monthlyCost !== null && showRenewFooter ? "grid-cols-2" : "grid-cols-1",
            )}
          >
            {monthlyCost !== null && (
              <div className="min-w-0 rounded-xl border border-zinc-200/40 bg-zinc-100/60 px-2.5 py-2">
                <p className="mb-1 text-[10px] whitespace-nowrap text-zinc-500">{t("usage.subscriptionCost")}</p>
                <p className="text-[11px] font-bold whitespace-nowrap text-zinc-800 tabular-nums">
                  {formatCurrencyAmount(monthlyCost, sub.currency)}
                  <span className="ml-0.5 text-[9px] font-normal text-zinc-400">{t("usage.perMonth")}</span>
                </p>
              </div>
            )}
            {showRenewFooter && (
              <div className="min-w-0 rounded-xl border border-zinc-200/40 bg-zinc-100/60 px-2.5 py-2">
                <p className="mb-1 text-[10px] whitespace-nowrap text-zinc-500">{t("usage.nextRenew")}</p>
                <div className="text-[11px] font-bold whitespace-nowrap">
                  {renewDays !== null ? (
                    renewDays < 0 ? (
                      <span className="text-rose-600">{t("usage.expired", { days: -renewDays })}</span>
                    ) : renewDays === 0 ? (
                      <span className="text-amber-600">{t("usage.expiresToday")}</span>
                    ) : renewDays <= 7 ? (
                      <span className="text-amber-600">{t("usage.renewInDays", { days: renewDays })}</span>
                    ) : (
                      <span className="text-zinc-700">{t("usage.renewInDays", { days: renewDays })}</span>
                    )
                  ) : (
                    <span className="font-normal text-zinc-400">{t("usage.noExpiry")}</span>
                  )}
                </div>
              </div>
            )}
          </div>
        )}
        <div className="flex items-center justify-end gap-0.5">
          {onSetActive && !sub.is_active && (
            <Button
              size="icon-sm"
              variant="ghost"
              title={t("usage.setActive")}
              onClick={() => void handleSetActive()}
              disabled={activating}
              className="text-zinc-500 transition-transform hover:scale-105 hover:text-emerald-600"
            >
              <BadgeCheck className={cn("h-3.5 w-3.5", activating && "animate-pulse")} />
            </Button>
          )}
          {onSwitchToCli && sub.is_active && sub.supports_cli_switch && (
            <Button
              size="icon-sm"
              variant="ghost"
              title={t("usage.resyncCli")}
              onClick={() => void handleSwitchToCli()}
              disabled={cliSyncing}
              className={cn(
                "transition-transform hover:scale-105",
                sub.switch_result && !sub.switch_result.success
                  ? "text-amber-500 hover:text-amber-600"
                  : "text-zinc-500 hover:text-zinc-800",
              )}
            >
              <RefreshCw className={cn("h-3.5 w-3.5", cliSyncing && "animate-spin")} />
            </Button>
          )}
          {isTauri() && (
            <Button
              size="icon-sm"
              variant="ghost"
              title={t("usage.openInWindow")}
              onClick={() => void usageApi.openUsageCardWindow(sub.id)}
              className="text-zinc-500 transition-transform hover:scale-105 hover:text-zinc-800"
            >
              <ExternalLink className="h-3.5 w-3.5" />
            </Button>
          )}
          {sub.requires_reauth ? (
            <Button
              size="icon-sm"
              variant="destructive"
              title={t("usage.requiresReauth")}
              onClick={() => onReauth?.()}
              className="transition-transform hover:scale-105"
            >
              <ShieldAlert className="h-3.5 w-3.5" />
            </Button>
          ) : (
            <Button
              size="icon-sm"
              variant="ghost"
              title={t("usage.syncUsage")}
              onClick={() => void handleRefresh()}
              disabled={refreshing || refreshDisabled}
              className="text-zinc-500 transition-transform hover:scale-105 hover:text-zinc-800"
            >
              <RefreshCw className={cn("h-3.5 w-3.5", refreshing && "animate-spin")} />
            </Button>
          )}
          <Button
            size="icon-sm"
            variant="ghost"
            title={t("common.edit")}
            onClick={onEdit}
            className="text-zinc-500 transition-transform hover:scale-105 hover:text-zinc-800"
          >
            <Pencil className="h-3.5 w-3.5" />
          </Button>
          <Button
            size="icon-sm"
            variant="ghost"
            title={t("common.delete")}
            onClick={() => setDeletePending(true)}
            className="text-red-500 transition-transform hover:scale-105 hover:bg-red-50 hover:text-red-600"
          >
            <Trash2 className="h-3.5 w-3.5" />
          </Button>
          {subscriptionUrl && (
            <ExternalAnchor
              href={subscriptionUrl}
              className="inline-flex h-8 w-8 items-center justify-center rounded-md text-muted-foreground transition-all hover:scale-105 hover:bg-white/5 hover:text-foreground"
              title={t("usage.renewConsole")}
            >
              <ExternalLink className="h-3.5 w-3.5" />
            </ExternalAnchor>
          )}
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
    </>
  );
}
