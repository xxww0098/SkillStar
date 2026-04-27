import { motion } from "framer-motion";
import { AlertCircle, AlertTriangle, CheckCircle2, HelpCircle, Loader2, RefreshCw, WifiOff } from "lucide-react";
import { useCallback } from "react";
import { cn } from "../../../lib/utils";
import type { ProviderHealthDashboard } from "../hooks/useProviderHealthDashboard";
import type { ModelAppId } from "./AppCapsuleSwitcher";

interface ProviderHealthDashboardProps {
  dashboard: ProviderHealthDashboard | null;
  loading: boolean;
  refreshing: boolean;
  onRefresh: () => void;
  appId: ModelAppId;
  appColor: string;
}

const STATUS_META: Record<string, { label: string; icon: React.ReactNode; colorClass: string; bgClass: string }> = {
  healthy: {
    label: "正常",
    icon: <CheckCircle2 className="w-3.5 h-3.5" />,
    colorClass: "text-emerald-500",
    bgClass: "bg-emerald-500/10 border-emerald-500/20",
  },
  degraded: {
    label: "缓慢",
    icon: <AlertTriangle className="w-3.5 h-3.5" />,
    colorClass: "text-amber-500",
    bgClass: "bg-amber-500/10 border-amber-500/20",
  },
  unreachable: {
    label: "不可达",
    icon: <WifiOff className="w-3.5 h-3.5" />,
    colorClass: "text-rose-500",
    bgClass: "bg-rose-500/10 border-rose-500/20",
  },
  unknown: {
    label: "未知",
    icon: <HelpCircle className="w-3.5 h-3.5" />,
    colorClass: "text-slate-400",
    bgClass: "bg-slate-400/10 border-slate-400/20",
  },
};

export function ProviderHealthDashboardCard({
  dashboard,
  loading,
  refreshing,
  onRefresh,
}: ProviderHealthDashboardProps) {
  const handleRefresh = useCallback(() => {
    if (!refreshing) onRefresh();
  }, [refreshing, onRefresh]);

  if (loading && !dashboard) {
    return (
      <div className="flex items-center justify-center py-6">
        <Loader2 className="w-4 h-4 animate-spin text-muted-foreground" />
      </div>
    );
  }

  if (!dashboard || dashboard.summary.total === 0) {
    return null;
  }

  const { summary, entries } = dashboard;

  return (
    <motion.div
      initial={{ opacity: 0, y: 6 }}
      animate={{ opacity: 1, y: 0 }}
      transition={{ duration: 0.2 }}
      className="rounded-2xl border border-border/70 bg-card/60 backdrop-blur-sm overflow-hidden"
    >
      <div className="px-4 py-3 border-b border-border/50 flex items-center justify-between gap-3">
        <div className="flex items-center gap-2 min-w-0">
          <span className="text-xs font-semibold text-foreground">健康状态</span>
          <div className="flex items-center gap-1.5">
            {summary.healthy > 0 && (
              <span className="inline-flex items-center gap-1 px-1.5 py-0.5 rounded-md bg-emerald-500/10 text-emerald-500 text-[10px] font-medium">
                <CheckCircle2 className="w-3 h-3" />
                {summary.healthy}
              </span>
            )}
            {summary.degraded > 0 && (
              <span className="inline-flex items-center gap-1 px-1.5 py-0.5 rounded-md bg-amber-500/10 text-amber-500 text-[10px] font-medium">
                <AlertTriangle className="w-3 h-3" />
                {summary.degraded}
              </span>
            )}
            {summary.unreachable > 0 && (
              <span className="inline-flex items-center gap-1 px-1.5 py-0.5 rounded-md bg-rose-500/10 text-rose-500 text-[10px] font-medium">
                <WifiOff className="w-3 h-3" />
                {summary.unreachable}
              </span>
            )}
            {summary.unknown > 0 && (
              <span className="inline-flex items-center gap-1 px-1.5 py-0.5 rounded-md bg-slate-400/10 text-slate-400 text-[10px] font-medium">
                <HelpCircle className="w-3 h-3" />
                {summary.unknown}
              </span>
            )}
          </div>
        </div>

        <button
          type="button"
          onClick={handleRefresh}
          disabled={refreshing}
          className="flex items-center gap-1 px-2.5 py-1 rounded-lg text-[11px] font-medium text-muted-foreground hover:text-foreground hover:bg-muted/50 border border-border transition-all disabled:opacity-50 shrink-0"
          title="刷新健康状态"
        >
          {refreshing ? <Loader2 className="w-3 h-3 animate-spin" /> : <RefreshCw className="w-3 h-3" />}
          刷新
        </button>
      </div>

      <div className="px-4 py-3">
        <div className="grid grid-cols-1 sm:grid-cols-2 gap-2">
          {entries.map((entry) => {
            const meta = STATUS_META[entry.healthStatus] || STATUS_META.unknown;
            return (
              <div
                key={entry.providerId}
                className={cn("flex items-center gap-2.5 px-3 py-2 rounded-xl border transition-colors", meta.bgClass)}
              >
                <div className={cn("shrink-0", meta.colorClass)}>{meta.icon}</div>
                <div className="flex-1 min-w-0">
                  <div className="flex items-center gap-1.5">
                    <span className="text-xs font-medium text-foreground truncate">{entry.name}</span>
                    {entry.error && (
                      <span title={entry.error}>
                        <AlertCircle className="w-3 h-3 text-rose-400 shrink-0" />
                      </span>
                    )}
                  </div>
                  <div className="flex items-center gap-2 mt-0.5">
                    {entry.latencyMs !== null && (
                      <span className="text-[10px] text-muted-foreground font-mono">{entry.latencyMs}ms</span>
                    )}
                    {entry.usagePercent !== null && (
                      <span className="text-[10px] text-muted-foreground font-mono">用量 {entry.usagePercent}%</span>
                    )}
                    {entry.remaining && (
                      <span className="text-[10px] text-muted-foreground font-mono truncate">余 {entry.remaining}</span>
                    )}
                  </div>
                </div>
              </div>
            );
          })}
        </div>
      </div>
    </motion.div>
  );
}
