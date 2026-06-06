import { Check, ChevronDown, RefreshCw } from "lucide-react";
import { Popover } from "radix-ui";
import { useState } from "react";
import { useTranslation } from "react-i18next";
import { Switch } from "@/components/ui/switch";
import { cn } from "@/lib/utils";
import { USAGE_REFRESH_INTERVALS } from "../hooks/useUsageAutoRefresh";

interface UsageRefreshControlProps {
  onRefresh: () => Promise<void>;
  refreshing: boolean;
  refreshDisabled?: boolean;
  autoRefreshEnabled: boolean;
  intervalMs: number;
  setAutoRefreshEnabled: (enabled: boolean) => void;
  setIntervalMs: (intervalMs: number) => void;
}

export function UsageRefreshControl({
  onRefresh,
  refreshing,
  refreshDisabled = false,
  autoRefreshEnabled,
  intervalMs,
  setAutoRefreshEnabled,
  setIntervalMs,
}: UsageRefreshControlProps) {
  const { t } = useTranslation();
  const [open, setOpen] = useState(false);

  const activeInterval = USAGE_REFRESH_INTERVALS.find((item) => item.ms === intervalMs) ?? USAGE_REFRESH_INTERVALS[1];

  return (
    <Popover.Root open={open} onOpenChange={setOpen}>
      <div
        className={cn(
          "inline-flex h-8 shrink-0 overflow-hidden rounded-md border shadow-sm transition-colors",
          autoRefreshEnabled ? "border-primary/40 bg-primary/5" : "border-border/80 bg-background/50",
        )}
      >
        <button
          type="button"
          onClick={() => void onRefresh()}
          disabled={refreshing || refreshDisabled}
          className={cn(
            "inline-flex h-full items-center gap-1.5 px-2.5 text-[13px] font-medium leading-[18px] transition-colors",
            "text-foreground/80 hover:bg-accent/10 hover:text-foreground",
            "focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-primary/50 focus-visible:ring-inset",
            "disabled:pointer-events-none disabled:opacity-50",
          )}
        >
          <RefreshCw className={cn("size-4", refreshing && "animate-spin")} />
          <span>{t("usage.refreshAll")}</span>
          {autoRefreshEnabled && (
            <span className="rounded bg-primary/15 px-1.5 py-0.5 text-[10px] font-medium text-primary">
              {t(`usage.${activeInterval.key}`)}
            </span>
          )}
        </button>

        <Popover.Trigger asChild>
          <button
            type="button"
            aria-label={t("usage.refreshOptions")}
            className={cn(
              "inline-flex h-full w-7 items-center justify-center border-l transition-colors",
              autoRefreshEnabled ? "border-primary/30 hover:bg-primary/10" : "border-border/80 hover:bg-accent/10",
              "focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-primary/50 focus-visible:ring-inset",
            )}
          >
            <ChevronDown className={cn("size-3.5 text-muted-foreground transition-transform", open && "rotate-180")} />
          </button>
        </Popover.Trigger>
      </div>

      <Popover.Portal>
        <Popover.Content
          sideOffset={6}
          align="end"
          className="z-50 w-[220px] rounded-xl border border-border bg-card/95 p-3 shadow-xl backdrop-blur-xl animate-in fade-in-0 zoom-in-95 data-[state=closed]:animate-out data-[state=closed]:fade-out-0 data-[state=closed]:zoom-out-95"
        >
          <div className="flex items-center justify-between gap-3">
            <div className="min-w-0">
              <div className="text-xs font-medium text-foreground">{t("usage.autoRefresh")}</div>
              <div className="text-[11px] text-muted-foreground">
                {autoRefreshEnabled
                  ? t("usage.autoRefreshHintOn", { interval: t(`usage.${activeInterval.key}`) })
                  : t("usage.autoRefreshHintOff")}
              </div>
            </div>
            <Switch
              checked={autoRefreshEnabled}
              onCheckedChange={setAutoRefreshEnabled}
              aria-label={t("usage.autoRefresh")}
            />
          </div>

          <div className="my-3 h-px bg-border/50" />

          <div className="space-y-2">
            <div className="text-[11px] font-medium text-muted-foreground">{t("usage.refreshInterval")}</div>
            <div className="grid grid-cols-2 gap-1.5">
              {USAGE_REFRESH_INTERVALS.map((item) => {
                const selected = intervalMs === item.ms;
                return (
                  <button
                    key={item.key}
                    type="button"
                    onClick={() => setIntervalMs(item.ms)}
                    className={cn(
                      "flex items-center justify-between rounded-lg px-2.5 py-1.5 text-xs transition-colors",
                      selected ? "bg-primary/12 font-medium text-primary" : "text-foreground/80 hover:bg-accent/50",
                    )}
                  >
                    <span>{t(`usage.${item.key}`)}</span>
                    {selected && <Check className="size-3.5 shrink-0" />}
                  </button>
                );
              })}
            </div>
          </div>
        </Popover.Content>
      </Popover.Portal>
    </Popover.Root>
  );
}
