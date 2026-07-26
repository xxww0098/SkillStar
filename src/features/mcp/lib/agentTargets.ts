import { selectTargetableAgentProfiles } from "../../../lib/agentProfiles";
import type { AgentProfile, McpToolId } from "../../../types";

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
 * Intersect manually activated Settings profiles with static MCP support.
 * Host tool detection must never hide a target the user explicitly enabled.
 */
export function selectMcpAgentTargets(profiles: readonly AgentProfile[]): McpAgentTarget[] {
  return selectTargetableAgentProfiles(profiles).flatMap((profile) => {
    const toolId = MCP_TOOL_BY_AGENT_ID[profile.id];
    return toolId ? [{ toolId, profile }] : [];
  });
}
