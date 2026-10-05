import { invoke } from "@tauri-apps/api/core";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { act, cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { Skill, SkillUpdateReport } from "../../../types";
import { SkillsProvider } from "../hooks/useSkills";
import { LocalSkillsContent } from "./LocalSkillsContent";

vi.mock("../../../hooks/useAgentProfiles", () => ({
  useAgentProfiles: () => ({ profiles: [], deploySkillsToProject: vi.fn() }),
}));
vi.mock("../hooks/useSkillCards", () => ({
  useSkillCards: () => ({ groups: [], createGroup: vi.fn() }),
}));
vi.mock("../../../lib/toast", () => ({
  toast: { success: vi.fn(), error: vi.fn(), warning: vi.fn(), info: vi.fn() },
}));

const REMOVED: Skill = {
  name: "old-skill",
  description: "Original description",
  skill_type: "hub",
  stars: 0,
  installed: true,
  update_available: false,
  last_updated: "2026-08-01T00:00:00Z",
  git_url: "https://github.com/acme/skills",
  tree_hash: "hash",
  category: "None",
  author: null,
  topics: [],
  source: "acme/skills",
  upstream_change: { kind: "removed", suggested_local_name: "old-skill.local", successor: null },
};
const UPDATABLE: Skill = {
  ...REMOVED,
  name: "updatable",
  description: "Before update",
  upstream_change: null,
  update_available: true,
};

function deferred<T>() {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((done) => {
    resolve = done;
  });
  return { promise, resolve };
}

let library: Skill[];
let client: QueryClient;
let update: ReturnType<typeof deferred<SkillUpdateReport>>;

beforeEach(() => {
  localStorage.clear();
  library = [UPDATABLE, REMOVED];
  update = deferred<SkillUpdateReport>();
  client = new QueryClient({ defaultOptions: { queries: { retry: false, gcTime: Infinity } } });
  client.setQueryData(["skills"], library);
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
  library = [fresh, REMOVED];
  await act(async () =>
    update.resolve({
      updated: [{ skill: fresh, siblings_cleared: [], agent_link_failures: [] }],
      failed: [],
      skipped: [],
      channel_managed: [],
    }),
  );
  await waitFor(() => expect(screen.queryAllByText("After update")).not.toHaveLength(0));
}

describe("LocalSkillsContent selection and updates", () => {
  it("does not reopen a drawer closed while update-all was pending", async () => {
    renderSkills();
    openCard(UPDATABLE.name);
    fireEvent.click(screen.getByRole("button", { name: /全部更新/ }));
    fireEvent.click(drawer().getByRole("button", { name: "关闭" }));
    expect(screen.queryByRole("complementary")).not.toBeInTheDocument();
    await finishUpdate();
    expect(screen.queryByRole("complementary")).not.toBeInTheDocument();
  });

  it("refreshes the selected installed skill from the update result without requiring a reopen", async () => {
    renderSkills();
    openCard(UPDATABLE.name);
    fireEvent.click(screen.getByRole("button", { name: /全部更新/ }));
    await finishUpdate();
    expect(drawer().getByRole("heading", { name: UPDATABLE.name })).toBeInTheDocument();
    expect(drawer().getByRole("button", { name: "重新安装" })).toBeInTheDocument();
    expect(drawer().getByText("After update")).toBeInTheDocument();
    expect(drawer().queryByText("Before update")).not.toBeInTheDocument();
  });

  it("lists upstream-removed names in a toast when update-all skips them", async () => {
    const { toast } = await import("../../../lib/toast");
    renderSkills();
    fireEvent.click(screen.getByRole("button", { name: /全部更新/ }));

    const fresh = { ...UPDATABLE, description: "After update", update_available: false };
    library = [fresh];
    await act(async () =>
      update.resolve({
        updated: [{ skill: fresh, siblings_cleared: [], agent_link_failures: [] }],
        failed: [],
        skipped: ["old-skill"],
        channel_managed: [],
      }),
    );

    await waitFor(() => expect(toast.info).toHaveBeenCalledWith(expect.stringContaining("old-skill")));
  });
});
