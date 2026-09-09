/** Presentation scope for the Claude-only workbench, not the backend agent registry. */
export type ClaudeClientId = "claude-code" | "claude-desktop";
export type ClaudeSurface = "cli" | "desktop";

export const CLAUDE_CLIENTS = [
  { id: "claude-code", surface: "cli", label: "Claude Code CLI" },
  { id: "claude-desktop", surface: "desktop", label: "Claude Code Desktop" },
] as const satisfies readonly { id: ClaudeClientId; surface: ClaudeSurface; label: string }[];

/**
 * Desktop is still registered by the backend, but its writer only emits a
 * SkillStar marker, not native Desktop configuration. Neither descriptor
 * presence nor that writer's success result makes it configurable here.
 */
export function canConfigureClaudeClient(clientId: ClaudeClientId): boolean {
  return clientId === "claude-code";
}
