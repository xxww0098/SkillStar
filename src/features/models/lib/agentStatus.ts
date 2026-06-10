/**
 * Canonical agent activation/health status model. Single source for the
 * agent cards, the section summary, and the diagnostics tab — replaces the
 * divergent computations that previously lived in HealthBar, AgentHeroCard
 * and ToolActivationPanel. Latency coloring delegates to lib/latencyColor.
 */
import type { ConnectionTestResult, ProviderEntryFlat, ToolActivation } from "../../../types";
import type { AgentDescriptor } from "./agentRegistry";
import { providerCompatibleWithAgent } from "./agentRegistry";

export type AgentStatus =
  | { kind: "not_installed" }
  | { kind: "inactive" }
  /** Bound, but the provider lacks the base URL this agent needs. */
  | { kind: "misconfigured"; requiredUrlField: "openai" | "anthropic" }
  | { kind: "syncing" }
  /** Bound and configured; no probe result yet (optimistic). */
  | { kind: "unverified" }
  | { kind: "healthy"; latencyMs: number | null }
  | { kind: "auth_failed" }
  | { kind: "timeout" }
  | { kind: "error"; detail: "network_error" | "model_unavailable" | "unknown" };

export interface AgentStatusInput {
  agent: AgentDescriptor;
  activation: ToolActivation | null;
  /** Provider resolved from activation.provider_id (null when unbound or missing). */
  boundProvider: ProviderEntryFlat | null;
  installed: boolean;
  installLoading: boolean;
  /** In-flight activate/deactivate/resync. */
  isSyncing?: boolean;
  /** Latest connection probe for the bound (tool, provider) pair. */
  probe?: ConnectionTestResult | null;
  probing?: boolean;
}

export function computeAgentStatus(input: AgentStatusInput): AgentStatus {
  const { agent, activation, boundProvider, installed, installLoading, isSyncing, probe, probing } = input;
  // While install detection runs we assume installed to avoid flashing the banner.
  if (!installLoading && !installed) return { kind: "not_installed" };
  if (isSyncing) return { kind: "syncing" };
  if (!activation || !boundProvider) return { kind: "inactive" };
  if (!providerCompatibleWithAgent(agent, boundProvider)) {
    return { kind: "misconfigured", requiredUrlField: agent.requiredUrlField };
  }
  if (probing) return { kind: "unverified" };
  if (!probe) return { kind: "unverified" };
  switch (probe.status) {
    case "ok":
      return { kind: "healthy", latencyMs: probe.latency_ms ?? null };
    case "auth_failed":
      return { kind: "auth_failed" };
    case "timeout":
      return { kind: "timeout" };
    case "network_error":
      return { kind: "error", detail: "network_error" };
    case "model_unavailable":
      return { kind: "error", detail: "model_unavailable" };
    default:
      return { kind: "error", detail: "unknown" };
  }
}

/** Is this agent usable right now (bound + configured)? */
export function isConnected(status: AgentStatus): boolean {
  return status.kind === "unverified" || status.kind === "healthy" || status.kind === "syncing";
}

/** Should the section summary count this agent as needing attention? */
export function isProblem(status: AgentStatus): boolean {
  return (
    status.kind === "misconfigured" ||
    status.kind === "auth_failed" ||
    status.kind === "timeout" ||
    status.kind === "error"
  );
}

export interface AgentSummary {
  connected: number;
  problems: number;
  total: number;
}

export function summarizeAgentStatuses(statuses: AgentStatus[]): AgentSummary {
  return {
    connected: statuses.filter(isConnected).length,
    problems: statuses.filter(isProblem).length,
    total: statuses.length,
  };
}
