import { describe, expect, it } from "vitest";
import type { AgentProfile } from "../types";
import { selectTargetableAgentProfiles } from "./agentProfiles";

function profile(id: string, installed: boolean, enabled: boolean): AgentProfile {
  return {
    id,
    display_name: id,
    icon: `agents/${id}.svg`,
    global_skills_dir: `/home/test/.${id}/skills`,
    project_skills_rel: `.${id}/skills`,
    installed,
    enabled,
    synced_count: 0,
  };
}

describe("selectTargetableAgentProfiles", () => {
  it("keeps only Settings agents that are both installed and enabled", () => {
    const profiles = [
      profile("active", true, true),
      profile("disabled", true, false),
      profile("removed-but-still-enabled", false, true),
      profile("missing", false, false),
    ];

    expect(selectTargetableAgentProfiles(profiles).map(({ id }) => id)).toEqual(["active"]);
  });

  it("preserves profile order and objects without mutating the Settings list", () => {
    const first = profile("custom-first", true, true);
    first.icon = "data:image/svg+xml;base64,PHN2Zy8+";
    const second = profile("builtin-second", true, true);
    const profiles = [first, profile("hidden", false, true), second];
    const snapshot = [...profiles];

    const selected = selectTargetableAgentProfiles(profiles);

    expect(selected).toEqual([first, second]);
    expect(selected[0]).toBe(first);
    expect(profiles).toEqual(snapshot);
  });
});
