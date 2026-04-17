import { AnimatePresence, motion } from "framer-motion";
import { Check, ChevronDown, Key, RefreshCw, Trash2, ShieldPlus, X, Zap } from "lucide-react";
import { useCallback, useEffect, useRef, useState } from "react";

import { cn } from "../../../../lib/utils";
import type { ProviderEntry } from "../../hooks/useModelProviders";
import { CodexQuotaBar } from "../CodexQuotaBar";
import { getPlanColor, getPlanLabel } from "./AccountRow";
import { AgentIcon } from "./ProviderIcon";

interface GeminiAccountRowProps {
  account: ProviderEntry;
  isCurrent: boolean;
  expanded: boolean;
  quotaRefreshing: boolean;
  quota?: {
    percentage: number;
    resetTime: string;
    planName?: string;
    models?: { name: string; displayName?: string; percentage: number; resetTime: string }[];
    availableCredits?: string;
    isForbidden?: boolean;
    errorMessage?: string;
  };
  onSwitch: () => void;
  onToggle: () => void;
  onDelete: () => void;
  onRefreshQuota: () => void;
}

export function GeminiAccountRow({
  account,
  isCurrent,
  expanded,
  quotaRefreshing,
  quota,
  onSwitch,
  onToggle,
  onDelete,
  onRefreshQuota,
}: GeminiAccountRowProps) {
  const [confirmDelete, setConfirmDelete] = useState(false);
  const timerRef = useRef<ReturnType<typeof setTimeout>>(undefined);
  
  const isOAuth = account.id.includes("oauth");
  const planColor = getPlanColor(quota?.planName);
  const planLabel = getPlanLabel(quota?.planName);
  const friendlyName = account.name.replace(/^Google \((.+)\)$/, "$1");

  // Clear confirm state when row collapses
  useEffect(() => {
    if (!expanded) setConfirmDelete(false);
  }, [expanded]);

  // Cleanup timer on unmount
  useEffect(() => {
    return () => {
      if (timerRef.current) clearTimeout(timerRef.current);
    };
  }, []);

  const handleDeleteClick = useCallback((e: React.MouseEvent) => {
    e.stopPropagation();
    if (!confirmDelete) {
      setConfirmDelete(true);
      timerRef.current = setTimeout(() => setConfirmDelete(false), 3500);
      return;
    }
    onDelete();
  }, [confirmDelete, onDelete]);

  // Extract compact quota for collapse view
  let compactSummary = null;
  if (isOAuth && quota) {
    if (quota.models && quota.models.length > 0) {
      // Just show top 2 items compactly
      compactSummary = (
        <div className="mt-1.5 space-y-0.5 max-w-[280px]">
          {quota.models.slice(0, 2).map((m, i) => {
            let labelName = m.displayName || m.name;
            if (labelName.toLowerCase().includes("gemini")) {
              labelName = labelName.replace(/gemini-?/i, "Gem. ").replace(/-(pro|flash|ultra)(-[0-9.]+)?/i, " $1");
            } else if (labelName.toLowerCase().includes("claude")) {
              labelName = labelName.replace(/claude-?-?/i, "Cld. ").replace(/-(sonnet|opus|haiku)(-[0-9.]+)?/i, " $1");
            }
            labelName = labelName.replace(/\b\w/g, c => c.toUpperCase());
            if (labelName.length > 15) labelName = labelName.substring(0, 15) + ".";
            return <CodexQuotaBar key={i} label={labelName} percentage={m.percentage} compact />;
          })}
        </div>
      );
    } else {
      compactSummary = (
        <div className="mt-1.5 max-w-[280px]">
          <CodexQuotaBar label="当前配额" percentage={quota.percentage} compact />
        </div>
      );
    }
  }

  return (
    <div
      className={cn(
        "group relative w-full overflow-hidden transition-all duration-300 rounded-2xl border",
        isCurrent ? "border-2 shadow-lg bg-card/90" : "border-border/70 bg-card/60 hover:bg-card/80 hover:shadow-md",
      )}
      style={isCurrent ? { borderColor: "#4285F460" } : undefined}
    >
      {/* Active gradient glow */}
      {isCurrent && (
        <div
          className="absolute inset-0 rounded-2xl pointer-events-none opacity-[0.06]"
          style={{
            background: "linear-gradient(135deg, #4285F4 0%, transparent 50%)",
          }}
        />
      )}

      {/* Header */}
      <div
        role="button"
        tabIndex={0}
        onClick={onToggle}
        onKeyDown={(e) => {
          if (e.key === "Enter" || e.key === " ") {
            e.preventDefault();
            onToggle();
          }
        }}
        className={cn(
          "relative w-full flex items-center gap-3 px-4 py-3.5 cursor-pointer select-none",
          expanded && "border-b border-border/50",
        )}
      >
        {/* Auth mode icon */}
        <div
          className={cn(
            "w-9 h-9 rounded-xl flex items-center justify-center shrink-0 border transition-all duration-200",
            isCurrent ? "border-transparent shadow-sm" : "border-border/50",
          )}
          style={{
            backgroundColor: isOAuth ? (isCurrent ? "#4285F420" : "#4285F410") : isCurrent ? "#F59E0B20" : "#F59E0B10",
          }}
        >
          {isOAuth ? <AgentIcon appId="gemini" color="#4285F4" className="w-4 h-4" /> : <Key className="w-3.5 h-3.5 text-amber-400" />}
        </div>

        {/* Info */}
        <div className="flex-1 min-w-0">
          <div className="flex items-center gap-1.5">
            <span className="text-sm font-semibold text-foreground truncate max-w-[200px]" title={isOAuth ? friendlyName : "API Key"}>
              {isOAuth ? friendlyName : `API Key: ${((account.settingsConfig?.env as any)?.GEMINI_API_KEY || "**********").slice(0, 12)}...`}
            </span>
            {isCurrent && (
              <span
                className="inline-flex items-center gap-0.5 px-1.5 py-0.5 rounded-md text-[10px] font-semibold text-white shrink-0"
                style={{ backgroundColor: "#4285F4" }}
              >
                <Check className="w-2.5 h-2.5" />
                当前
              </span>
            )}
            {(account.settingsConfig?.env as any)?.GEMINI_MODEL && (
              <span
                className="px-1.5 py-0.5 rounded-md text-[10px] font-medium border shrink-0 truncate max-w-[150px]"
                style={{
                  color: isCurrent ? "#4285F4" : "var(--muted-foreground)",
                  borderColor: isCurrent ? "#4285F430" : "var(--border)",
                  backgroundColor: isCurrent ? "#4285F408" : "var(--muted)",
                }}
                title="当前配置使用的主模型"
              >
                {(account.settingsConfig.env as any).GEMINI_MODEL as string}
              </span>
            )}
            {(quota?.planName || isOAuth) && (
              <span
                className="px-1.5 py-0.5 rounded-md text-[10px] font-medium text-white leading-relaxed shrink-0"
                style={{ backgroundColor: planColor }}
              >
                {planLabel}
              </span>
            )}
            {/* Auth mode label */}
            <span
              className={cn(
                "text-[10px] font-medium px-1.5 py-0.5 rounded-md shrink-0",
                isOAuth ? "text-[#4285F4] bg-[#4285F4]/10" : "text-amber-400/80 bg-amber-500/8",
              )}
            >
              {isOAuth ? "OAuth" : "API Key"}
            </span>
          </div>

          {/* Compact quota (collapsed state) */}
          {!expanded && compactSummary}
        </div>

        {/* Right actions */}
        <div className="flex items-center gap-1 shrink-0" onClick={(e) => e.stopPropagation()}>
          {!isCurrent && (
            <button
              type="button"
              onClick={(e) => {
                e.stopPropagation();
                onSwitch();
                onRefreshQuota();
              }}
              className="flex items-center gap-1.5 px-3 py-1.5 rounded-lg text-xs font-medium border transition-all hover:shadow-sm"
              style={{
                borderColor: "#4285F440",
                color: "#4285F4",
              }}
              onMouseOver={(e) => {
                e.currentTarget.style.backgroundColor = "#4285F410";
              }}
              onFocus={(e) => {
                e.currentTarget.style.backgroundColor = "#4285F410";
              }}
              onMouseOut={(e) => {
                e.currentTarget.style.backgroundColor = "transparent";
              }}
              onBlur={(e) => {
                e.currentTarget.style.backgroundColor = "transparent";
              }}
            >
              <Zap className="w-3.5 h-3.5" />
              使用
            </button>
          )}
          <ChevronDown
            className={cn(
              "w-4 h-4 text-muted-foreground/50 transition-transform duration-200",
              expanded && "rotate-180",
            )}
          />
        </div>
      </div>

      {/* Expanded detail */}
      <AnimatePresence>
        {expanded && (
          <motion.div
            initial={{ height: 0, opacity: 0 }}
            animate={{ height: "auto", opacity: 1 }}
            exit={{ height: 0, opacity: 0 }}
            transition={{ duration: 0.2 }}
            className="overflow-hidden"
          >
            <div className="px-3.5 pb-3.5 pt-1 border-t border-border/30 space-y-3">
              
              {/* Detailed view */}
              {isOAuth && (
                <div className="space-y-4">
                  {quotaRefreshing && !quota ? (
                    <div className="animate-pulse h-12 bg-muted/40 rounded-lg shrink-0 mt-2" />
                  ) : quota ? (
                    <div className="space-y-2 p-3 rounded-lg bg-muted/20 shrink-0 mt-2">
                       {quota.isForbidden ? (
                        <div className="flex flex-col gap-1.5 p-3 border border-amber-500/20 bg-amber-500/5 rounded-xl">
                          <div className="flex items-center gap-1.5 text-amber-500/90 text-xs font-semibold">
                            <ShieldPlus className="w-3.5 h-3.5" />
                            <span>未开启模型配额 API 权限</span>
                          </div>
                          <div className="text-[11px] text-muted-foreground/80 leading-relaxed pl-5">
                            账号已通过验证且积分正常，但 Cloud Code 拒绝返回具体模型使用量（403 Forbidden）。
                          </div>
                        </div>
                      ) : quota.errorMessage ? (
                        <div className="flex flex-col gap-1.5 p-3 border border-destructive/20 bg-destructive/5 rounded-xl">
                          <div className="flex items-center gap-1.5 text-destructive/90 text-xs font-semibold">
                            <X className="w-3.5 h-3.5" />
                            <span>获取模型使用配额失败</span>
                          </div>
                          <div className="text-[11px] text-muted-foreground/80 leading-relaxed pl-5">
                            {quota.errorMessage}
                          </div>
                        </div>
                      ) : quota.models && quota.models.length > 0 ? (
                        <div className="grid grid-cols-2 gap-x-6 gap-y-4">
                          {quota.models.map((model, idx) => {
                            let detailedName = model.displayName || model.name;
                            if (detailedName.toLowerCase().includes("gemini")) {
                              detailedName = detailedName.replace(/gemini-?/i, "Gemini ").replace(/-(pro|flash|ultra)(-[0-9.]+)?/i, " $1");
                            } else if (detailedName.toLowerCase().includes("claude")) {
                              detailedName = detailedName.replace(/claude-?-?/i, "Claude ").replace(/-(sonnet|opus|haiku)(-[0-9.]+)?/i, " $1");
                            }
                            detailedName = detailedName.replace(/\b\w/g, c => c.toUpperCase());
                            if (detailedName.length > 20) detailedName = detailedName.substring(0, 20) + "...";
                            
                            return (
                              <CodexQuotaBar 
                                key={idx}
                                label={detailedName} 
                                percentage={model.percentage} 
                                resetTime={model.resetTime as any} 
                              />
                            );
                          })}
                        </div>
                      ) : (
                        <CodexQuotaBar 
                          label="当前配额" 
                          percentage={quota.percentage} 
                          resetTime={quota.resetTime as any} 
                        />
                      )}
                      {quota.availableCredits && (
                        <div className="text-[11px] font-semibold text-foreground/80 pt-1">
                          可用 AI 积分: {quota.availableCredits}
                        </div>
                      )}
                    </div>
                  ) : null}
                </div>
              )}

              {/* No quota yet */}
              {isOAuth && !quota && !quotaRefreshing && (
                <p className="text-[10px] text-muted-foreground/60 px-1 pt-2">尚未获取配额数据，点击下方刷新按钮获取</p>
              )}

              {/* Actions */}
              <div className="flex items-center gap-2 pt-1">
                {isOAuth && (
                  <button
                    type="button"
                    onClick={(e) => { e.stopPropagation(); onRefreshQuota(); }}
                    disabled={quotaRefreshing}
                    className="flex items-center gap-1.5 px-2.5 py-1.5 rounded-lg text-[10px] font-medium bg-muted/30 hover:bg-muted/50 text-muted-foreground transition-colors disabled:opacity-50"
                  >
                    <RefreshCw className={cn("w-3 h-3", quotaRefreshing && "animate-spin")} />
                    刷新配额
                  </button>
                )}

                <div className="flex-1" />

                <button
                  type="button"
                  onClick={handleDeleteClick}
                  className={cn(
                    "flex items-center gap-1 px-2.5 py-1.5 rounded-lg text-[10px] font-medium transition-colors",
                    confirmDelete ? "bg-destructive text-white" : "text-destructive hover:bg-destructive/10",
                  )}
                >
                  <Trash2 className="w-3 h-3" />
                  {confirmDelete ? "确认删除" : "删除"}
                </button>
              </div>
            </div>
          </motion.div>
        )}
      </AnimatePresence>
    </div>
  );
}
