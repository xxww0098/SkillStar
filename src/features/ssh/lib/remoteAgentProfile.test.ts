import { describe, expect, it } from "vitest";
import { remoteAgentProfile } from "./remoteAgentProfile";

describe("remoteAgentProfile", () => {
  it("normalizes the Models alias to the single Claude Code identity", () => {
    expect(remoteAgentProfile("claude-code", [])).toMatchObject({
      id: "claude",
      display_name: "Claude Code",
      icon: "agents/claude.svg",
    });
  });
});
