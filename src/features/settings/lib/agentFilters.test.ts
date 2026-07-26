import { describe, expect, it } from "vitest";
import type { AgentProfile } from "../../../types";
import {
  countAgentStatuses,
  filterAgentProfilesByStatus,
  isAgentFilterActive,
  searchAgentProfiles,
} from "./agentFilters";

function profile(id: string, displayName: string, enabled: boolean): AgentProfile {
  return {
    id,
    display_name: displayName,
    icon: "",
    global_skills_dir: `/home/test/.${id}/skills`,
    project_skills_rel: `.${id}/skills`,
    installed: false,
    enabled,
    synced_count: 0,
  };
}

const PROFILES = [
  profile("claude", "Claude Code", true),
  profile("codex", "Codex", false),
  profile("kiro", "Kiro", false),
];

describe("agentFilters", () => {
  it("matches on display name and id, case-insensitively", () => {
    expect(searchAgentProfiles(PROFILES, "claude code").map((p) => p.id)).toEqual(["claude"]);
    expect(searchAgentProfiles(PROFILES, "CODEX").map((p) => p.id)).toEqual(["codex"]);
    expect(searchAgentProfiles(PROFILES, "co").map((p) => p.id)).toEqual(["claude", "codex"]);
  });

  it("treats a blank query as no narrowing and preserves order", () => {
    expect(searchAgentProfiles(PROFILES, "   ").map((p) => p.id)).toEqual(["claude", "codex", "kiro"]);
  });

  it("splits profiles by activation status", () => {
    expect(filterAgentProfilesByStatus(PROFILES, "all")).toHaveLength(3);
    expect(filterAgentProfilesByStatus(PROFILES, "enabled").map((p) => p.id)).toEqual(["claude"]);
    expect(filterAgentProfilesByStatus(PROFILES, "disabled").map((p) => p.id)).toEqual(["codex", "kiro"]);
  });

  it("counts statuses over whatever set it is given", () => {
    expect(countAgentStatuses(PROFILES)).toEqual({ all: 3, enabled: 1, disabled: 2 });
    expect(countAgentStatuses(searchAgentProfiles(PROFILES, "kiro"))).toEqual({ all: 1, enabled: 0, disabled: 1 });
    expect(countAgentStatuses([])).toEqual({ all: 0, enabled: 0, disabled: 0 });
  });

  it("reports an active filter only when something actually narrows", () => {
    expect(isAgentFilterActive({ query: "", status: "all" })).toBe(false);
    expect(isAgentFilterActive({ query: "  ", status: "all" })).toBe(false);
    expect(isAgentFilterActive({ query: "kiro", status: "all" })).toBe(true);
    expect(isAgentFilterActive({ query: "", status: "disabled" })).toBe(true);
  });
});
