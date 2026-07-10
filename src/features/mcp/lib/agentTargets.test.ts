import { describe, expect, it } from "vitest";
import { MCP_TOOL_IDS, isMcpToolId, type AgentProfile, type McpToolStatus } from "../../../types";
import { resolveMcpToolFilter, selectMcpAgentTargets } from "./agentTargets";

function profile(id: string, installed = true, enabled = true): AgentProfile {
  return {
    id,
    display_name: id === "claude" ? "Claude Code" : id,
    icon: `agents/${id}.svg`,
    global_skills_dir: `/home/test/.${id}/skills`,
    project_skills_rel: `.${id}/skills`,
    installed,
    enabled,
    synced_count: 0,
  };
}

function status(toolId: string, installed: boolean): McpToolStatus {
  return {
    toolId,
    label: toolId,
    configPath: `/home/test/.${toolId}/config`,
    installed,
    serverCount: 0,
  };
}

describe("selectMcpAgentTargets", () => {
  it("intersects MCP support with targetable Settings profiles and live tool detection", () => {
    const targets = selectMcpAgentTargets(
      [
        profile("claude"),
        profile("codex"),
        profile("gemini", true, false),
        profile("cursor", false, true),
        profile("custom-agent"),
      ],
      [
        status("claude-code", true),
        status("claude-desktop", true),
        status("codex", false),
        status("gemini", true),
        status("cursor", true),
      ],
    );

    expect(targets.map(({ toolId, profile: agent }) => [toolId, agent.id])).toEqual([["claude-code", "claude"]]);
  });

  it("fails closed when either the Settings profile or MCP detection status is absent", () => {
    expect(selectMcpAgentTargets([profile("claude"), profile("codex")], [])).toEqual([]);
    expect(selectMcpAgentTargets([], [status("claude-code", true)])).toEqual([]);
  });

  it("preserves Settings order instead of maintaining a second SVG registry", () => {
    const targets = selectMcpAgentTargets(
      [profile("cursor"), profile("opencode"), profile("claude")],
      [status("claude-code", true), status("opencode", true), status("cursor", true)],
    );

    expect(targets.map(({ toolId }) => toolId)).toEqual(["cursor", "opencode", "claude-code"]);
    expect(targets.map(({ profile: agent }) => agent.icon)).toEqual([
      "agents/cursor.svg",
      "agents/opencode.svg",
      "agents/claude.svg",
    ]);
  });

  it("clears a tool filter that disappeared from the current target set", () => {
    const targets = selectMcpAgentTargets([profile("claude")], [status("claude-code", true)]);

    expect(resolveMcpToolFilter("claude-code", targets)).toBe("claude-code");
    expect(resolveMcpToolFilter("codex", targets)).toBeNull();
    expect(resolveMcpToolFilter(null, targets)).toBeNull();
  });

  it("exposes exactly one public Claude Code MCP target", () => {
    expect(MCP_TOOL_IDS.filter((toolId) => toolId.startsWith("claude"))).toEqual(["claude-code"]);
    expect(isMcpToolId("claude-code")).toBe(true);
    expect(isMcpToolId("claude-desktop")).toBe(false);
  });
});
