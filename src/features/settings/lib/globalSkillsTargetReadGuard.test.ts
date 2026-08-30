import { describe, expect, it } from "vitest";
import { GlobalSkillsTargetReadGuard } from "./globalSkillsTargetReadGuard";

describe("GlobalSkillsTargetReadGuard", () => {
  it("rejects a read that began before a target mutation", () => {
    const guard = new GlobalSkillsTargetReadGuard();
    const preload = guard.begin("/Users/test/.agent/skills/");

    guard.invalidate("/Users/test/.agent/skills");
    const authoritativeRefresh = guard.begin("/Users/test/.agent/skills");

    expect(guard.accepts(preload)).toBe(false);
    expect(guard.accepts(authoritativeRefresh)).toBe(true);
  });

  it("accepts only the latest read for a shared physical target", () => {
    const guard = new GlobalSkillsTargetReadGuard();
    const firstRead = guard.begin("C:\\Users\\test\\.agent\\skills");
    const secondRead = guard.begin("C:/Users/test/.agent/skills/");

    expect(guard.accepts(firstRead)).toBe(false);
    expect(guard.accepts(secondRead)).toBe(true);
  });
});
