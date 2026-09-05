import { Activity, KeyRound } from "lucide-react";
import { useMemo, type ReactNode } from "react";
import { useTranslation } from "react-i18next";
import { cn } from "../../../lib/utils";
import type { McpServerEntry } from "../../../types";
import type { McpProbeEntry } from "../hooks/useMcpProbe";
import { MCP_FLEET_PROBE_CAP } from "../hooks/useMcpProbe";
import type { McpFleetHealthFilter } from "../lib/fleetStatus";
import { summarizeMcpFleetHealth } from "../lib/fleetStatus";
import { formatSchemaTokens } from "../lib/pasteDraft";

interface McpFleetStripProps {
  servers: readonly McpServerEntry[];
  entryFor: (id: string) => McpProbeEntry;
  filter?: McpFleetHealthFilter;
  onFilterChange?: (next: McpFleetHealthFilter) => void;
}

function Chip({
  pressed,
  onClick,
  children,
  className,
}: {
  pressed: boolean;
  onClick?: () => void;
  children: ReactNode;
  className?: string;
}) {
  if (!onClick) {
    return <span className={cn("rounded-md bg-muted/70 px-1.5 py-0.5", className)}>{children}</span>;
  }
  return (
    <button
      type="button"
      aria-pressed={pressed}
      onClick={onClick}
      className={cn(
        "cursor-pointer rounded-md px-1.5 py-0.5 font-medium transition-colors duration-150 focus-ring",
        pressed ? "bg-accent text-accent-foreground" : "bg-muted/70 text-foreground/80 hover:bg-muted",
        className,
      )}
    >
      {children}
    </button>
  );
}

/**
 * Compact fleet health + schema overlay. 30-day usage is intentionally absent:
 * SkillStar is not the agent runtime and cannot observe tool calls.
 *
 * Chips filter the fleet list. Authorization is a sign-in nudge, never a red
 * failure — matching Hermes health transitions and D-052.
 */
export function McpFleetStrip({ servers, entryFor, filter = "all", onFilterChange }: McpFleetStripProps) {
  const { t } = useTranslation();
  const stats = useMemo(() => summarizeMcpFleetHealth(servers, entryFor), [servers, entryFor]);

  if (servers.length === 0) return null;

  const setFilter = (next: McpFleetHealthFilter) => {
    onFilterChange?.(filter === next ? "all" : next);
  };

  return (
    <section className="flex flex-wrap items-center gap-1.5 rounded-xl border border-border/70 bg-background/40 px-3 py-2 text-[11px] text-muted-foreground">
      <span className="inline-flex items-center gap-1 font-medium text-foreground">
        <Activity className="h-3.5 w-3.5 text-primary" />
        {t("mcp.fleetHealthTitle")}
      </span>
      <span role="status" aria-atomic="true" className="sr-only">
        {t("mcp.fleetHealthCounts", {
          healthy: stats.healthy,
          auth: stats.auth,
          pending: stats.pending,
          unchecked: stats.unchecked + stats.attention,
        })}
      </span>
      <Chip pressed={filter === "all"} onClick={onFilterChange ? () => onFilterChange("all") : undefined}>
        {t("toolbar.all")} {stats.total}
      </Chip>
      {stats.healthy > 0 || filter === "healthy" ? (
        <Chip pressed={filter === "healthy"} onClick={onFilterChange ? () => setFilter("healthy") : undefined}>
          {t("mcp.fleetFilterHealthy", { count: stats.healthy })}
        </Chip>
      ) : null}
      {stats.auth > 0 || filter === "auth" ? (
        <Chip
          pressed={filter === "auth"}
          onClick={onFilterChange ? () => setFilter("auth") : undefined}
          className={filter === "auth" ? undefined : "bg-sky-500/10 text-sky-700 paper:text-sky-800 dark:text-sky-400"}
        >
          <span className="inline-flex items-center gap-1">
            <KeyRound className="h-3 w-3" />
            {t("mcp.fleetAuthNudge", { count: stats.auth })}
          </span>
        </Chip>
      ) : null}
      {stats.attention > 0 || filter === "attention" ? (
        <Chip pressed={filter === "attention"} onClick={onFilterChange ? () => setFilter("attention") : undefined}>
          {t("mcp.fleetFilterAttention", { count: stats.attention })}
        </Chip>
      ) : null}
      {stats.schemaTokens > 0 ? (
        <span className="rounded-md bg-muted/70 px-1.5 py-0.5 font-mono text-foreground/80">
          {t("mcp.fleetSchema", { tokens: formatSchemaTokens(stats.schemaTokens) })}
        </span>
      ) : null}
      {stats.rest > 0 ? <span>{t("mcp.fleetProbeCapped", { cap: MCP_FLEET_PROBE_CAP, rest: stats.rest })}</span> : null}
    </section>
  );
}
