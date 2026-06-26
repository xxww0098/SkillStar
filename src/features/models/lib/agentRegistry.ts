/**
 * Single source of truth for the agent CLIs the Models hub can configure.
 *
 * Replaces four previously duplicated tables (ModelsHub.AGENTS,
 * ToolActivationPanel.KNOWN_TOOLS + TOOL_CONFIG_PATHS, HealthBar.AGENTS,
 * configFiles.AGENT_TOOLS). When adding a new agent CLI, extend this file —
 * see ADDING-AN-AGENT.md.
 */

/** Tools that bind a provider (model sync). */
export type ProviderToolId = "claude-code" | "codex" | "opencode" | "gemini";

/** All tools with on-disk config files the app can read/write. */
export type AgentToolId = ProviderToolId | "claude-desktop";

export interface AgentDescriptor {
  toolId: ProviderToolId;
  displayName: string;
  iconId: ProviderToolId;
  /** Which provider base URL this agent requires. */
  requiredUrlField: "openai" | "anthropic";
  installDocsUrl: string;
  /** Tagline shown under the card title. */
  tagline: string;
  /** Tooltip when activation is blocked by the missing base URL. */
  disabledTooltip: string;
  /** Human-readable config file location(s), display only. */
  configPathDisplay: string;
}

export const PROVIDER_AGENTS: AgentDescriptor[] = [
  {
    toolId: "claude-code",
    displayName: "Claude",
    iconId: "claude-code",
    requiredUrlField: "anthropic",
    installDocsUrl: "https://docs.anthropic.com/en/docs/claude-code/overview",
    tagline: "Anthropic 兼容 · 写入 ~/.claude/settings.json",
    disabledTooltip: "此供应商未提供 Anthropic 兼容端点",
    configPathDisplay: "~/.claude/settings.json",
  },
  {
    toolId: "codex",
    displayName: "Codex",
    iconId: "codex",
    requiredUrlField: "openai",
    installDocsUrl: "https://github.com/openai/codex",
    tagline: "CLI · Desktop App · IDE 扩展 共用 ~/.codex/ 配置",
    disabledTooltip: "此供应商未提供 OpenAI 兼容端点",
    configPathDisplay: "~/.codex/config.toml · ~/.codex/auth.json",
  },
  {
    toolId: "opencode",
    displayName: "OpenCode",
    iconId: "opencode",
    requiredUrlField: "openai",
    installDocsUrl: "https://opencode.ai/docs",
    tagline: "OpenAI 兼容 · 开源 IDE 代理",
    disabledTooltip: "此供应商未提供 OpenAI 兼容端点",
    configPathDisplay: "~/.config/opencode/opencode.json",
  },
  {
    toolId: "gemini",
    displayName: "Gemini CLI",
    iconId: "gemini",
    requiredUrlField: "openai",
    installDocsUrl: "https://github.com/google-gemini/gemini-cli",
    tagline: "OpenAI 兼容 · 写入 ~/.gemini/.env",
    disabledTooltip: "此供应商未提供 OpenAI 兼容端点",
    configPathDisplay: "~/.gemini/.env",
  },
];

export const CLAUDE_DESKTOP_TOOL_ID = "claude-desktop" as const;

export function getAgent(toolId: string): AgentDescriptor | undefined {
  return PROVIDER_AGENTS.find((a) => a.toolId === toolId);
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

/** Tools listed in the on-disk config file editor (provider agents + Claude Desktop). */
export const CONFIG_FILE_TOOLS: { toolId: AgentToolId; label: string }[] = [
  { toolId: "claude-code", label: "Claude" },
  { toolId: "codex", label: "Codex" },
  { toolId: "opencode", label: "OpenCode" },
  { toolId: CLAUDE_DESKTOP_TOOL_ID, label: "Claude Desktop" },
  { toolId: "gemini", label: "Gemini CLI" },
];
