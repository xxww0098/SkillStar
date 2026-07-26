import type { TFunction } from "i18next";
import { ExternalLink, Loader2, Play, RefreshCw, WalletCards } from "lucide-react";
import { memo, useCallback } from "react";
import { useTranslation } from "react-i18next";
import { Button } from "../../../../components/ui/button";
import { openExternalUrl } from "../../../../lib/externalOpen";
import { cn } from "../../../../lib/utils";
import { useBalanceQuery } from "../../api/balance";
import { useLatencyTest } from "../../api/diagnostics";
import { providerCardClass } from "../providerForm/ProviderConfigPrimitives";

export interface ConnectionStatusPanelProps {
  providerId: string;
  presetId?: string;
  apiKey: string;
  /** @deprecated Use baseUrlOpenai */
  baseUrl?: string;
  baseUrlOpenai?: string;
  baseUrlAnthropic?: string;
}

const BALANCE_CONSOLE_URLS = {
  deepseek: "https://platform.deepseek.com/usage",
  kimi: "https://platform.moonshot.cn/console/account",
  openrouter: "https://openrouter.ai/settings/credits",
  siliconflow: "https://cloud.siliconflow.cn/account/balance",
} as const;

type BalancePresetId = keyof typeof BALANCE_CONSOLE_URLS;

function isBalancePreset(presetId?: string): presetId is BalancePresetId {
  return Boolean(presetId && presetId in BALANCE_CONSOLE_URLS);
}

function formatBalanceAmount(available: number, currency: string, locale: string) {
  const normalizedCurrency = currency.toUpperCase();
  const value = new Intl.NumberFormat(locale, {
    minimumFractionDigits: 2,
    maximumFractionDigits: 2,
  }).format(available);

  if (normalizedCurrency === "CNY" || normalizedCurrency === "RMB") {
    return `¥${value}`;
  }
  if (normalizedCurrency === "USD") {
    return `$${value}`;
  }
  return `${normalizedCurrency} ${value}`;
}

function getConnectionStatus(result: ReturnType<typeof useLatencyTest>["result"], t: TFunction) {
  if (!result) {
    return {
      label: t("models.diagnosticsPanel.notTested"),
      dotClass: "bg-muted-foreground/40",
      textClass: "text-muted-foreground",
    };
  }
  if (result.status === "ok") {
    return {
      label:
        result.latency_ms == null
          ? t("models.diagnosticsPanel.ok")
          : t("models.diagnosticsPanel.okLatency", { ms: result.latency_ms }),
      dotClass: "bg-success",
      textClass: "text-success",
    };
  }
  if (result.status === "timeout") {
    return { label: t("models.diagnosticsPanel.timeout"), dotClass: "bg-amber-400", textClass: "text-amber-500" };
  }
  if (result.status === "auth_failed") {
    return {
      label: t("models.diagnosticsPanel.authFailed"),
      dotClass: "bg-destructive",
      textClass: "text-destructive",
    };
  }
  return { label: t("models.diagnosticsPanel.failed"), dotClass: "bg-destructive", textClass: "text-destructive" };
}

