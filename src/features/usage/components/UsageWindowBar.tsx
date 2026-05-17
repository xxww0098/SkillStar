import { cn } from "@/lib/utils";
import type { UsageWindow } from "../types";

interface UsageWindowBarProps {
  window: UsageWindow;
  compact?: boolean;
}

/**
 * Generic 5h / 7d / 30d / 余额 progress bar.
 * Colors follow CodexQuotaBar conventions (green / amber / orange / red).
 */
export function UsageWindowBar({ window, compact }: UsageWindowBarProps) {
  const percent = clamp(window.percent ?? computePercent(window.used, window.total));
  const remaining = 100 - percent;
  const tone = pickTone(remaining);
  const reset = window.reset_at ? formatRelative(window.reset_at) : null;

  return (
    <div className={cn("space-y-1", compact ? "text-[10px]" : "text-xs")}>
      <div className="flex items-center justify-between text-muted-foreground">
        <span className="font-medium">{window.label}</span>
        <span className="tabular-nums">
          {Number.isFinite(percent) ? `${percent}%` : "—"}
          {reset && <span className="ml-1 opacity-60">· 重置 {reset}</span>}
        </span>
      </div>
      <div className="h-1.5 w-full rounded-full bg-muted/60 overflow-hidden">
        <div
          className={cn("h-full rounded-full transition-[width] duration-300", tone)}
          style={{ width: `${Math.max(2, percent)}%` }}
        />
      </div>
    </div>
  );
}

function clamp(p: number | null): number {
  if (p === null || Number.isNaN(p)) return 0;
  return Math.max(0, Math.min(100, Math.round(p)));
}

function computePercent(used: number, total: number | null): number | null {
  if (!total || total <= 0) return null;
  return Math.round((used / total) * 100);
}

function pickTone(remaining: number): string {
  if (remaining < 5) return "bg-red-500";
  if (remaining < 20) return "bg-orange-500";
  if (remaining < 40) return "bg-amber-500";
  return "bg-emerald-500";
}

function formatRelative(epoch: number): string {
  const now = Math.floor(Date.now() / 1000);
  const diff = epoch - now;
  if (diff <= 0) return "已到";
  const days = Math.floor(diff / 86_400);
  const hours = Math.floor((diff % 86_400) / 3_600);
  const minutes = Math.floor((diff % 3_600) / 60);
  if (days > 0) return `${days}d${hours > 0 ? `${hours}h` : ""}`;
  if (hours > 0) return `${hours}h${minutes > 0 ? `${minutes}m` : ""}`;
  return `${Math.max(1, minutes)}m`;
}
