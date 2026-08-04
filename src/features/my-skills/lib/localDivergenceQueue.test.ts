import { describe, expect, it } from "vitest";
import type { ResolveSkillUpdateResult, SkillUpdateBlocked } from "../../../types";
import { reconcileBlockedUpdates } from "./localDivergenceQueue";

function blocked(name: string): SkillUpdateBlocked {
  return {
    name,
    reason: "content_changed",
    suggested_local_name: `${name}.local`,
    error: null,
  };
}

describe("reconcileBlockedUpdates", () => {
  it("keeps unrelated checkouts queued after one checkout updates", () => {
    const result: ResolveSkillUpdateResult = {
      update: {
        skill: {
          name: "alpha",
          description: "",
          skill_type: "hub",
          stars: 0,
          installed: true,
          update_available: false,
          last_updated: "2026-08-05T00:00:00Z",
          git_url: "https://github.com/acme/a",
          tree_hash: "new",
          category: "None",
          author: null,
          topics: [],
        },
        siblings_cleared: ["beta"],
        agent_link_failures: [],
      },
      local_copy: null,
      remaining_blocked: [],
    };

    expect(reconcileBlockedUpdates([blocked("alpha"), blocked("gamma")], "alpha", result)).toEqual([blocked("gamma")]);
  });

  it("replaces the current checkout blockers without dropping unrelated ones", () => {
    const result: ResolveSkillUpdateResult = {
      update: null,
      local_copy: null,
      remaining_blocked: [blocked("beta")],
    };

    expect(reconcileBlockedUpdates([blocked("alpha"), blocked("beta"), blocked("gamma")], "alpha", result)).toEqual([
      blocked("beta"),
      blocked("gamma"),
    ]);
  });
});
