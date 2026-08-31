import { fireEvent, render, screen } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { AgentProfile, Skill } from "../../../types";

const installSkill = vi.fn();
const toggleSkillForAgent = vi.fn();

const RUST: Skill = {
  name: "rust",
  description: "Rust skills",
  skill_type: "hub",
  stars: 0,
  installed: true,
  update_available: false,
  last_updated: "2026-08-01T00:00:00Z",
  git_url: "https://github.com/acme/rust-skills",
  tree_hash: "hash",
  category: "None",
  author: "acme",
  topics: [],
  source: "acme/rust-skills",
  agent_links: ["Cursor"],
};

const CURSOR: AgentProfile = {
  id: "cursor",
  display_name: "Cursor",
  icon: "cursor",
  global_skills_dir: "~/.cursor/skills",
  project_skills_rel: ".agents/skills",
  installed: true,
  enabled: true,
  synced_count: 0,
};

const DEEPSEEK: AgentProfile = {
  ...CURSOR,
  id: "deepseek",
  display_name: "DeepSeek Harness",
  icon: "deepseek",
  global_skills_dir: "~/.dsh/skills",
  project_skills_rel: ".dsh/skills",
};

vi.mock("../hooks/useSkills", () => ({
  useSkills: () => ({
    skills: [RUST],
    loading: false,
    refresh: vi.fn(),
    installSkill,
    reinstallRepoSkills: vi.fn(),
    uninstallSkill: vi.fn(),
    runSkillUpdate: vi.fn(),
    resolveRemovedSkill: vi.fn(),
    migrateRenamedSkill: vi.fn(),
    pendingMigrationNames: new Set(),
    pendingUpdateNames: new Set(),
    toggleSkillForAgent,
    pendingAgentToggleKeys: new Set(),
    readSkillContent: vi.fn(),
    updateSkillContent: vi.fn(),
    batchRemoveSkillsFromAllAgents: vi.fn(),
    ghostSkills: [],
    dismissGhostSkill: vi.fn(),
    dismissGhostRepo: vi.fn(),
    installGhostSkill: vi.fn(),
  }),
}));

vi.mock("../../../hooks/useAgentProfiles", () => ({
  useAgentProfiles: () => ({
    profiles: [CURSOR, DEEPSEEK],
    deploySkillsToProject: vi.fn(),
  }),
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

vi.mock("./SkillGrid", () => ({
  SkillGrid: ({
    onInstall,
  }: {
    onInstall: (url: string, name: string, agentId?: string) => void;
  }) => (
    <div>
      <button type="button" onClick={() => onInstall(RUST.git_url, RUST.name, "deepseek")}>
        carousel-deepseek
      </button>
      <button type="button" onClick={() => onInstall(RUST.git_url, RUST.name, "cursor")}>
        carousel-cursor
      </button>
    </div>
  ),
}));

import { LocalSkillsContent } from "./LocalSkillsContent";

describe("LocalSkillsContent install forwarding", () => {
  beforeEach(() => {
    installSkill.mockReset();
    toggleSkillForAgent.mockReset();
    installSkill.mockResolvedValue(RUST);
  });

  it("forwards carousel agentId to installSkill so a second harness is not a no-op", async () => {
    render(<LocalSkillsContent scopeSwitch={<span>scope</span>} />);

    fireEvent.click(screen.getByText("carousel-deepseek"));
    expect(installSkill).toHaveBeenCalledWith(RUST.git_url, "rust", "deepseek");
    expect(toggleSkillForAgent).not.toHaveBeenCalled();

    fireEvent.click(screen.getByText("carousel-cursor"));
    expect(installSkill).toHaveBeenCalledWith(RUST.git_url, "rust", "cursor");
    expect(installSkill).toHaveBeenCalledTimes(2);
  });
});
