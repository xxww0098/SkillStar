import { AlertTriangle, Info, ShieldAlert, X } from "lucide-react";
import { useState } from "react";
import { useTranslation } from "react-i18next";
import { cn } from "@/lib/utils";
import type { SubscriptionAlert } from "../types";

interface UsageAlertBannerProps {
  alerts: SubscriptionAlert[];
  onDismiss: (alertId: string) => void;
}

export function UsageAlertBanner({ alerts, onDismiss }: UsageAlertBannerProps) {
  const { t } = useTranslation();
  const [expanded, setExpanded] = useState(false);
  if (alerts.length === 0) return null;
  const visibleAlerts = expanded ? alerts : alerts.slice(0, 2);
  const hiddenCount = alerts.length - visibleAlerts.length;

  return (
    <div className="max-h-[30vh] shrink-0 overflow-y-auto space-y-1.5 border-b border-border/40 bg-card/30 px-4 py-2">
      {visibleAlerts.map((alert) => (
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
            className="text-current/75 hover:text-current"
            onClick={() => onDismiss(alert.id)}
            aria-label={t("usage.dismissAlert")}
          >
            <X className="w-3.5 h-3.5" />
          </button>
        </div>
      ))}
      {alerts.length > 2 && (
        <button
          type="button"
          className="text-[11px] text-muted-foreground hover:text-foreground"
          onClick={() => setExpanded((value) => !value)}
          aria-expanded={expanded}
        >
          {expanded ? t("common.collapse") : t("usage.expandAlerts", { count: hiddenCount })}
        </button>
      )}
    </div>
  );
}

/**
 * Dark-theme tints stay as-is; `paper:` picks the darker ramp so the text keeps
 * WCAG AA against the light tinted background. `dark:` would be wrong here — it
 * tracks the OS preference, not the in-app `data-bg-style` switch.
 */
function toneClasses(severity: SubscriptionAlert["severity"]) {
  switch (severity) {
    case "danger":
      return "border-red-500/40 bg-red-500/10 text-red-300 paper:border-red-600/50 paper:text-red-700";
    case "warning":
      return "border-amber-500/40 bg-amber-500/10 text-amber-300 paper:border-amber-600/50 paper:text-amber-800";
    default:
      return "border-blue-500/40 bg-blue-500/10 text-blue-300 paper:border-blue-600/50 paper:text-blue-700";
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
