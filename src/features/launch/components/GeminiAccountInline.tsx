/**
 * Compact inline Gemini account manager for the Launch pane.
 * Reuses the same data hooks as GeminiAccountSection but renders
 * a minimal card list suited for embedding inside PaneCell.
 */
import { invoke } from "@tauri-apps/api/core";
import { Check, Key, Loader2, LogIn, RefreshCw, Zap } from "lucide-react";
import { useCallback, useEffect, useState } from "react";

import { cn } from "../../../lib/utils";
import { CodexQuotaBar } from "../../models/components/CodexQuotaBar";
import { getPlanColor, getPlanLabel } from "../../models/components/shared/AccountRow";
import { AgentIcon } from "../../models/components/shared/ProviderIcon";
import { useGeminiOAuth } from "../../models/hooks/useGeminiOAuth";
import { useModelProviders } from "../../models/hooks/useModelProviders";

export function GeminiAccountInline() {
  const { providers, addProvider, updateProvider, switchTo, currentId } = useModelProviders("gemini");

  const accounts = Object.values(providers).filter(
    (p) => p.id.startsWith("gemini_oauth_") || p.id.startsWith("gemini_apikey_"),
  );

  // Quota states
  const [quotas, setQuotas] = useState<
    Record<
      string,
      {
        percentage: number;
        resetTime: string;
        planName?: string;
        models?: { name: string; displayName?: string; percentage: number; resetTime: string }[];
        availableCredits?: string;
        isForbidden?: boolean;
        errorMessage?: string;
      }
    >
  >({});
  const [refreshing, setRefreshing] = useState<Set<string>>(new Set());

  const refreshQuota = useCallback(async (providerId: string) => {
    setRefreshing((prev) => new Set(prev).add(providerId));
    try {
      const quota = await invoke<any>("refresh_gemini_quota", {
        appId: "gemini",
        providerId,
      });
      setQuotas((prev) => ({ ...prev, [providerId]: quota }));
    } catch {
      // silent
    } finally {
      setRefreshing((prev) => {
        const next = new Set(prev);
        next.delete(providerId);
        return next;
      });
    }
  }, []);

  // Auto-refresh quota for current account on mount
  useEffect(() => {
    if (currentId && accounts.some((a) => a.id === currentId)) {
      refreshQuota(currentId);
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [currentId]);

  const geminiState = useGeminiOAuth({
    onAccountAdded: (provider) => {
      const existing = Object.values(providers).find(
        (p) => p.id.startsWith("gemini_oauth_") && p.name === provider.name,
      );
      if (existing) {
        const merged = { ...provider, id: existing.id };
        updateProvider(merged);
        switchTo(merged.id);
      } else {
        addProvider(provider);
        switchTo(provider.id);
      }
    },
  });

  if (accounts.length === 0) {
    return (
      <div className="w-full mt-1.5 relative z-30">
        <div className="flex flex-col items-center gap-2 py-2.5 px-2 rounded-lg border border-dashed border-border/50 bg-card/20">
          <span className="text-[10px] text-muted-foreground/60">未配置 Gemini 账号</span>
          <button
            type="button"
            onClick={(e) => {
              e.stopPropagation();
              geminiState.startOAuth();
            }}
            disabled={geminiState.oauthLoading}
            className={cn(
              "flex items-center gap-1.5 px-3 py-1.5 rounded-md text-[10px] font-medium transition-all cursor-pointer",
              geminiState.oauthLoading
                ? "bg-[#4285F4]/20 text-[#4285F4] animate-pulse"
                : "bg-[#4285F4] hover:bg-[#4285F4]/90 text-white",
            )}
          >
            {geminiState.oauthLoading ? <Loader2 className="w-3 h-3 animate-spin" /> : <LogIn className="w-3 h-3" />}
            {geminiState.oauthLoading ? "等待授权..." : "OAuth 登录"}
          </button>
        </div>
      </div>
    );
  }

  return (
    <div className="w-full mt-1.5 space-y-1.5 max-h-[180px] overflow-y-auto scrollbar-thin px-0.5 relative z-30">
      {accounts.map((account) => {
        const isCurrent = account.id === currentId;
        const isOAuth = account.id.includes("oauth");
        const quota = quotas[account.id] || (account.meta as any)?.gemini_quota;
        const planLabel = getPlanLabel(quota?.planName);
        const planColor = getPlanColor(quota?.planName);
        const friendlyName = account.name.replace(/^Google \((.+)\)$/, "$1");
        const isRefreshing = refreshing.has(account.id);

        // Compact quota summary
        let quotaSummary = null;
        if (isOAuth && quota && !quota.isForbidden && !quota.errorMessage) {
          quotaSummary = (
            <div className="mt-1 w-full">
              <CodexQuotaBar label="当前" percentage={quota.percentage} compact />
            </div>
          );
        }

        return (
          <div
            key={account.id}
            className={cn(
              "relative rounded-xl border px-2.5 py-2 transition-all duration-200 cursor-pointer group/row",
              isCurrent
                ? "border-[#4285F4]/40 bg-[#4285F4]/5 shadow-sm"
                : "border-border/50 bg-card/40 hover:bg-card/60 hover:border-border/70",
            )}
            onClick={(e) => {
              e.stopPropagation();
              if (!isCurrent) {
                switchTo(account.id);
                refreshQuota(account.id);
              }
            }}
            onKeyDown={(e) => {
              if (e.key === "Enter" || e.key === " ") {
                e.preventDefault();
                e.stopPropagation();
                if (!isCurrent) {
                  switchTo(account.id);
                  refreshQuota(account.id);
                }
              }
            }}
            role="button"
            tabIndex={0}
          >
            <div className="flex items-center gap-2">
              {/* Icon */}
              <div
                className={cn(
                  "w-6 h-6 rounded-lg flex items-center justify-center shrink-0",
                  isCurrent ? "bg-[#4285F4]/15" : "bg-muted/30",
                )}
              >
                {isOAuth ? (
                  <AgentIcon appId="gemini" color="#4285F4" className="w-3 h-3" />
                ) : (
                  <Key className="w-2.5 h-2.5 text-amber-400" />
                )}
              </div>

              {/* Name + badges */}
              <div className="flex-1 min-w-0 flex items-center gap-1">
                <span className="text-[10px] font-medium text-foreground truncate max-w-[120px]" title={friendlyName}>
                  {isOAuth ? friendlyName : "API Key"}
                </span>
                {isCurrent && (
                  <span
                    className="inline-flex items-center gap-0.5 px-1 py-px rounded text-[8px] font-bold text-white shrink-0"
                    style={{ backgroundColor: "#4285F4" }}
                  >
                    <Check className="w-2 h-2" />
                    当前
                  </span>
                )}
                {quota?.planName && (
                  <span
                    className="px-1 py-px rounded text-[8px] font-semibold text-white shrink-0"
                    style={{ backgroundColor: planColor }}
                  >
                    {planLabel}
                  </span>
                )}
              </div>

              {/* Actions */}
              {!isCurrent && (
                <button
                  type="button"
                  onClick={(e) => {
                    e.stopPropagation();
                    switchTo(account.id);
                    refreshQuota(account.id);
                  }}
                  className="flex items-center gap-1 px-1.5 py-0.5 rounded text-[9px] font-medium border border-[#4285F4]/30 text-[#4285F4] hover:bg-[#4285F4]/10 transition-all cursor-pointer shrink-0"
                >
                  <Zap className="w-2.5 h-2.5" />
                  使用
                </button>
              )}
              {isOAuth && (
                <button
                  type="button"
                  onClick={(e) => {
                    e.stopPropagation();
                    refreshQuota(account.id);
                  }}
                  disabled={isRefreshing}
                  className="p-0.5 rounded text-muted-foreground/50 hover:text-foreground transition-colors disabled:opacity-40 cursor-pointer shrink-0"
                  title="刷新配额"
                >
                  <RefreshCw className={cn("w-2.5 h-2.5", isRefreshing && "animate-spin")} />
                </button>
              )}
            </div>

            {/* Quota bar */}
            {quotaSummary}
          </div>
        );
      })}

      {/* Add account button */}
      <div className="flex justify-center pt-0.5">
        {geminiState.oauthLoading ? (
          <div className="flex items-center gap-1 px-2 py-1 rounded text-[9px] font-medium text-[#4285F4] animate-pulse">
            <Loader2 className="w-2.5 h-2.5 animate-spin" />
            等待授权...
          </div>
        ) : (
          <button
            type="button"
            onClick={(e) => {
              e.stopPropagation();
              geminiState.startOAuth();
            }}
            className="flex items-center gap-1 px-2.5 py-1 rounded-md text-[9px] font-medium text-muted-foreground hover:text-[#4285F4] hover:bg-[#4285F4]/8 border border-transparent hover:border-[#4285F4]/20 transition-all cursor-pointer"
          >
            <LogIn className="w-2.5 h-2.5" />
            添加账号
          </button>
        )}
      </div>
    </div>
  );
}
