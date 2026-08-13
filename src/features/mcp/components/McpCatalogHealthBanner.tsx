import { AlertTriangle, CircleCheck, Clock, RefreshCw } from "lucide-react";
import { useTranslation } from "react-i18next";
import { Button } from "../../../components/ui/button";
import { cn } from "../../../lib/utils";
import type { McpCatalogHealth } from "../lib/sourceHealth";

/**
 * Snapshot freshness and degradation, above the catalog grid.
 *
 * The point of this strip is the negative case. A sync where one of four
 * sources was rate-limited still reports success, and the grid below looks
 * exactly as complete as a healthy one — so the user has to be told, in words,
 * that what they are browsing is not the whole catalog and which source is
 * missing from it. Silence here is the bug (audit D.3-8).
 */

function formatTime(value: string | null): string | null {
  if (!value) return null;
  const parsed = Date.parse(value);
  if (Number.isNaN(parsed)) return null;
  return new Date(parsed).toLocaleString();
}

interface McpCatalogHealthBannerProps {
  health: McpCatalogHealth;
  onRefresh: () => void;
  refreshing: boolean;
  className?: string;
}

export function McpCatalogHealthBanner({ health, onRefresh, refreshing, className }: McpCatalogHealthBannerProps) {
  const { t } = useTranslation();
  const lastSuccess = formatTime(health.lastSuccessAt);
  const complete = !health.incomplete && health.reasons.length === 0;

  return (
    <div
      className={cn(
        "rounded-xl border px-3.5 py-2.5 text-xs",
        health.incomplete
          ? "border-amber-500/30 bg-amber-500/8 text-amber-700 dark:text-amber-300"
          : "border-border/60 bg-background/40 text-muted-foreground",
        className,
      )}
    >
      <div className="flex flex-wrap items-center gap-x-3 gap-y-1.5">
        <span className="inline-flex items-center gap-1.5 font-medium">
          {health.incomplete ? <AlertTriangle className="h-3.5 w-3.5" /> : <CircleCheck className="h-3.5 w-3.5" />}
          {health.incomplete ? t("mcp.catalogIncomplete") : t("mcp.catalogComplete")}
        </span>

        <span className="inline-flex items-center gap-1.5">
          <Clock className="h-3.5 w-3.5" />
          {lastSuccess ? t("mcp.catalogLastSync", { time: lastSuccess }) : t("mcp.catalogNeverSynced")}
        </span>

        <span className="tabular-nums">
          {t("mcp.catalogSourceCount", { fresh: health.freshCount, total: health.enabledCount })}
        </span>

        <Button
          type="button"
          variant="ghost"
          size="sm"
          className="ml-auto h-7 gap-1.5 px-2 text-[11px]"
          onClick={onRefresh}
          disabled={refreshing}
        >
          <RefreshCw className={refreshing ? "h-3 w-3 animate-spin" : "h-3 w-3"} />
          {refreshing ? t("marketplace.refreshingSnapshot") : t("common.refresh")}
        </Button>
      </div>

      {health.reasons.length > 0 ? (
        <ul className="mt-2 space-y-1 border-t border-current/15 pt-2">
          {health.reasons.map((reason) => (
            <li key={reason.sourceId} className="flex flex-wrap items-baseline gap-x-1.5">
              <span className="font-medium">{reason.label}</span>
              <span className="opacity-80">{t(`mcp.sourceHealth_${reason.health}`)}</span>
              {reason.detail ? <span className="break-all font-mono opacity-70">— {reason.detail}</span> : null}
            </li>
          ))}
        </ul>
      ) : null}

      {complete ? null : <p className="mt-1.5 text-[11px] opacity-75">{t("mcp.catalogIncompleteHint")}</p>}
    </div>
  );
}
