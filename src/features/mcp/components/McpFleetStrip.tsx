import { Activity, KeyRound } from "lucide-react";
import { useMemo } from "react";
import { useTranslation } from "react-i18next";
import type { McpServerEntry } from "../../../types";
import type { McpProbeEntry } from "../hooks/useMcpProbe";
import { MCP_FLEET_PROBE_CAP } from "../hooks/useMcpProbe";
import { formatSchemaTokens } from "../lib/pasteDraft";

interface McpFleetStripProps {
  servers: readonly McpServerEntry[];
  entryFor: (id: string) => McpProbeEntry;
}

/**
 * Compact fleet health + schema overlay. 30-day usage is intentionally absent:
 * SkillStar is not the agent runtime and cannot observe tool calls.
 */
export function McpFleetStrip({ servers, entryFor }: McpFleetStripProps) {
  const { t } = useTranslation();
  const stats = useMemo(() => {
    const capped = servers.slice(0, MCP_FLEET_PROBE_CAP);
    let healthy = 0;
    let auth = 0;
    let missing = 0;
    let unreachable = 0;
    let pending = 0;
    let unchecked = 0;
    let schemaTokens = 0;
    for (const server of capped) {
      const entry = entryFor(server.id);
      if (entry.pending) {
        pending += 1;
        continue;
      }
      const status = entry.report?.status;
      if (!status) {
        unchecked += 1;
        continue;
      }
      if (status === "healthy") {
        healthy += 1;
        schemaTokens += entry.report?.schemaTokens ?? 0;
      } else if (status === "authorization-required") auth += 1;
      else if (status === "runtime-missing") missing += 1;
      else unreachable += 1;
    }
    return {
      probed: capped.length,
      rest: Math.max(0, servers.length - capped.length),
      healthy,
      auth,
      missing,
      unreachable,
      pending,
      unchecked,
      schemaTokens,
    };
  }, [servers, entryFor]);

  if (servers.length === 0) return null;

  return (
    <section className="flex flex-wrap items-center gap-2 rounded-xl border border-border/70 bg-background/40 px-3 py-2 text-[11px] text-muted-foreground">
      <span className="inline-flex items-center gap-1 font-medium text-foreground">
        <Activity className="h-3.5 w-3.5 text-primary" />
        {t("mcp.fleetHealthTitle")}
      </span>
      <span>
        {t("mcp.fleetHealthCounts", {
          healthy: stats.healthy,
          auth: stats.auth,
          pending: stats.pending,
          unchecked: stats.unchecked + stats.missing + stats.unreachable,
        })}
      </span>
      {stats.schemaTokens > 0 ? (
        <span className="rounded-md bg-muted/70 px-1.5 py-0.5 font-mono text-foreground/80">
          {t("mcp.fleetSchema", { tokens: formatSchemaTokens(stats.schemaTokens) })}
        </span>
      ) : null}
      {stats.rest > 0 ? <span>{t("mcp.fleetProbeCapped", { cap: MCP_FLEET_PROBE_CAP, rest: stats.rest })}</span> : null}
      {stats.auth > 0 ? (
        <span className="inline-flex items-center gap-1 rounded-md bg-sky-500/10 px-1.5 py-0.5 font-medium text-sky-700 dark:text-sky-400">
          <KeyRound className="h-3 w-3" />
          {t("mcp.fleetAuthNudge", { count: stats.auth })}
        </span>
      ) : null}
    </section>
  );
}
