import { invoke } from "@tauri-apps/api/core";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { act, cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { RepoNewSkill, Skill, SkillMigrationReport, SkillUpdateReport } from "../../../types";
import { SkillsProvider } from "../hooks/useSkills";
import { LocalSkillsContent } from "./LocalSkillsContent";

vi.mock("../../../hooks/useAgentProfiles", () => ({
  useAgentProfiles: () => ({ profiles: [], deploySkillsToProject: vi.fn() }),
}));
vi.mock("../hooks/useSkillCards", () => ({
  useSkillCards: () => ({ groups: [], createGroup: vi.fn() }),
}));
vi.mock("../../../hooks/useAiStream", () => {
  const state = {
    content: null,
    visible: false,
    loading: false,
    error: null,
    aiConfigured: false,
    locale: "zh-CN",
    cancel: vi.fn(),
    hydrate: vi.fn(),
    setVisible: vi.fn(),
    setError: vi.fn(),
    dismiss: vi.fn(),
  };
  return { useAiStream: () => state };
});
vi.mock("../../../lib/toast", () => ({
  toast: { success: vi.fn(), error: vi.fn(), warning: vi.fn(), info: vi.fn() },
}));

const GHOST: RepoNewSkill = {
  repo_source: "acme/skills",
  repo_url: "https://github.com/acme/skills",
  skill_id: "next-skill",
  folder_path: "skills/next-skill",
  description: "Successor description",
  renamed_from: "old-skill",
};
const OLD: Skill = {
  name: "old-skill",
  description: "Original description",
  skill_type: "hub",
  stars: 0,
  installed: true,
  update_available: false,
  last_updated: "2026-08-01T00:00:00Z",
  git_url: GHOST.repo_url,
  tree_hash: "hash",
  category: "None",
  author: null,
  topics: [],
  source: GHOST.repo_source,
  upstream_change: {
    kind: "removed",
    suggested_local_name: "old-skill.local",
    successor: {
      skill_id: GHOST.skill_id,
      folder_path: GHOST.folder_path,
      description: GHOST.description,
      similarity: 99,
    },
  },
};
const UPDATABLE: Skill = {
  ...OLD,
  name: "updatable",
  description: "Before update",
  upstream_change: null,
  update_available: true,
};
const SUCCESSOR: Skill = { ...OLD, name: GHOST.skill_id, description: GHOST.description, upstream_change: null };

function deferred<T>() {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((done) => {
    resolve = done;
  });
  return { promise, resolve };
}

let library: Skill[];
let ghosts: RepoNewSkill[];
let client: QueryClient;
let migration: ReturnType<typeof deferred<SkillMigrationReport>>;
let update: ReturnType<typeof deferred<SkillUpdateReport>>;

beforeEach(() => {
  localStorage.clear();
  library = [UPDATABLE, OLD];
  ghosts = [GHOST];
  migration = deferred<SkillMigrationReport>();
  update = deferred<SkillUpdateReport>();
  client = new QueryClient({ defaultOptions: { queries: { retry: false, gcTime: Infinity } } });
  client.setQueryData(["skills"], library);
  client.setQueryData(["skills", "ghost"], ghosts);
  vi.mocked(invoke).mockReset();
  vi.mocked(invoke).mockImplementation(async (command) => {
    switch (command) {
      case "list_skills":
        return library;
      case "refresh_skill_updates":
        return library.map(({ name, update_available, upstream_change }) => ({
          name,
          update_available,
          upstream_change,
        }));
      case "check_new_repo_skills":
        return ghosts;
      case "migrate_renamed_skill":
        return migration.promise;
      case "update_skills":
        return update.promise;
      case "get_skill_detail_local":
        return { data: null, snapshot_status: "fresh" };
      case "get_storage_overview":
        return { broken_count: 0 };
      default:
        return [];
    }
  });
});

afterEach(() => {
  cleanup();
  client.clear();
});

function renderSkills() {
  return render(
    <QueryClientProvider client={client}>
      <SkillsProvider>
        <LocalSkillsContent scopeSwitch={<span>Local</span>} />
      </SkillsProvider>
    </QueryClientProvider>,
  );
}

function openCard(name: string) {
  fireEvent.click(screen.getByText(name, { selector: 'h3, [data-slot="card-title"]' }));
}

function drawer() {
  return within(screen.getByRole("complementary"));
}

async function finishUpdate() {
  const fresh = { ...UPDATABLE, description: "After update", update_available: false };
  library = [fresh, OLD];
  await act(async () =>
    update.resolve({
      updated: [{ skill: fresh, siblings_cleared: [], agent_link_failures: [] }],
      blocked: [],
      failed: [],
      skipped: [],
      channel_managed: [],
    }),
  );
  await waitFor(() => expect(screen.queryAllByText("After update")).not.toHaveLength(0));
}

describe("LocalSkillsContent selection and migration", () => {
  it("does not reopen a drawer closed while update-all was pending", async () => {
    renderSkills();
    openCard(UPDATABLE.name);
    fireEvent.click(screen.getByRole("button", { name: /全部更新/ }));
    fireEvent.click(drawer().getByRole("button", { name: "关闭" }));
    expect(screen.queryByRole("complementary")).not.toBeInTheDocument();
    await finishUpdate();
    expect(screen.queryByRole("complementary")).not.toBeInTheDocument();
  });

  it("keeps the ghost selected during update-all and migrates its original identity after reopen", async () => {
    renderSkills();
    openCard(UPDATABLE.name);
    fireEvent.click(screen.getByRole("button", { name: /全部更新/ }));
    openCard(GHOST.skill_id);
    await finishUpdate();
    expect(drawer().getByRole("heading", { name: GHOST.skill_id })).toBeInTheDocument();

    fireEvent.click(drawer().getByRole("button", { name: "关闭" }));
    openCard(GHOST.skill_id);
    fireEvent.click(drawer().getByRole("button", { name: "迁移到 next-skill" }));
    await waitFor(() => expect(drawer().getByRole("button", { name: "迁移中..." })).toBeDisabled());
    expect(vi.mocked(invoke).mock.calls.filter(([command]) => command === "migrate_renamed_skill")).toEqual([
      ["migrate_renamed_skill", { name: OLD.name }],
    ]);
  });

  it("refreshes the selected installed skill from the update result without requiring a reopen", async () => {
    renderSkills();
    openCard(UPDATABLE.name);
    fireEvent.click(screen.getByRole("button", { name: /全部更新/ }));
    await finishUpdate();
    expect(drawer().getByRole("heading", { name: UPDATABLE.name })).toBeInTheDocument();
    expect(drawer().getByText("After update")).toBeInTheDocument();
    expect(drawer().queryByText("Before update")).not.toBeInTheDocument();
  });

  it("keeps a ghost migration pending across drawer reopen and installs the successor once", async () => {
    renderSkills();
    openCard(GHOST.skill_id);
    fireEvent.click(drawer().getByRole("button", { name: "迁移到 next-skill" }));
    await waitFor(() => expect(drawer().getByRole("button", { name: "迁移中..." })).toBeDisabled());

    fireEvent.click(drawer().getByRole("button", { name: "关闭" }));
    openCard(GHOST.skill_id);
    const pending = drawer().getByRole("button", { name: "迁移中..." });
    expect(pending).toBeDisabled();
    fireEvent.click(pending);
    expect(vi.mocked(invoke).mock.calls.filter(([command]) => command === "migrate_renamed_skill")).toEqual([
      ["migrate_renamed_skill", { name: OLD.name }],
    ]);

    library = [UPDATABLE, SUCCESSOR];
    ghosts = [];
    await act(async () =>
      migration.resolve({
        installed: SUCCESSOR.name,
        removed: OLD.name,
        agents_relinked: [],
        agent_failures: [],
        projects_relinked: [],
        project_failures: [],
        removal_failure: null,
      }),
    );
    await waitFor(() => expect(drawer().getByRole("button", { name: "卸载" })).toBeEnabled());
    expect(drawer().getByRole("heading", { name: GHOST.skill_id })).toBeInTheDocument();
    expect(drawer().queryByRole("button", { name: "迁移到 next-skill" })).not.toBeInTheDocument();
  });
});
