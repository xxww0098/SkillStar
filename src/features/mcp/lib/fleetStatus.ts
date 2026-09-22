import type { McpProbeEntry } from "../hooks/useMcpProbe";

/**
 * Per-server health, mapped onto SkillStar's probe report.
 *
 * There is no aggregate health dashboard any more: health is a property of one
 * server, shown as the status dot on its card and spelled out by the probe
 * panel in its editor. `needs-auth` is a sign-in state, never a failure — the
 * probe answered `401 + WWW-Authenticate`, which means the server is up and
 * asking for authorization rather than broken.
 */
export type McpFleetStatus = "ok" | "needs-auth" | "runtime-missing" | "error" | "probing" | "unknown";

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
