import { selectTargetableAgentProfiles } from "../../../lib/agentProfiles";
import type { AgentProfile, McpToolId } from "../../../types";

/**
 * MCP capability mapping; visual identity always comes from AgentProfile.
 *
 * Keys are `AgentProfile.id` (the Settings/Skills Agent registry, whose SSOT is
 * `crates/skillstar-agents/src/builtin.rs`); values are `McpToolId` (whose SSOT
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
 * - `gemini-cli -> gemini-cli`. Same id on both sides, but the pairing is not
 *   automatic: the two `~/.gemini`-rooted profiles that came from upstream are
 *   Google **Antigravity**'s, a different product that must not stand in for
 *   Gemini CLI. The `gemini-cli` profile is a SkillStar extension added
 *   alongside this row precisely so the target has an owner.
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
  opencode: "opencode",
  zcode: "zcode",
  kiro: "kiro",
  cursor: "cursor",
  "github-copilot": "vscode",
  windsurf: "windsurf",
  cline: "cline",
  "gemini-cli": "gemini-cli",
  zed: "zed",
  maka: "maka",
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

/** Tool ids no enabled Agent profile can currently reach, for the tool view. */
export function mcpToolIdsWithoutAgentProfile(
  toolIds: readonly McpToolId[],
  profiles: readonly AgentProfile[],
): McpToolId[] {
  const reachable = new Set(selectMcpAgentTargets(profiles).map(({ toolId }) => toolId));
  return toolIds.filter((toolId) => !reachable.has(toolId));
}
