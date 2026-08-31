import { describe, expect, it } from "vitest";
import { MCP_TOOL_IDS, isMcpToolId, type AgentProfile } from "../../../types";
import { mcpToolIdsWithoutAgentProfile, resolveMcpToolFilter, selectMcpAgentTargets } from "./agentTargets";

function profile(id: string, installed = true, enabled = true): AgentProfile {
  return {
    id,
    display_name: id === "claude" ? "Claude Code" : id,
    icon: `lobe:${id}`,
    global_skills_dir: `/home/test/.${id}/skills`,
    project_skills_rel: `.${id}/skills`,
    installed,
    enabled,
    synced_count: 0,
  };
}

describe("selectMcpAgentTargets", () => {
  it("intersects MCP support with manually enabled Settings profiles", () => {
    const targets = selectMcpAgentTargets([
      profile("claude"),
      profile("codex"),
      profile("pi"),
      profile("cursor", false, true),
      profile("custom-agent"),
    ]);

    expect(targets.map(({ toolId, profile: agent }) => [toolId, agent.id])).toEqual([
      ["claude-code", "claude"],
      ["codex", "codex"],
      ["cursor", "cursor"],
    ]);
  });

  it("returns no targets when no Settings profile is enabled", () => {
    expect(selectMcpAgentTargets([profile("claude", true, false), profile("codex", false, false)])).toEqual([]);
    expect(selectMcpAgentTargets([])).toEqual([]);
  });

  it("preserves Settings order instead of maintaining a second SVG registry", () => {
    const targets = selectMcpAgentTargets([profile("cursor"), profile("opencode"), profile("claude")]);

    expect(targets.map(({ toolId }) => toolId)).toEqual(["cursor", "opencode", "claude-code"]);
    expect(targets.map(({ profile: agent }) => agent.icon)).toEqual(["lobe:cursor", "lobe:opencode", "lobe:claude"]);
  });

  it("clears a tool filter that disappeared from the current target set", () => {
    const targets = selectMcpAgentTargets([profile("claude")]);

    expect(resolveMcpToolFilter("claude-code", targets)).toBe("claude-code");
    expect(resolveMcpToolFilter("codex", targets)).toBeNull();
    expect(resolveMcpToolFilter(null, targets)).toBeNull();
  });

  it("exposes one public MCP target per Claude surface, and neither is a tombstone id", () => {
    // Code (`~/.claude.json`, CLI + Desktop Code) and Chat
    // (`claude_desktop_config.json`) are different files in different wire
    // formats. `claude-desktop` and `gemini` stay non-ids: a once-only cleanup
    // tombstone must never share an id with a standing enable flag.
    expect(MCP_TOOL_IDS.filter((toolId) => toolId.startsWith("claude"))).toEqual([
      "claude-code",
      "claude-desktop-chat",
    ]);
    expect(isMcpToolId("claude-code")).toBe(true);
    expect(isMcpToolId("claude-desktop-chat")).toBe(true);
    expect(isMcpToolId("claude-desktop")).toBe(false);
    expect(isMcpToolId("gemini")).toBe(false);
  });

  it("maps the four newly reachable targets to their own Agent profiles", () => {
    const targets = selectMcpAgentTargets([
      profile("windsurf"),
      profile("cline"),
      profile("zed"),
      profile("github-copilot"),
    ]);

    expect(targets.map(({ toolId }) => toolId)).toEqual(["windsurf", "cline", "zed", "vscode"]);
  });

  it("routes the vscode target through the github-copilot profile", () => {
    // The `vscode` MCP target writes ~/.copilot/mcp-config.json, the same config
    // root the `github-copilot` Agent profile owns. There is no `vscode` profile
    // to map instead, and inventing one in the frontend would put a second
    // Agent registry next to the Rust one.
    expect(selectMcpAgentTargets([profile("vscode")])).toEqual([]);
    expect(selectMcpAgentTargets([profile("github-copilot")]).map(({ toolId }) => toolId)).toEqual(["vscode"]);
  });

  it("routes the maka target through its own profile", () => {
    expect(selectMcpAgentTargets([profile("maka")]).map(({ toolId }) => toolId)).toEqual(["maka"]);
  });

  it("routes the gemini-cli target through its own profile, never Antigravity's", () => {
    // Both are rooted at ~/.gemini, but Antigravity is a different product and
    // must not stand in for Gemini CLI. The `gemini-cli` Agent profile
    // (crates/skillstar-agents/src/builtin.rs) is what owns this target.
    expect(selectMcpAgentTargets([profile("antigravity")])).toEqual([]);
    expect(selectMcpAgentTargets([profile("gemini-cli")]).map(({ toolId }) => toolId)).toEqual(["gemini-cli"]);
  });

  it("keeps claude-desktop-chat reachable without inventing an Agent profile for it", () => {
    // Claude Desktop is a chat app with no verified skills directory, so it
    // gets no Agent profile and therefore no Agent-rail toggle. It must still
    // be listed as an unreachable-by-profile target rather than vanish — the
    // tool view and the target picker enumerate MCP_TOOL_IDS directly.
    expect(selectMcpAgentTargets([profile("claude-desktop-chat")])).toEqual([]);
    expect(mcpToolIdsWithoutAgentProfile(MCP_TOOL_IDS, [profile("claude")])).toContain("claude-desktop-chat");
  });

  it("lists the tool ids no enabled profile can reach", () => {
    const enabled = [profile("claude"), profile("codex")];
    const unreachable = mcpToolIdsWithoutAgentProfile(MCP_TOOL_IDS, enabled);

    expect(unreachable).not.toContain("claude-code");
    expect(unreachable).not.toContain("codex");
    expect(unreachable).toContain("claude-desktop-chat");
    expect(unreachable).toContain("zed");
    expect(unreachable.length).toBe(MCP_TOOL_IDS.length - 2);
  });
});
