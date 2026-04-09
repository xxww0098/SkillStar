/**
 * Shared AccountRow component — unified account card used by both
 * CodexAccountSection (ModelsPanel) and CodexConfigPanel.
 *
 * Displays account info, plan badge, quota bars, and contextual actions.
 */
import { AnimatePresence, motion } from "framer-motion";
import { AlertCircle, Check, ChevronDown, Key, RefreshCw, Trash2, User, Zap } from "lucide-react";
import { useCallback, useEffect, useRef, useState } from "react";

import { cn } from "../../../../lib/utils";
import type { CodexAccount } from "../../hooks/useCodexAccounts";
import { CodexQuotaBar } from "../CodexQuotaBar";
import { AgentIcon } from "./ProviderIcon";

// ── Plan badge helpers ──────────────────────────────────────────────

export function getPlanColor(planType?: string): string {
  if (!planType) return "#6B7280";
  const p = planType.toLowerCase();
  if (p.includes("team") || p.includes("ultra") || p.includes("advanced") || p.includes("premium")) return "#7C3AED";
  if (p.includes("pro") || p.includes("plus")) return "#F59E0B";
  if (p.includes("enterprise")) return "#3B82F6";
  return "#6B7280";
}

export function getPlanLabel(planType?: string): string {
  if (!planType) return "FREE";
  const p = planType.toLowerCase();
  if (p.includes("ultra")) return "ULTRA";
  if (p.includes("advanced")) return "ADVANCED";
  if (p.includes("premium")) return "PREMIUM";
  if (p.includes("team")) return "TEAM";
  if (p.includes("pro")) return "PRO";
  if (p.includes("plus")) return "PLUS";
  if (p.includes("enterprise")) return "ENT";
  if (p === "api_key") return "KEY";
  return planType.toUpperCase().slice(0, 8);
}

// ── AccountRow ──────────────────────────────────────────────────────

interface AccountRowProps {
  account: CodexAccount;
  isCurrent: boolean;
  expanded: boolean;
  quotaRefreshing: boolean;
  onSwitch: () => void;
  onToggle: () => void;
  onDelete: () => void;
  onRefreshQuota: () => void;
}

