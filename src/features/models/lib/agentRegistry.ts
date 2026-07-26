/**
 * Single source of truth for the agent CLIs the Models hub can configure.
 *
 * Replaces four previously duplicated tables (ModelsHub.AGENTS,
 * ToolActivationPanel.KNOWN_TOOLS + TOOL_CONFIG_PATHS, HealthBar.AGENTS,
 * configFiles.AGENT_TOOLS). When adding a new agent CLI, extend this file —
 * see docs/features/agents/README.md.
 */

/** Tools that bind a provider (model sync). */
export type ProviderToolId = "claude-code" | "codex" | "opencode" | "gemini" | "pi";

/** All tools with on-disk config files the app can read/write. */
export type AgentToolId = ProviderToolId;

/**
 * How an agent loads providers.
 * - `single`: one global env block — exactly one active provider+model
 *   (Claude Code's `~/.claude/settings.json` env, Gemini's `~/.gemini/.env`).
 * - `multi`: the config format holds several providers at once with a pointer
 *   selecting the active one (Codex `[model_providers.*]` + `model_provider`,
 *   OpenCode `provider.*` + top-level `model`).
 */
export type AgentKind = "single" | "multi";

export interface AgentDescriptor {
  toolId: ProviderToolId;
  displayName: string;
  iconId: ProviderToolId;
  /** Which provider base URL this agent requires. */
  requiredUrlField: "openai" | "anthropic";
  /** Provider-loading model — drives which card/dialog layout renders. */
  kind: AgentKind;
  installDocsUrl: string;
  /**
   * i18n key for the tagline shown under the card title (e.g.
   * "models.card.taglines.claudeCode"). Resolve with `t(agent.taglineKey)` —
   * kept as a key rather than resolved text so this registry stays a plain
   * data table with no dependency on i18n context.
   */
  taglineKey: string;
  /** Human-readable config file location(s), display only. */
  configPathDisplay: string;
}

export const PROVIDER_AGENTS: AgentDescriptor[] = [
  {
    toolId: "claude-code",
    displayName: "Claude Code",
    iconId: "claude-code",
    requiredUrlField: "anthropic",
    kind: "single",
    installDocsUrl: "https://docs.anthropic.com/en/docs/claude-code/overview",
    taglineKey: "models.card.taglines.claudeCode",
    configPathDisplay: "~/.claude/settings.json",
  },
  {
    toolId: "codex",
    displayName: "Codex",
    iconId: "codex",
    requiredUrlField: "openai",
    kind: "multi",
    installDocsUrl: "https://github.com/openai/codex",
    taglineKey: "models.card.taglines.codex",
    configPathDisplay: "~/.codex/config.toml · ~/.codex/auth.json",
  },
  {
    toolId: "opencode",
    displayName: "OpenCode",
    iconId: "opencode",
    requiredUrlField: "openai",
    kind: "multi",
    installDocsUrl: "https://opencode.ai/docs",
    taglineKey: "models.card.taglines.opencode",
    configPathDisplay: "~/.config/opencode/opencode.json",
  },
  {
    toolId: "gemini",
    displayName: "Gemini CLI",
    iconId: "gemini",
    requiredUrlField: "openai",
    kind: "single",
    installDocsUrl: "https://github.com/google-gemini/gemini-cli",
    taglineKey: "models.card.taglines.gemini",
    configPathDisplay: "~/.gemini/.env",
  },
  {
    toolId: "pi",
    displayName: "Pi",
    iconId: "pi",
    requiredUrlField: "openai",
    kind: "multi",
    installDocsUrl: "https://pi.dev/docs/latest",
    taglineKey: "models.card.taglines.pi",
    configPathDisplay: "~/.pi/agent/models.json · ~/.pi/agent/settings.json",
  },
];

export function getAgent(toolId: string): AgentDescriptor | undefined {
  return PROVIDER_AGENTS.find((a) => a.toolId === toolId);
}

/** Whether a tool's config natively holds several providers at once. */
export function agentSupportsMultipleProviders(toolId: string): boolean {
  return getAgent(toolId)?.kind === "multi";
}

/** Does this provider expose the base URL `agent` requires? */
export function providerCompatibleWithAgent(
  agent: Pick<AgentDescriptor, "requiredUrlField">,
  provider: { base_url_openai?: string; base_url_anthropic?: string },
): boolean {
  return agent.requiredUrlField === "anthropic"
    ? Boolean(provider.base_url_anthropic?.trim())
    : Boolean(provider.base_url_openai?.trim());
}

/** Tools listed in the on-disk config file editor. */
export const CONFIG_FILE_TOOLS: { toolId: AgentToolId; label: string }[] = [
  { toolId: "claude-code", label: "Claude Code" },
  { toolId: "codex", label: "Codex" },
  { toolId: "opencode", label: "OpenCode" },
  { toolId: "gemini", label: "Gemini CLI" },
  { toolId: "pi", label: "Pi" },
];
