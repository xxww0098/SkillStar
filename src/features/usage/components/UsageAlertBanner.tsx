import { AlertTriangle, Info, ShieldAlert, X } from "lucide-react";
import { useState } from "react";
import { cn } from "@/lib/utils";
import type { SubscriptionAlert } from "../types";

interface UsageAlertBannerProps {
  alerts: SubscriptionAlert[];
  onDismiss: (alertId: string) => void;
}

export function UsageAlertBanner({ alerts, onDismiss }: UsageAlertBannerProps) {
  const [collapsed, setCollapsed] = useState(false);
  if (alerts.length === 0) return null;
  const top = alerts.slice(0, collapsed ? 1 : 4);

  return (
    <div className="space-y-1.5 border-b border-border/40 bg-card/30 px-4 py-2">
      {top.map((alert) => (
        <div
          key={alert.id}
          className={cn(
            "flex items-center gap-2 rounded-md border px-3 py-1.5 text-[12px]",
            toneClasses(alert.severity),
          )}
        >
          {toneIcon(alert.severity)}
          <span className="flex-1 truncate" title={alert.message}>
            {alert.message}
          </span>
          <button
            type="button"
            className="text-current/60 hover:text-current"
            onClick={() => onDismiss(alert.id)}
            aria-label="忽略提醒"
          >
            <X className="w-3.5 h-3.5" />
          </button>
        </div>
      ))}
      {alerts.length > 4 && !collapsed && (
        <button
          type="button"
          className="text-[11px] text-muted-foreground hover:text-foreground"
          onClick={() => setCollapsed(true)}
        >
          收起更多 {alerts.length - 4} 条提醒
        </button>
      )}
      {collapsed && alerts.length > 1 && (
        <button
          type="button"
          className="text-[11px] text-muted-foreground hover:text-foreground"
          onClick={() => setCollapsed(false)}
        >
          展开全部 {alerts.length} 条提醒
        </button>
      )}
    </div>
  );
}

function toneClasses(severity: SubscriptionAlert["severity"]) {
  switch (severity) {
    case "danger":
      return "border-red-500/40 bg-red-500/10 text-red-300";
    case "warning":
      return "border-amber-500/40 bg-amber-500/10 text-amber-300";
    default:
      return "border-blue-500/40 bg-blue-500/10 text-blue-300";
  }
}

function toneIcon(severity: SubscriptionAlert["severity"]) {
  switch (severity) {
    case "danger":
      return <ShieldAlert className="w-3.5 h-3.5 shrink-0" />;
    case "warning":
      return <AlertTriangle className="w-3.5 h-3.5 shrink-0" />;
    default:
      return <Info className="w-3.5 h-3.5 shrink-0" />;
  }
}