function ConnectionStatusPanelInner({
  presetId,
  apiKey,
  baseUrl,
  baseUrlOpenai,
  baseUrlAnthropic,
}: ConnectionStatusPanelProps) {
  const { t, i18n } = useTranslation();
  const openaiUrl = (baseUrlOpenai ?? baseUrl ?? "").trim();
  const anthropicUrl = (baseUrlAnthropic ?? "").trim();
  const primaryUrl = openaiUrl || anthropicUrl;

  const { testConnection, isLoading: isTesting, result: latencyResult } = useLatencyTest();

  const balancePresetId = isBalancePreset(presetId) ? presetId : null;
  const {
    balance,
    isLoading: isBalanceLoading,
    error: balanceError,
    refresh: refreshBalance,
  } = useBalanceQuery(balancePresetId, apiKey, primaryUrl);

  const handleTestConnection = useCallback(() => {
    if (!primaryUrl || !apiKey) return;
    const format = openaiUrl ? "openai" : "anthropic";
    testConnection(primaryUrl, apiKey, "", format);
  }, [primaryUrl, apiKey, openaiUrl, testConnection]);

  const handleRefreshBalance = useCallback(() => {
    void refreshBalance();
  }, [refreshBalance]);

  const handleOpenConsole = useCallback(() => {
    if (!balancePresetId) return;
    void openExternalUrl(BALANCE_CONSOLE_URLS[balancePresetId]);
  }, [balancePresetId]);

  const status = getConnectionStatus(latencyResult, t);
  const balanceAmount = balance
    ? formatBalanceAmount(balance.available, balance.currency, i18n.resolvedLanguage ?? i18n.language)
    : "--";

  let balanceHint = t("models.diagnosticsPanel.accountBalance");
  if (!apiKey) {
    balanceHint = t("models.diagnosticsPanel.noApiKey");
  } else if (balanceError) {
    balanceHint = t("models.diagnosticsPanel.balanceFailed");
  }

  return (
    <div className="space-y-3">
      <section className={cn(providerCardClass, "space-y-3 p-4")}>
        <div className="flex items-center justify-between gap-3">
          <h4 className="flex items-center gap-2 text-sm font-semibold text-foreground">
            <span
              className={cn("h-2.5 w-2.5 rounded-full", isTesting ? "animate-pulse bg-primary" : status.dotClass)}
            />
            {t("models.diagnosticsPanel.connectionStatus")}
          </h4>
          <div className="flex items-center gap-1.5 text-xs font-medium">
            {isTesting ? (
              <>
                <Loader2 className="h-3.5 w-3.5 animate-spin text-primary" />
                <span className="text-muted-foreground">{t("models.diagnosticsPanel.testing")}</span>
              </>
            ) : (
              <span className={status.textClass}>{status.label}</span>
            )}
          </div>
        </div>

        <div>
          <Button
            type="button"
            variant="outline"
            size="sm"
            onClick={handleTestConnection}
            disabled={isTesting || !primaryUrl || !apiKey}
            className="h-9 w-full justify-center text-xs"
          >
            {isTesting ? (
              <Loader2 className="h-3.5 w-3.5 animate-spin text-primary" />
            ) : (
              <Play className="h-3.5 w-3.5 text-primary" />
            )}
            {t("models.diagnosticsPanel.deepTest")}
          </Button>
        </div>
      </section>

      {balancePresetId ? (
        <section className={cn(providerCardClass, "p-4")}>
          <div className="flex items-center justify-between gap-3">
            <h4 className="flex items-center gap-2 text-sm font-semibold text-foreground">
              <span className="flex h-7 w-7 items-center justify-center rounded-full border border-primary/15 bg-primary/10 text-primary">
                <WalletCards className="h-3.5 w-3.5" />
              </span>
              {t("models.diagnosticsPanel.balance")}
            </h4>
            {isBalanceLoading ? <Loader2 className="h-4 w-4 animate-spin text-primary" /> : null}
          </div>

          <div className="mt-4">
            <div className="truncate text-[28px] font-semibold leading-none tracking-normal text-foreground">
              {balanceAmount}
            </div>
            <p className={cn("mt-2 text-xs font-medium", balanceError ? "text-destructive" : "text-muted-foreground")}>
              {balanceHint}
            </p>
          </div>

          <div className="mt-4 grid grid-cols-2 gap-2">
            <Button
              type="button"
              variant="outline"
              size="sm"
              onClick={handleRefreshBalance}
              disabled={isBalanceLoading || !apiKey}
              className="h-9 justify-center text-xs"
            >
              {isBalanceLoading ? (
                <Loader2 className="h-3.5 w-3.5 animate-spin text-primary" />
              ) : (
                <RefreshCw className="h-3.5 w-3.5 text-primary" />
              )}
              {t("models.diagnosticsPanel.refresh")}
            </Button>
            <Button
              type="button"
              variant="outline"
              size="sm"
              onClick={handleOpenConsole}
              className="h-9 justify-center text-xs"
            >
              <ExternalLink className="h-3.5 w-3.5 text-primary" />
              {t("models.diagnosticsPanel.console")}
            </Button>
          </div>
        </section>
      ) : null}
    </div>
  );
}

export const ConnectionStatusPanel = memo(ConnectionStatusPanelInner);
