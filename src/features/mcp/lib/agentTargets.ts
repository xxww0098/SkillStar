import { selectTargetableAgentProfiles } from "../../../lib/agentProfiles";
import type { AgentProfile, McpToolId } from "../../../types";

/**
 * MCP capability mapping; visual identity always comes from AgentProfile.
 *
 * Keys are `AgentProfile.id` (the Settings/Skills Agent registry, whose SSOT is
 * `crates/skillstar-skills/src/agents/builtin.rs`); values are `McpToolId` (whose SSOT
 * is `MCP_TOOL_IDS` in `crates/skillstar-models/src/mcp/types.rs`). The two
 * vocabularies are deliberately separate — an Agent can exist without an MCP
 * projection and vice versa — so this table is the seam, not a rename.
 *
 * Two entries are worth spelling out because the ids do not match:
 *
 * - `github-copilot -> vscode`. The `vscode` MCP target writes
 *   `~/.copilot/mcp-config.json` (`skillstar_models::mcp::tools`), and the
 *   `github-copilot` profile's skills root is `~/.copilot/skills`. Same product,
 *   same config root, so the Copilot profile is the one that legitimately owns
 *   this target's on/off state.
 * - `gemini-cli -> gemini-cli` and `antigravity -> antigravity`. Same
 *   `~/.gemini` prefix, different products and different files
 *   (`settings.json` vs `config/mcp_config.json`). Neither profile stands in
 *   for the other.
 *
 * `claude-desktop-chat` has **no** row, and that is a decision rather than an
 * omission: Claude Desktop is a chat app with no verified filesystem skills
 * directory, so inventing an Agent profile for it would put a made-up skills
 * root into the Settings/Skills registry to buy an MCP toggle. The target stays
 * fully reachable without one — the create/edit form and the tool-status view
 * enumerate `MCP_TOOL_IDS` directly, not this map — it just has no Agent rail
 * toggle, and `mcpToolIdsWithoutAgentProfile` reports it as such.
 */
const MCP_TOOL_BY_AGENT_ID: Readonly<Partial<Record<string, McpToolId>>> = {
  claude: "claude-code",
  codex: "codex",
  grok: "grok",
  hermes: "hermes",
  opencode: "opencode",
  zcode: "zcode",
  kiro: "kiro",
  cursor: "cursor",
  "github-copilot": "vscode",
  windsurf: "windsurf",
  cline: "cline",
  "gemini-cli": "gemini-cli",
  antigravity: "antigravity",
  zed: "zed",
};

const AGENT_ID_BY_MCP_TOOL: Partial<Record<McpToolId, string>> = Object.fromEntries(
  Object.entries(MCP_TOOL_BY_AGENT_ID).map(([agentId, toolId]) => [toolId, agentId]),
) as Partial<Record<McpToolId, string>>;

/**
 * Agent registry id whose brand SVG should represent this MCP target.
 * `claude-desktop-chat` has no profile (by design) but still uses Claude's mark.
 */
export function mcpIconAgentIdForTool(toolId: McpToolId): string {
  if (toolId === "claude-desktop-chat") return "claude";
  return AGENT_ID_BY_MCP_TOOL[toolId] ?? toolId;
}

export interface McpAgentTarget {
  toolId: McpToolId;
  profile: AgentProfile;
}

/** Drop a stale toolbar filter when its Agent is no longer targetable. */
export function resolveMcpToolFilter(toolFilter: string | null, targets: readonly McpAgentTarget[]): McpToolId | null {
  if (!toolFilter) return null;
  return targets.find(({ toolId }) => toolId === toolFilter)?.toolId ?? null;
}

function mappedMcpTargets(profiles: readonly AgentProfile[]): McpAgentTarget[] {
  return profiles.flatMap((profile) => {
    const toolId = MCP_TOOL_BY_AGENT_ID[profile.id];
    return toolId ? [{ toolId, profile }] : [];
  });
}

/**
 * Intersect manually activated Settings profiles with static MCP support.
 * Host tool detection must never hide a target the user explicitly enabled.
 * Toolbar filters and form defaults use this set.
 */
export function selectMcpAgentTargets(profiles: readonly AgentProfile[]): McpAgentTarget[] {
  return mappedMcpTargets(selectTargetableAgentProfiles(profiles));
}

/** Default `enabled` map for a new MCP server: currently Settings-on targets. */
export function mcpEnabledMapFromProfiles(profiles: readonly AgentProfile[]): Record<string, boolean> {
  const enabled: Record<string, boolean> = {};
  for (const { toolId } of selectMcpAgentTargets(profiles)) enabled[toolId] = true;
  return enabled;
}

/** Tool ids no enabled Agent profile can currently reach, for the tool view. */
export function mcpToolIdsWithoutAgentProfile(
  toolIds: readonly McpToolId[],
  profiles: readonly AgentProfile[],
): McpToolId[] {
  const reachable = new Set(selectMcpAgentTargets(profiles).map(({ toolId }) => toolId));
  return toolIds.filter((toolId) => !reachable.has(toolId));
}
