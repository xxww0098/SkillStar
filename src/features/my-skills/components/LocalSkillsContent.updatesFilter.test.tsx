import { fireEvent, render, screen } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { Skill } from "../../../types";

/** Regression harness for the "needs attention" redesign: the sidebar badge,
 *  the toolbar chip and the updates filter must all read the same predicate
 *  (content updates + removed upstreams), so the number the badge promises is
 *  exactly what the filter shows. */

let mockSkills: Skill[] = [];

vi.mock("../../../lib/toast", () => ({
  toast: {
    error: vi.fn(),
    success: vi.fn(),
    info: vi.fn(),
    warning: vi.fn(),
  },
}));

vi.mock("../hooks/useSkills", () => ({
  useSkills: () => ({
    skills: mockSkills,
    loading: false,
    refresh: vi.fn(),
    installSkill: vi.fn(),
    reinstallSkill: vi.fn(),
    reinstallRepoSkills: vi.fn(),
    uninstallSkill: vi.fn(),
    runSkillUpdate: vi.fn(),
    pendingUpdateNames: new Set<string>(),
    toggleSkillForAgent: vi.fn(),
    pendingAgentToggleKeys: new Set<string>(),
    readSkillContent: vi.fn(),
    updateSkillContent: vi.fn(),
    batchRemoveSkillsFromAllAgents: vi.fn(),
  }),
  useSkillBadgeCounts: () => ({
    pendingUpdatesCount: mockSkills.filter(
      (skill) => skill.skill_type !== "local" && (skill.update_available || skill.upstream_change),
    ).length,
  }),
}));

vi.mock("../../../hooks/useAgentProfiles", () => ({
  useAgentProfiles: () => ({ profiles: [], deploySkillsToProject: vi.fn() }),
}));

vi.mock("../hooks/useSkillCards", () => ({
  useSkillCards: () => ({ createGroup: vi.fn(), groups: [] }),
}));

vi.mock("../../../hooks/useViewMode", () => ({
  useViewMode: () => ["grid", vi.fn()],
}));

vi.mock("../../../hooks/useSkillsSelectionShortcuts", () => ({
  useSkillsSelectionShortcuts: () => undefined,
}));

vi.mock("../../../lib/ipc", () => ({
  tauriInvoke: vi.fn(async () => ({ broken_count: 0 })),
}));

let renderedSkills: Skill[] = [];
let renderedEmptyMessage: string | null = null;

vi.mock("./SkillGrid", () => ({
  SkillGrid: ({ skills, emptyMessage }: { skills: Skill[]; emptyMessage?: string }) => {
    renderedSkills = skills;
    renderedEmptyMessage = emptyMessage ?? null;
    return (
      <div>
        <div data-testid="grid-count">{skills.length}</div>
        {skills.map((skill) => (
          <span key={skill.name}>{skill.name}</span>
        ))}
        {skills.length === 0 && emptyMessage ? <div data-testid="grid-empty">{emptyMessage}</div> : null}
      </div>
    );
  },
}));

import { LocalSkillsContent } from "./LocalSkillsContent";

const baseSkill: Skill = {
  name: "steady",
  description: "No changes upstream",
  skill_type: "hub",
  stars: 0,
  installed: true,
  update_available: false,
  last_updated: "2026-08-01T00:00:00Z",
  git_url: "https://github.com/acme/skills",
  tree_hash: "hash",
  category: "None",
  author: "acme",
  topics: [],
};

/** The exact shape that produced the reported bug: a removed skill with no
 *  content update — badge counts it, the old filter did not. */
const removedSkill: Skill = {
  ...baseSkill,
  name: "renamed",
  upstream_change: { kind: "removed", suggested_local_name: "renamed.local", successor: null },
};

describe("LocalSkillsContent attention filter", () => {
  beforeEach(() => {
    mockSkills = [baseSkill, removedSkill];
    renderedSkills = [];
    renderedEmptyMessage = null;
  });

  it("counts removed skills on the chip even when nothing is content-updatable", () => {
    render(<LocalSkillsContent scopeSwitch={<span>scope</span>} />);

    const chip = screen.getByRole("button", { name: /需处理 \(1\)/i });
    expect(chip).toBeInTheDocument();
    // Nothing is content-updatable, so the update-all CTA must not render.
    expect(screen.queryByRole("button", { name: /全部更新/i })).not.toBeInTheDocument();

    fireEvent.click(chip);

    // The filter delivers exactly what the chip promised: the removed skill.
    expect(renderedSkills.map((skill) => skill.name)).toEqual(["renamed"]);
  });

  it("keeps update-only skills inside the attention filter alongside upstream changes", () => {
    mockSkills = [baseSkill, removedSkill, { ...baseSkill, name: "stale", update_available: true }];

    render(<LocalSkillsContent scopeSwitch={<span>scope</span>} />);
    expect(screen.getByRole("button", { name: /需处理 \(2\)/i })).toBeInTheDocument();

    // The CTA appears the moment a content update exists.
    expect(screen.getByRole("button", { name: /全部更新/i })).toBeInTheDocument();

    fireEvent.click(screen.getByRole("button", { name: /需处理 \(2\)/i }));
    expect(renderedSkills.map((skill) => skill.name).sort()).toEqual(["renamed", "stale"]);
  });

  it("explains when attention exists only outside the active filters", () => {
    render(<LocalSkillsContent scopeSwitch={<span>scope</span>} />);

    const search = screen.getByPlaceholderText(/搜索技能/i);
    fireEvent.change(search, { target: { value: "steady" } });

    // Search hid the removed skill, so the chip must say 0 — not lie about 1.
    const chip = screen.getByRole("button", { name: /需处理 \(0\)/i });
    fireEvent.click(chip);

    expect(renderedSkills).toHaveLength(0);
    expect(renderedEmptyMessage).toContain("1");
  });
});
