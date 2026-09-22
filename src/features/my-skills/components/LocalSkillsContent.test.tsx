import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { AgentProfile, RepoNewSkill, Skill } from "../../../types";

const installSkill = vi.fn();
const installGhostSkill = vi.fn();
const toggleSkillForAgent = vi.fn();

vi.mock("../../../lib/toast", () => ({
  toast: {
    error: vi.fn(),
    success: vi.fn(),
    info: vi.fn(),
    warning: vi.fn(),
  },
}));

const GHOST: RepoNewSkill = {
  repo_source: "acme/ghost-skills",
  repo_url: "https://github.com/acme/ghost-skills",
  skill_id: "ghost-skill",
  folder_path: "skills/ghost-skill",
  description: "A ghost skill",
};

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
    reinstallSkill: vi.fn(),
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
    ghostSkills: [GHOST],
    dismissGhostSkill: vi.fn(),
    dismissGhostRepo: vi.fn(),
    installGhostSkill,
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
    onInstallGhost,
  }: {
    onInstall: (url: string, name: string, agentId?: string) => void;
    onInstallGhost?: (skill: RepoNewSkill) => Promise<unknown>;
  }) => (
    <div>
      <button type="button" onClick={() => onInstall(RUST.git_url, RUST.name, "deepseek")}>
        carousel-deepseek
      </button>
      <button type="button" onClick={() => onInstall(RUST.git_url, RUST.name, "cursor")}>
        carousel-cursor
      </button>
      <button type="button" onClick={() => void onInstallGhost?.(GHOST).catch(() => {})}>
        ghost-install
      </button>
    </div>
  ),
}));

import { LocalSkillsContent } from "./LocalSkillsContent";

describe("LocalSkillsContent install forwarding", () => {
  beforeEach(() => {
    installSkill.mockReset();
    installGhostSkill.mockReset();
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

  it("shows an error toast when a ghost skill install fails", async () => {
    installGhostSkill.mockRejectedValue(new Error("network down"));
    const { toast } = await import("../../../lib/toast");
    render(<LocalSkillsContent scopeSwitch={<span>scope</span>} />);

    fireEvent.click(screen.getByText("ghost-install"));

    await waitFor(() => expect(toast.error).toHaveBeenCalled());
    expect(toast.error).toHaveBeenCalledWith(expect.stringContaining("Error: network down"));
  });
});
