import { describe, expect, it } from "vitest";
import { getAgentSkillPauseSnapshot, globalSkillsTargetKey } from "./agentSkillSync";

describe("getAgentSkillPauseSnapshot", () => {
  it("waits for the backend-owned directory state", () => {
    const snapshot = getAgentSkillPauseSnapshot(undefined);

    expect(snapshot).toMatchObject({ status: "loading", action: null, checked: false });
  });

  it("pauses only the currently active directory names", () => {
    const snapshot = getAgentSkillPauseSnapshot({
      active_skill_names: ["alpha", "beta", "alpha", " "],
      suspended_skill_names: [],
    });

    expect(snapshot).toMatchObject({
      status: "active",
      action: "pause",
      checked: true,
      activeSkillNames: ["alpha", "beta"],
    });
  });

  it("restores only the persisted suspended set", () => {
    const snapshot = getAgentSkillPauseSnapshot({
      active_skill_names: [],
      suspended_skill_names: ["alpha", "retired-skill", "alpha"],
    });

    expect(snapshot).toMatchObject({
      status: "paused",
      action: "restore",
      checked: false,
      suspendedSkillNames: ["alpha", "retired-skill"],
    });
  });

  it("keeps a mixed directory in recovery mode rather than calculating Hub gaps", () => {
    const snapshot = getAgentSkillPauseSnapshot({
      active_skill_names: ["still-active"],
      suspended_skill_names: ["needs-retry"],
    });

    expect(snapshot).toMatchObject({
      status: "partial",
      action: "restore",
      checked: false,
      activeSkillNames: ["still-active"],
      suspendedSkillNames: ["needs-retry"],
    });
  });

  it("does not offer a destructive action for an empty directory without a journal", () => {
    const snapshot = getAgentSkillPauseSnapshot({
      active_skill_names: [],
      suspended_skill_names: [],
    });

    expect(snapshot).toMatchObject({ status: "empty", action: null, checked: false });
  });
});

describe("globalSkillsTargetKey", () => {
  it("groups equivalent separator and trailing-slash spellings", () => {
    expect(globalSkillsTargetKey("/Users/test/.agent/skills/")).toBe("/Users/test/.agent/skills");
    expect(globalSkillsTargetKey("C:\\Users\\test\\.agent\\skills")).toBe("C:/Users/test/.agent/skills");
  });
});