export function AccountRow({
  account,
  isCurrent,
  expanded,
  quotaRefreshing,
  onSwitch,
  onToggle,
  onDelete,
  onRefreshQuota,
}: AccountRowProps) {
  const [confirmDelete, setConfirmDelete] = useState(false);
  const timerRef = useRef<ReturnType<typeof setTimeout>>(undefined);
  const planColor = getPlanColor(account.planType);
  const planLabel = getPlanLabel(account.planType);
  const isOAuth = account.authMode === "oauth";

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

  const handleDeleteClick = useCallback(() => {
    if (!confirmDelete) {
      setConfirmDelete(true);
      timerRef.current = setTimeout(() => setConfirmDelete(false), 3500);
      return;
    }
    onDelete();
  }, [confirmDelete, onDelete]);

  return (
    <div
      className={cn(
        "group relative w-full overflow-hidden transition-all duration-300 rounded-2xl border",
        isCurrent ? "border-2 shadow-lg bg-card/90" : "border-border/70 bg-card/60 hover:bg-card/80 hover:shadow-md",
      )}
      style={isCurrent ? { borderColor: "#00A67E60" } : undefined}
    >
      {/* Active gradient glow */}
      {isCurrent && (
        <div
          className="absolute inset-0 rounded-2xl pointer-events-none opacity-[0.06]"
          style={{
            background: "linear-gradient(135deg, #00A67E 0%, transparent 50%)",
          }}
        />
      )}

      {/* Header */}
      {/* biome-ignore lint/a11y/useSemanticElements: Complex interactive row */}
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
            backgroundColor: isOAuth ? (isCurrent ? "#00A67E20" : "#00A67E10") : isCurrent ? "#F59E0B20" : "#F59E0B10",
          }}
        >
          {isOAuth ? <AgentIcon appId="codex" color="#34d399" className="w-4 h-4" /> : <Key className="w-3.5 h-3.5 text-amber-400" />}
        </div>

        {/* Info */}
        <div className="flex-1 min-w-0">
          <div className="flex items-center gap-1.5">
            <span className="text-sm font-semibold text-foreground truncate">
              {isOAuth ? account.email : `API Key: ${(account.openaiApiKey || "").slice(0, 12)}...`}
            </span>
            {isCurrent && (
              <span
                className="inline-flex items-center gap-0.5 px-1.5 py-0.5 rounded-md text-[10px] font-semibold text-white"
                style={{ backgroundColor: "#00A67E" }}
              >
                <Check className="w-2.5 h-2.5" />
                当前
              </span>
            )}
            <span
              className="px-1.5 py-0.5 rounded-md text-[10px] font-medium text-white leading-relaxed"
              style={{ backgroundColor: planColor }}
            >
              {planLabel}
            </span>
            {/* Auth mode label */}
            <span
              className={cn(
                "text-[10px] font-medium px-1.5 py-0.5 rounded-md",
                isOAuth ? "text-emerald-400/80 bg-emerald-500/8" : "text-amber-400/80 bg-amber-500/8",
              )}
            >
              {isOAuth ? "OAuth" : "API Key"}
            </span>
          </div>

          {/* Compact quota (collapsed state) */}
          {isOAuth && account.quota && !expanded && (
            <div className="mt-1.5 space-y-0.5 max-w-[280px]">
              <CodexQuotaBar label="5h" percentage={account.quota.hourlyPercentage} compact />
              <CodexQuotaBar label="7d" percentage={account.quota.weeklyPercentage} compact />
            </div>
          )}
        </div>

        {/* Right actions */}
        {/* biome-ignore lint/a11y/useKeyWithClickEvents: Action buttons group */}
        {/* biome-ignore lint/a11y/noStaticElementInteractions: Wrapping action buttons to prevent click propagation */}
        <div className="flex items-center gap-1 shrink-0" onClick={(e) => e.stopPropagation()}>
          {!isCurrent && (
            <button
              type="button"
              onClick={() => {
                onSwitch();
                onRefreshQuota();
              }}
              className="flex items-center gap-1.5 px-3 py-1.5 rounded-lg text-xs font-medium border transition-all hover:shadow-sm"
              style={{
                borderColor: "#00A67E40",
                color: "#00A67E",
              }}
              onMouseOver={(e) => {
                e.currentTarget.style.backgroundColor = "#00A67E10";
              }}
              onFocus={(e) => {
                e.currentTarget.style.backgroundColor = "#00A67E10";
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
              {/* Full quota bars */}
              {isOAuth && account.quota && (
                <div className="space-y-2 p-3 rounded-lg bg-muted/20">
                  <CodexQuotaBar
                    label="5小时配额"
                    percentage={account.quota.hourlyPercentage}
                    resetTime={account.quota.hourlyResetTime}
                  />
                  <CodexQuotaBar
                    label="7天配额"
                    percentage={account.quota.weeklyPercentage}
                    resetTime={account.quota.weeklyResetTime}
                  />
                </div>
              )}

              {/* Quota error */}
              {account.quotaError && (
                <div className="flex items-start gap-2 p-2.5 rounded-lg bg-destructive/10 border border-destructive/20">
                  <AlertCircle className="w-3.5 h-3.5 text-destructive shrink-0 mt-0.5" />
                  <p className="text-[10px] text-destructive leading-relaxed">{account.quotaError.message}</p>
                </div>
              )}

              {/* No quota yet */}
              {isOAuth && !account.quota && !account.quotaError && (
                <p className="text-[10px] text-muted-foreground/60 px-1">尚未获取配额数据，点击下方刷新按钮获取</p>
              )}

              {/* Account meta */}
              <div className="flex flex-wrap gap-x-4 gap-y-1 text-[10px] text-muted-foreground/60 px-1">
                {account.accountId && (
                  <span>
                    Account: <span className="font-mono">{account.accountId.slice(0, 12)}...</span>
                  </span>
                )}
                <span>创建于 {new Date(account.createdAt * 1000).toLocaleDateString("zh-CN")}</span>
              </div>

              {/* Actions */}
              <div className="flex items-center gap-2 pt-1">
                {isOAuth && (
                  <button
                    type="button"
                    onClick={onRefreshQuota}
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
