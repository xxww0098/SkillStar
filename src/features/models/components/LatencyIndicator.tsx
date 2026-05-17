import { memo } from "react";
import { Button } from "../../../components/ui/button";
import { cn } from "../../../lib/utils";
import { type LatencyColor, getLatencyColor } from "../utils/latencyColor";

export interface LatencyIndicatorProps {
  latencyMs: number | null | undefined;
  variant?: "dot" | "full";
  lastTestedAt?: string | null;
  onRetest?: () => void;
}

const colorToBgClass: Record<LatencyColor, string> = {
  green: "bg-emerald-400",
  yellow: "bg-amber-400",
  red: "bg-red-400",
  gray: "bg-muted-foreground/40",
};

function getStatusText(latencyMs: number | null | undefined): { icon: string; label: string } {
  if (latencyMs == null) return { icon: "○", label: "未测试" };
  if (latencyMs < 0) return { icon: "○", label: "网络错误" };
  if (latencyMs > 2000) return { icon: "○", label: "超时" };
  return { icon: "●", label: `正常 · ${latencyMs}ms` };
}

function formatLastTested(timestamp: string | null | undefined): string | null {
  if (!timestamp) return null;
  try {
    const date = new Date(timestamp);
    if (Number.isNaN(date.getTime())) return null;
    const now = new Date();
    const diffMs = now.getTime() - date.getTime();
    const diffMin = Math.floor(diffMs / 60000);
    if (diffMin < 1) return "刚刚测试";
    if (diffMin < 60) return `${diffMin} 分钟前`;
    const diffHour = Math.floor(diffMin / 60);
    if (diffHour < 24) return `${diffHour} 小时前`;
    return date.toLocaleDateString("zh-CN", { month: "short", day: "numeric", hour: "2-digit", minute: "2-digit" });
  } catch {
    return null;
  }
}

function LatencyIndicatorInner({ latencyMs, variant = "dot", lastTestedAt, onRetest }: LatencyIndicatorProps) {
  const color = getLatencyColor(latencyMs);
  const bgClass = colorToBgClass[color];

  if (variant === "dot") {
    return <span className={cn("w-1.5 h-1.5 rounded-full shrink-0", bgClass)} aria-label={`延迟状态: ${color}`} />;
  }

  // Full variant
  const { icon, label } = getStatusText(latencyMs);
  const lastTestedLabel = formatLastTested(lastTestedAt);

  return (
    <div className="flex flex-col gap-1.5">
      <div className="flex items-center gap-2">
        <span className={cn("w-2 h-2 rounded-full shrink-0", bgClass)} />
        <span className={cn("text-xs", color === "gray" ? "text-muted-foreground" : "text-foreground")}>
          {icon} {label}
        </span>
      </div>

      {lastTestedLabel && <span className="text-[11px] text-muted-foreground pl-4">{lastTestedLabel}</span>}

      {onRetest && (
        <Button variant="ghost" size="xs" onClick={onRetest} className="self-start mt-0.5 text-muted-foreground">
          再次测试
        </Button>
      )}
    </div>
  );
}

export const LatencyIndicator = memo(LatencyIndicatorInner);
