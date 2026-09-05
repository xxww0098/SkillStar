import type { McpServerEntry } from "../../../types";
import { MCP_FLEET_PROBE_CAP, type McpProbeEntry } from "../hooks/useMcpProbe";

/**
 * Hermes-style fleet status, mapped onto SkillStar's probe report.
 *
 * `needs-auth` is a sign-in state, never a failure. `attention` is the bucket
 * for runtime-missing + unreachable so the strip can filter without calling
 * authorization a red error.
 */
export type McpFleetStatus = "ok" | "needs-auth" | "runtime-missing" | "error" | "probing" | "unknown";

export type McpFleetHealthFilter = "all" | "healthy" | "auth" | "attention";

export const MCP_FLEET_STATUS_DOT: Record<McpFleetStatus, string> = {
  ok: "bg-emerald-500",
  "needs-auth": "bg-sky-500",
  "runtime-missing": "bg-amber-500",
  error: "bg-destructive",
  probing: "motion-safe:animate-pulse bg-foreground/40",
  unknown: "bg-foreground/25",
};

export function mcpFleetStatus(entry: McpProbeEntry): McpFleetStatus {
  if (entry.pending) return "probing";
  if (entry.error && !entry.report) return "error";
  switch (entry.report?.status) {
    case "healthy":
      return "ok";
    case "authorization-required":
      return "needs-auth";
    case "runtime-missing":
      return "runtime-missing";
    case "unreachable":
      return "error";
    default:
      return "unknown";
  }
}

export function mcpFleetStatusMatches(status: McpFleetStatus, filter: McpFleetHealthFilter): boolean {
  if (filter === "all") return true;
  if (filter === "healthy") return status === "ok";
  if (filter === "auth") return status === "needs-auth";
  return status === "runtime-missing" || status === "error";
}

export interface McpFleetHealthStats {
  total: number;
  healthy: number;
  auth: number;
  attention: number;
  pending: number;
  unchecked: number;
  schemaTokens: number;
  rest: number;
}

export function summarizeMcpFleetHealth(
  servers: readonly McpServerEntry[],
  entryFor: (id: string) => McpProbeEntry,
): McpFleetHealthStats {
  const capped = servers.slice(0, MCP_FLEET_PROBE_CAP);
  let healthy = 0;
  let auth = 0;
  let attention = 0;
  let pending = 0;
  let unchecked = 0;
  let schemaTokens = 0;
  for (const server of capped) {
    const status = mcpFleetStatus(entryFor(server.id));
    if (status === "probing") {
      pending += 1;
      continue;
    }
    if (status === "ok") {
      healthy += 1;
      schemaTokens += entryFor(server.id).report?.schemaTokens ?? 0;
      continue;
    }
    if (status === "needs-auth") {
      auth += 1;
      continue;
    }
    if (status === "runtime-missing" || status === "error") {
      attention += 1;
      continue;
    }
    unchecked += 1;
  }
  const rest = Math.max(0, servers.length - capped.length);
  return {
    total: servers.length,
    healthy,
    auth,
    attention,
    pending,
    unchecked: unchecked + rest,
    schemaTokens,
    rest,
  };
}
