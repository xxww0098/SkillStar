import { describe, expect, it } from "vitest";
import type { AgentProfile } from "../types";
import { selectRailAgentProfiles, selectTargetableAgentProfiles, supportsGlobalDeploy } from "./agentProfiles";

function profile(id: string, installed: boolean, enabled: boolean): AgentProfile {
  return {
    id,
    display_name: id,
    icon: `lobe:${id}`,
    global_skills_dir: `/home/test/.${id}/skills`,
    project_skills_rel: `.${id}/skills`,
    installed,
    enabled,
    synced_count: 0,
  };
}

describe("selectTargetableAgentProfiles", () => {
  it("keeps only Agents the user enabled in Settings", () => {
    const profiles = [
      profile("active", true, true),
      profile("disabled", true, false),
      profile("manual", false, true),
      profile("missing", false, false),
    ];

    expect(selectTargetableAgentProfiles(profiles).map(({ id }) => id)).toEqual(["active", "manual"]);
  });

  it("preserves profile order and objects without mutating the Settings list", () => {
    const first = profile("custom-first", true, true);
    first.icon = "data:image/svg+xml;base64,PHN2Zy8+";
    const second = profile("builtin-second", true, true);
    const profiles = [first, profile("hidden", true, false), second];
    const snapshot = [...profiles];

    const selected = selectTargetableAgentProfiles(profiles);

    expect(selected).toEqual([first, second]);
    expect(selected[0]).toBe(first);
    expect(profiles).toEqual(snapshot);
  });

  it("keeps a disabled Agent on the rail when the resource is still attached", () => {
    const profiles = [profile("active", true, true), profile("stale", true, false), profile("idle", true, false)];
    const rail = selectRailAgentProfiles(profiles, new Set(["stale"]));
    expect(rail.map(({ id }) => id)).toEqual(["active", "stale"]);
  });

  it("matches attached Skills by display name as well as id", () => {
    const claude = profile("claude", true, false);
    claude.display_name = "Claude Code";
    expect(selectRailAgentProfiles([claude], new Set(["Claude Code"])).map(({ id }) => id)).toEqual(["claude"]);
  });

  it("treats an empty global path as project-only", () => {
    const projectOnly = profile("eve", true, true);
    projectOnly.global_skills_dir = "";
    expect(supportsGlobalDeploy(projectOnly)).toBe(false);
  });
});
