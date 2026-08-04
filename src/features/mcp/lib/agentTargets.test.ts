import { describe, expect, it } from "vitest";
import { MCP_TOOL_IDS, isMcpToolId, type AgentProfile } from "../../../types";
import { resolveMcpToolFilter, selectMcpAgentTargets } from "./agentTargets";

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

  it("exposes exactly one public Claude Code MCP target", () => {
    expect(MCP_TOOL_IDS.filter((toolId) => toolId.startsWith("claude"))).toEqual(["claude-code"]);
    expect(isMcpToolId("claude-code")).toBe(true);
    expect(isMcpToolId("claude-desktop")).toBe(false);
    expect(isMcpToolId("gemini")).toBe(false);
  });
});
