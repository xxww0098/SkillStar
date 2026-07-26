import { describe, expect, it } from "vitest";
import type { AgentProfile } from "../../../types";
import { remoteAgentProfile } from "./remoteAgentProfile";

describe("remoteAgentProfile", () => {
  it("normalizes the Models alias to the single Claude Code identity", () => {
    expect(remoteAgentProfile("claude-code", [])).toMatchObject({
      id: "claude",
      display_name: "Claude Code",
      icon: "lobe:claude",
      installed: true,
      enabled: true,
    });
  });

  it("keeps explicit remote discovery independent from the local Settings switch", () => {
    const local: AgentProfile = {
      id: "codex",
      display_name: "Codex",
      icon: "lobe:codex",
      global_skills_dir: "/home/local/.codex/skills",
      project_skills_rel: ".agents/skills",
      installed: false,
      enabled: false,
      synced_count: 0,
    };

    expect(remoteAgentProfile("codex", [local])).toMatchObject({
      id: "codex",
      icon: "lobe:codex",
      installed: true,
      enabled: true,
    });
  });
});
