import { selectTargetableAgentProfiles } from "../../../lib/agentProfiles";
import type { AgentProfile, McpToolId, McpToolStatus } from "../../../types";

/** MCP capability mapping; visual identity always comes from AgentProfile. */
const MCP_TOOL_BY_AGENT_ID: Readonly<Partial<Record<string, McpToolId>>> = {
  claude: "claude-code",
  codex: "codex",
  gemini: "gemini",
  grok: "grok",
  opencode: "opencode",
  zcode: "zcode",
  kiro: "kiro",
  cursor: "cursor",
};

export interface McpAgentTarget {
  toolId: McpToolId;
  profile: AgentProfile;
}

/** Drop a stale toolbar filter when its Agent is no longer targetable. */
export function resolveMcpToolFilter(toolFilter: string | null, targets: readonly McpAgentTarget[]): McpToolId | null {
  if (!toolFilter) return null;
  return targets.find(({ toolId }) => toolId === toolFilter)?.toolId ?? null;
}

/**
 * Intersect Settings-active Agents with MCP support and live MCP detection.
 * Missing detection data is deliberately treated as unavailable.
 */
export function selectMcpAgentTargets(
  profiles: readonly AgentProfile[],
  toolStatuses: readonly McpToolStatus[],
): McpAgentTarget[] {
  const installedToolIds = new Set(toolStatuses.filter((status) => status.installed).map((status) => status.toolId));

  return selectTargetableAgentProfiles(profiles).flatMap((profile) => {
    const toolId = MCP_TOOL_BY_AGENT_ID[profile.id];
    return toolId && installedToolIds.has(toolId) ? [{ toolId, profile }] : [];
  });
}
