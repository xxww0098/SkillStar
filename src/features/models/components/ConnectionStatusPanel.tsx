import { ExternalLink, Loader2, Play, RefreshCw, WalletCards } from "lucide-react";
import { memo, useCallback, useMemo } from "react";
import { Button } from "../../../components/ui/button";
import { openExternalUrl } from "../../../lib/externalOpen";
import { cn } from "../../../lib/utils";
import { useBalanceQuery } from "../hooks/useBalanceQuery";
import { useEndpointSpeedTest } from "../hooks/useEndpointSpeedTest";
import { useLatencyTest } from "../hooks/useLatencyTest";
import { endpointProbeLabel, endpointProbeTone } from "../lib/endpointProbe";

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
    return { label: "未测试", dotClass: "bg-muted-foreground/40", textClass: "text-muted-foreground" };
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

function ConnectionStatusPanelInner({
  presetId,
  apiKey,
  baseUrl,
  baseUrlOpenai,
  baseUrlAnthropic,
}: ConnectionStatusPanelProps) {
  const openaiUrl = (baseUrlOpenai ?? baseUrl ?? "").trim();
  const anthropicUrl = (baseUrlAnthropic ?? "").trim();
  const primaryUrl = openaiUrl || anthropicUrl;

  const { testConnection, isLoading: isTesting, result: latencyResult } = useLatencyTest();
  const { testEndpoints, results: endpointResults, isLoading: isEndpointTesting } = useEndpointSpeedTest();

  const balancePresetId = isBalancePreset(presetId) ? presetId : null;
  const {
    balance,
    isLoading: isBalanceLoading,
    error: balanceError,
    refresh: refreshBalance,
  } = useBalanceQuery(balancePresetId, apiKey, primaryUrl);

  const probeUrls = useMemo(() => [openaiUrl, anthropicUrl].filter(Boolean), [openaiUrl, anthropicUrl]);

  const handleTestConnection = useCallback(() => {
    if (!primaryUrl || !apiKey) return;
    const format = openaiUrl ? "openai" : "anthropic";
    testConnection(primaryUrl, apiKey, "", format);
  }, [primaryUrl, apiKey, openaiUrl, testConnection]);

  const handleProbeEndpoints = useCallback(() => {
    void testEndpoints(probeUrls, apiKey);
  }, [testEndpoints, probeUrls, apiKey]);

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

  const isBusy = isTesting || isEndpointTesting;

  return (
    <div className="space-y-3">
      <section className="space-y-3 rounded-xl border border-border/55 bg-card/55 p-4 shadow-sm backdrop-blur-sm transition duration-200 hover:-translate-y-0.5 hover:border-primary/25 hover:bg-card/75 hover:shadow-[0_16px_36px_-28px_var(--color-shadow)]">
        <h4 className="flex items-center gap-2 text-sm font-semibold text-foreground">
          <span className="h-2.5 w-2.5 rounded-full bg-success shadow-[0_0_0_3px_rgba(var(--color-success-rgb),0.12)]" />
          连接状态
        </h4>

        <div className="flex items-center gap-2 text-xs font-medium">
          {isBusy ? (
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

        <div className="grid gap-2">
          <Button
            type="button"
            variant="outline"
            size="sm"
            onClick={handleTestConnection}
            disabled={isBusy || !primaryUrl || !apiKey}
            className="h-9 w-full justify-center border-border/60 bg-background/50 text-xs font-semibold text-foreground/80 hover:border-primary/40 hover:bg-background/70 hover:text-foreground"
          >
            {isTesting ? (
              <Loader2 className="h-3.5 w-3.5 animate-spin text-primary" />
            ) : (
              <Play className="h-3.5 w-3.5 text-primary" />
            )}
            深度连接测试
          </Button>
          {probeUrls.length > 0 && (
            <Button
              type="button"
              variant="outline"
              size="sm"
              onClick={handleProbeEndpoints}
              disabled={isBusy || !apiKey}
              className="h-9 w-full justify-center border-border/60 bg-background/50 text-xs font-semibold text-foreground/80 hover:border-primary/40 hover:bg-background/70 hover:text-foreground"
            >
              {isEndpointTesting ? (
                <Loader2 className="h-3.5 w-3.5 animate-spin text-primary" />
              ) : (
                <Play className="h-3.5 w-3.5 text-primary" />
              )}
              端点测速 ({probeUrls.length})
            </Button>
          )}
        </div>

        {endpointResults.length > 0 && (
          <ul className="space-y-1 border-t border-border/40 pt-2">
            {endpointResults.map((r) => (
              <li key={r.url} className="flex justify-between gap-2 text-[10px]">
                <span className="min-w-0 truncate font-mono text-muted-foreground">{r.url}</span>
                <span
                  className={cn(
                    "shrink-0 font-medium",
                    endpointProbeTone(r) === "ok" && "text-success",
                    endpointProbeTone(r) === "auth" && "text-amber-500",
                    endpointProbeTone(r) === "error" && "text-destructive",
                  )}
                >
                  {endpointProbeLabel(r)}
                </span>
              </li>
            ))}
          </ul>
        )}
      </section>

      {balancePresetId ? (
        <section className="rounded-xl border border-border/55 bg-card/55 p-4 shadow-sm backdrop-blur-sm transition duration-200 hover:-translate-y-0.5 hover:border-primary/25 hover:bg-card/75 hover:shadow-[0_16px_36px_-28px_var(--color-shadow)]">
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
