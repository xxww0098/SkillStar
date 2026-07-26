import { describe, expect, it } from "vitest";
import type { AgentProfile } from "../types";
import { selectTargetableAgentProfiles, supportsGlobalDeploy } from "./agentProfiles";

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
      profile("gemini", false, true),
      profile("missing", false, false),
    ];

    expect(selectTargetableAgentProfiles(profiles).map(({ id }) => id)).toEqual(["active", "gemini"]);
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

  it("treats an empty global path as project-only", () => {
    const projectOnly = profile("eve", true, true);
    projectOnly.global_skills_dir = "";
    expect(supportsGlobalDeploy(projectOnly)).toBe(false);
  });
});
