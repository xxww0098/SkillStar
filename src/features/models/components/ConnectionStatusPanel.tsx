import { ExternalLink, Loader2, Play, RefreshCw, WalletCards } from "lucide-react";
import { memo, useCallback } from "react";
import { Button } from "../../../components/ui/button";
import { openExternalUrl } from "../../../lib/externalOpen";
import { cn } from "../../../lib/utils";
import { useBalanceQuery } from "../hooks/useBalanceQuery";
import { useLatencyTest } from "../hooks/useLatencyTest";

export interface ConnectionStatusPanelProps {
  providerId: string;
  presetId?: string;
  apiKey: string;
  baseUrl: string;
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

function formatBalanceAmount(available: number, currency: string) {
  const normalizedCurrency = currency.toUpperCase();
  const value = new Intl.NumberFormat("zh-CN", {
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

function getConnectionStatus(result: ReturnType<typeof useLatencyTest>["result"]) {
  if (!result) {
    return { label: "未测试", dotClass: "bg-success", textClass: "text-muted-foreground" };
  }
  if (result.status === "ok") {
    return {
      label: result.latency_ms == null ? "连接正常" : `连接正常 · ${result.latency_ms}ms`,
      dotClass: "bg-success",
      textClass: "text-success",
    };
  }
  if (result.status === "timeout") {
    return { label: "连接超时", dotClass: "bg-amber-400", textClass: "text-amber-500" };
  }
  if (result.status === "auth_failed") {
    return { label: "鉴权失败", dotClass: "bg-destructive", textClass: "text-destructive" };
  }
  return { label: "连接失败", dotClass: "bg-destructive", textClass: "text-destructive" };
}

function ConnectionStatusPanelInner({ presetId, apiKey, baseUrl }: ConnectionStatusPanelProps) {
  const { testConnection, isLoading: isTesting, result: latencyResult } = useLatencyTest();
  const balancePresetId = isBalancePreset(presetId) ? presetId : null;
  const {
    balance,
    isLoading: isBalanceLoading,
    error: balanceError,
    refresh: refreshBalance,
  } = useBalanceQuery(balancePresetId, apiKey, baseUrl);

  const handleTestConnection = useCallback(() => {
    if (!baseUrl || !apiKey) return;
    testConnection(baseUrl, apiKey, "", "openai");
  }, [baseUrl, apiKey, testConnection]);

  const handleRefreshBalance = useCallback(() => {
    void refreshBalance();
  }, [refreshBalance]);

  const handleOpenConsole = useCallback(() => {
    if (!balancePresetId) return;
    void openExternalUrl(BALANCE_CONSOLE_URLS[balancePresetId]);
  }, [balancePresetId]);

  const status = getConnectionStatus(latencyResult);
  const balanceAmount = balance ? formatBalanceAmount(balance.available, balance.currency) : "--";

  let balanceHint = "账户余额";
  if (!apiKey) {
    balanceHint = "未配置 API Key";
  } else if (balanceError) {
    balanceHint = "余额获取失败";
  }

  return (
    <div className="space-y-3">
      <section className="space-y-3 rounded-2xl border border-border/55 bg-card/55 p-4 shadow-sm backdrop-blur-sm transition duration-200 hover:-translate-y-0.5 hover:border-primary/25 hover:bg-card/75 hover:shadow-[0_16px_36px_-28px_var(--color-shadow)]">
        <h4 className="flex items-center gap-2 text-sm font-semibold text-foreground">
          <span className="h-2.5 w-2.5 rounded-full bg-success shadow-[0_0_0_3px_rgba(var(--color-success-rgb),0.12)]" />
          连接状态
        </h4>

        <div className="flex items-center gap-2 text-xs font-medium">
          {isTesting ? (
            <>
              <Loader2 className="h-3.5 w-3.5 animate-spin text-primary" />
              <span className="text-muted-foreground">测试中...</span>
            </>
          ) : (
            <>
              <span className={cn("h-2 w-2 rounded-full", status.dotClass)} />
              <span className={status.textClass}>{status.label}</span>
            </>
          )}
        </div>

        <Button
          type="button"
          variant="outline"
          size="sm"
          onClick={handleTestConnection}
          disabled={isTesting || !baseUrl || !apiKey}
          className="h-9 w-full justify-center border-border/60 bg-background/50 text-xs font-semibold text-foreground/80 hover:border-primary/40 hover:bg-background/70 hover:text-foreground"
        >
          {isTesting ? (
            <Loader2 className="h-3.5 w-3.5 animate-spin text-primary" />
          ) : (
            <Play className="h-3.5 w-3.5 text-primary" />
          )}
          测试连接
        </Button>
      </section>

      {balancePresetId ? (
        <section className="rounded-2xl border border-border/55 bg-card/55 p-4 shadow-sm backdrop-blur-sm transition duration-200 hover:-translate-y-0.5 hover:border-primary/25 hover:bg-card/75 hover:shadow-[0_16px_36px_-28px_var(--color-shadow)]">
          <div className="flex items-center justify-between gap-3">
            <h4 className="flex items-center gap-2 text-sm font-semibold text-foreground">
              <span className="flex h-7 w-7 items-center justify-center rounded-full border border-primary/15 bg-primary/10 text-primary">
                <WalletCards className="h-3.5 w-3.5" />
              </span>
              余额
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
              className="h-8 justify-center border-border/60 bg-background/50 text-xs font-semibold text-foreground/80 hover:border-primary/40 hover:bg-background/70 hover:text-foreground"
            >
              {isBalanceLoading ? (
                <Loader2 className="h-3.5 w-3.5 animate-spin text-primary" />
              ) : (
                <RefreshCw className="h-3.5 w-3.5 text-primary" />
              )}
              刷新
            </Button>
            <Button
              type="button"
              variant="outline"
              size="sm"
              onClick={handleOpenConsole}
              className="h-8 justify-center border-border/60 bg-background/50 text-xs font-semibold text-foreground/80 hover:border-primary/40 hover:bg-background/70 hover:text-foreground"
            >
              <ExternalLink className="h-3.5 w-3.5 text-primary" />
              控制台
            </Button>
          </div>
        </section>
      ) : null}
    </div>
  );
}

export const ConnectionStatusPanel = memo(ConnectionStatusPanelInner);
