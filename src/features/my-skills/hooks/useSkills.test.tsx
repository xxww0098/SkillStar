import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { invoke } from "@tauri-apps/api/core";
import { act, fireEvent, renderHook, screen, waitFor } from "@testing-library/react";
import type { ReactNode } from "react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { Skill, SkillUpdateReport } from "../../../types";
import { toast } from "../../../lib/toast";
import { SkillsProvider, useSkills } from "./useSkills";

vi.mock("../../../lib/toast", () => ({
  toast: {
    error: vi.fn(),
    warning: vi.fn(),
  },
}));

const mockedInvoke = vi.mocked(invoke);
let defaultUpdateApplied = false;

const INITIAL_SKILLS: Skill[] = [
  {
    name: "opencli-repair",
    description: "Repair adapters",
    skill_type: "hub",
    stars: 0,
    installed: true,
    update_available: true,
    last_updated: "2026-01-01T00:00:00.000Z",
    git_url: "https://github.com/jackwener/opencli.git",
    tree_hash: "hash-a",
    category: "None",
    author: null,
    topics: [],
    agent_links: [],
    rank: undefined,
    source: "jackwener/opencli",
  },
  {
    name: "opencli-search",
    description: "Search adapters",
    skill_type: "hub",
    stars: 0,
    installed: true,
    update_available: true,
    last_updated: "2026-01-01T00:00:00.000Z",
    git_url: "https://github.com/jackwener/opencli.git",
    tree_hash: "hash-b",
    category: "None",
    author: null,
    topics: [],
    agent_links: [],
    rank: undefined,
    source: "jackwener/opencli",
  },
  {
    name: "opencli-usage",
    description: "Usage adapters",
    skill_type: "hub",
    stars: 0,
    installed: true,
    update_available: true,
    last_updated: "2026-01-01T00:00:00.000Z",
    git_url: "https://github.com/jackwener/opencli.git",
    tree_hash: "hash-c",
    category: "None",
    author: null,
    topics: [],
    agent_links: [],
    rank: undefined,
    source: "jackwener/opencli",
  },
];

function createWrapper() {
  const queryClient = new QueryClient({
    defaultOptions: {
      queries: { retry: false },
      mutations: { retry: false },
    },
  });

  return function Wrapper({ children }: { children: ReactNode }) {
    return (
      <QueryClientProvider client={queryClient}>
        <SkillsProvider>{children}</SkillsProvider>
      </QueryClientProvider>
    );
  };
}

describe("useSkills", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    defaultUpdateApplied = false;

    mockedInvoke.mockImplementation(async (command, args) => {
      switch (command) {
        case "list_skills":
          return defaultUpdateApplied
            ? INITIAL_SKILLS.map((skill, index) => ({
                ...skill,
                description: index === 1 ? "Search adapters after shared pull" : skill.description,
                update_available: false,
              }))
            : INITIAL_SKILLS;
        case "refresh_skill_updates":
          return [];
        case "check_new_repo_skills":
          return [];
        case "migrate_local_skills":
          return 0;
        case "update_skills": {
          // The backend collapses the repo down to one pull and reports the
          // rest as skipped — the UI does not group by git_url itself.
          expect(args).toEqual({ names: INITIAL_SKILLS.map((skill) => skill.name) });

          defaultUpdateApplied = true;
          const report: SkillUpdateReport = {
            updated: [
              {
                skill: {
                  ...INITIAL_SKILLS[0],
                  update_available: false,
                  last_updated: "2026-04-08T08:00:00.000Z",
                },
                siblings_cleared: [],
                agent_link_failures: [],
              },
            ],
            blocked: [],
            failed: [],
            skipped: INITIAL_SKILLS.slice(1).map((skill) => skill.name),
            channel_managed: [],
          };
          return report;
        }
        default:
          return undefined;
      }
    });
  });

  it("clears every card the backend reports as moved after update-all", async () => {
    const { result } = renderHook(() => useSkills(), { wrapper: createWrapper() });

    await waitFor(() => {
      expect(result.current.loading).toBe(false);
    });

    expect(result.current.skills).toHaveLength(3);
    expect(result.current.skills.every((skill) => skill.update_available)).toBe(true);

    await act(async () => {
      await result.current.updateSkills(INITIAL_SKILLS.map((skill) => skill.name));
    });

    await waitFor(() => {
      expect(result.current.skills.every((skill) => !skill.update_available)).toBe(true);
    });
    expect(result.current.skills[1].description).toBe("Search adapters after shared pull");
  });

  it("reinstalls every discovered skill from the requested repository only", async () => {
    const source = "jackwener/opencli";
    const sourceUrl = "https://github.com/jackwener/opencli.git";
    const targets = INITIAL_SKILLS.map((skill) => ({
      id: skill.name,
      folder_path: `skills/${skill.name}`,
      description: skill.description,
      already_installed: true,
    }));

    mockedInvoke.mockImplementation(async (command, args) => {
      switch (command) {
        case "list_skills":
          return INITIAL_SKILLS;
        case "refresh_skill_updates":
        case "check_new_repo_skills":
          return [];
        case "migrate_local_skills":
          return 0;
        case "scan_github_repo":
          expect(args).toEqual({ url: sourceUrl, fullDepth: true });
          return { source, source_url: sourceUrl, skills: targets };
        case "install_from_scan":
          expect(args).toEqual({
            repoUrl: sourceUrl,
            source,
            skills: targets.map(({ id, folder_path }) => ({ id, folder_path })),
          });
          return INITIAL_SKILLS.map((skill) => skill.name);
        default:
          return undefined;
      }
    });

    const { result } = renderHook(() => useSkills(), { wrapper: createWrapper() });
    await waitFor(() => expect(result.current.loading).toBe(false));

    let installed: string[] | undefined;
    await act(async () => {
      installed = await result.current.reinstallRepoSkills(sourceUrl);
    });

    expect(installed).toEqual(INITIAL_SKILLS.map((skill) => skill.name));
  });

  it("keeps a divergent card unchanged until the user resolves the blocked update", async () => {
    mockedInvoke.mockImplementation(async (command) => {
      switch (command) {
        case "list_skills":
          return INITIAL_SKILLS;
        case "refresh_skill_updates":
        case "check_new_repo_skills":
          return [];
        case "migrate_local_skills":
          return 0;
        case "update_skills":
          return {
            updated: [],
            blocked: [
              {
                name: "opencli-repair",
                reason: "content_changed",
                suggested_local_name: "opencli-repair.local",
                error: null,
              },
            ],
            failed: [],
            skipped: [],
          };
        case "resolve_skill_update":
          return {
            update: {
              skill: {
                ...INITIAL_SKILLS[0],
                update_available: false,
                last_updated: "2026-04-08T09:00:00.000Z",
              },
              siblings_cleared: [],
              agent_link_failures: [],
            },
            local_copy: { ...INITIAL_SKILLS[0], name: "opencli-repair.local", skill_type: "local" },

            uninstalled: [],
            remaining_blocked: [],
          };
        default:
          return undefined;
      }
    });

    const { result } = renderHook(() => useSkills(), { wrapper: createWrapper() });
    await waitFor(() => expect(result.current.loading).toBe(false));

    let report: SkillUpdateReport | undefined;
    await act(async () => {
      report = await result.current.updateSkills(["opencli-repair"]);
    });
    expect(report?.blocked[0]?.suggested_local_name).toBe("opencli-repair.local");
    expect(result.current.skills[0].update_available).toBe(true);

    await act(async () => {
      await result.current.resolveSkillUpdate("opencli-repair", {
        kind: "preserve",
        local_name: "opencli-repair.local",
      });
    });

    expect(mockedInvoke).toHaveBeenCalledWith("resolve_skill_update", {
      name: "opencli-repair",
      resolution: { kind: "preserve", local_name: "opencli-repair.local" },
    });
    expect(result.current.skills[0].update_available).toBe(false);

    let updatePromise!: Promise<Skill>;
    act(() => {
      updatePromise = result.current.updateSkill("opencli-repair");
    });
    await screen.findByRole("alertdialog");
    fireEvent.click(screen.getByText("丢弃本地修改"));
    fireEvent.click(screen.getByRole("button", { name: "丢弃修改并更新" }));
    await act(async () => {
      await updatePromise;
    });
    expect(mockedInvoke).toHaveBeenCalledWith("resolve_skill_update", {
      name: "opencli-repair",
      resolution: { kind: "discard" },
    });
  });

  it("shows every divergent Skill once and applies one batch decision", async () => {
    const blocked = [
      {
        name: "opencli-search",
        reason: "content_changed" as const,
        suggested_local_name: "opencli-search.local",
        error: null,
      },
      {
        name: "opencli-usage",
        reason: "content_changed" as const,
        suggested_local_name: "opencli-usage.local",
        error: null,
      },
    ];

    mockedInvoke.mockImplementation(async (command, args) => {
      switch (command) {
        case "list_skills":
          return INITIAL_SKILLS;
        case "refresh_skill_updates":
        case "check_new_repo_skills":
          return [];
        case "migrate_local_skills":
          return 0;
        case "update_skills":
          return { updated: [], blocked, failed: [], skipped: [] };
        case "resolve_skill_update": {
          const name = (args as { name: string }).name;
          return name === "opencli-search"
            ? { update: null, local_copy: null, uninstalled: [], remaining_blocked: [blocked[1]] }
            : {
                update: {
                  skill: { ...INITIAL_SKILLS[2], update_available: false },
                  siblings_cleared: ["opencli-repair", "opencli-search"],
                  agent_link_failures: [],
                },
                local_copy: null,
                uninstalled: [],
                remaining_blocked: [],
              };
        }
        default:
          return undefined;
      }
    });

    const { result } = renderHook(() => useSkills(), { wrapper: createWrapper() });
    await waitFor(() => expect(result.current.loading).toBe(false));

    let updatePromise!: Promise<Skill>;
    act(() => {
      updatePromise = result.current.updateSkill("opencli-repair");
    });

    await screen.findByRole("alertdialog");
    expect(screen.getByText("opencli-search")).toBeInTheDocument();
    expect(screen.getByText("opencli-usage")).toBeInTheDocument();
    expect(screen.getAllByRole("alertdialog")).toHaveLength(1);

    fireEvent.click(screen.getByText("丢弃以下 2 个 Skill 的本地修改"));
    fireEvent.click(screen.getByRole("button", { name: "全部丢弃修改并更新" }));
    await act(async () => {
      await updatePromise;
    });

    await waitFor(() => expect(screen.queryByRole("alertdialog")).not.toBeInTheDocument());
    expect(mockedInvoke).toHaveBeenCalledWith("resolve_skill_update", {
      name: "opencli-search",
      resolution: { kind: "discard" },
    });
    expect(mockedInvoke).toHaveBeenCalledWith("resolve_skill_update", {
      name: "opencli-usage",
      resolution: { kind: "discard" },
    });
  });

  it("keeps the queue and the typed names when one resolution fails, then retries", async () => {
    const blocked = [
      {
        name: "opencli-search",
        reason: "content_changed" as const,
        suggested_local_name: "opencli-search.local",
        error: null,
      },
      {
        name: "opencli-usage",
        reason: "content_changed" as const,
        suggested_local_name: "opencli-usage.local",
        error: null,
      },
    ];
    let searchAttempts = 0;

    mockedInvoke.mockImplementation(async (command, args) => {
      switch (command) {
        case "list_skills":
          return INITIAL_SKILLS;
        case "refresh_skill_updates":
        case "check_new_repo_skills":
          return [];
        case "migrate_local_skills":
          return 0;
        case "update_skills":
          return { updated: [], blocked, failed: [], skipped: [] };
        case "resolve_skill_update": {
          const name = (args as { name: string }).name;
          if (name === "opencli-search") {
            searchAttempts += 1;
            if (searchAttempts === 1) throw new Error("disk full");
            return { update: null, local_copy: null, uninstalled: [], remaining_blocked: [blocked[1]] };
          }
          return {
            update: {
              skill: { ...INITIAL_SKILLS[2], update_available: false },
              siblings_cleared: ["opencli-repair", "opencli-search"],
              agent_link_failures: [],
            },
            local_copy: null,
            uninstalled: [],
            remaining_blocked: [],
          };
        }
        default:
          return undefined;
      }
    });

    const { result } = renderHook(() => useSkills(), { wrapper: createWrapper() });
    await waitFor(() => expect(result.current.loading).toBe(false));

    let updatePromise!: Promise<Skill>;
    act(() => {
      updatePromise = result.current.updateSkill("opencli-repair");
    });

    await screen.findByRole("alertdialog");
    fireEvent.change(screen.getByLabelText("本地副本名称", { selector: "#local-divergence-copy-name-1" }), {
      target: { value: "usage-copy" },
    });
    fireEvent.click(screen.getByRole("button", { name: "全部保留副本并更新" }));

    // The failure re-opens the same queue with the reason, and the name the
    // user chose for the *other* Skill survives the round trip.
    await screen.findByText("disk full");
    expect(screen.getByText("opencli-search")).toBeInTheDocument();
    expect(screen.getByLabelText("本地副本名称", { selector: "#local-divergence-copy-name-1" })).toHaveValue(
      "usage-copy",
    );

    fireEvent.click(screen.getByRole("button", { name: "全部保留副本并更新" }));
    await act(async () => {
      await updatePromise;
    });

    await waitFor(() => expect(screen.queryByRole("alertdialog")).not.toBeInTheDocument());
    expect(mockedInvoke).toHaveBeenCalledWith("resolve_skill_update", {
      name: "opencli-usage",
      resolution: { kind: "preserve", local_name: "usage-copy" },
    });
  });

  it("stops instead of re-sending the same resolution when a Skill stays blocked", async () => {
    const blocked = [
      {
        name: "opencli-search",
        reason: "content_changed" as const,
        suggested_local_name: "opencli-search.local",
        error: null,
      },
    ];

    mockedInvoke.mockImplementation(async (command) => {
      switch (command) {
        case "list_skills":
          return INITIAL_SKILLS;
        case "refresh_skill_updates":
        case "check_new_repo_skills":
          return [];
        case "migrate_local_skills":
          return 0;
        case "update_skills":
          return { updated: [], blocked, failed: [], skipped: [] };
        case "resolve_skill_update":
          // A backend that never clears the blocker used to spin forever here.
          return { update: null, local_copy: null, uninstalled: [], remaining_blocked: blocked };
        default:
          return undefined;
      }
    });

    const { result } = renderHook(() => useSkills(), { wrapper: createWrapper() });
    await waitFor(() => expect(result.current.loading).toBe(false));

    act(() => {
      void result.current.updateSkill("opencli-repair").catch(() => {});
    });

    await screen.findByRole("alertdialog");
    fireEvent.click(screen.getByText("丢弃本地修改"));
    fireEvent.click(screen.getByRole("button", { name: "丢弃修改并更新" }));

    await screen.findByText(/opencli-search/);
    await waitFor(() =>
      expect(mockedInvoke.mock.calls.filter(([command]) => command === "resolve_skill_update")).toHaveLength(1),
    );
  });

  it("surfaces update failures for every page using the shared hook", async () => {
    mockedInvoke.mockImplementation(async (command) => {
      switch (command) {
        case "list_skills":
          return INITIAL_SKILLS;
        case "refresh_skill_updates":
        case "check_new_repo_skills":
          return [];
        case "migrate_local_skills":
          return 0;
        case "update_skills":
          return {
            updated: [],
            blocked: [],
            failed: [{ name: "opencli-repair", error: "remote authentication failed" }],
            skipped: [],
          };
        default:
          return undefined;
      }
    });

    const { result } = renderHook(() => useSkills(), { wrapper: createWrapper() });
    await waitFor(() => expect(result.current.loading).toBe(false));

    await expect(result.current.updateSkill("opencli-repair")).rejects.toThrow("remote authentication failed");
    expect(toast.error).toHaveBeenCalledWith("remote authentication failed");
  });

  it("returns a freshly listed requested sibling after resolving the checkout blocker", async () => {
    let resolved = false;
    const refreshedSkills = INITIAL_SKILLS.map((skill) =>
      skill.name === "opencli-repair"
        ? { ...skill, description: "Updated repair", tree_hash: "hash-after-pull", update_available: false }
        : { ...skill, update_available: false },
    );
    mockedInvoke.mockImplementation(async (command) => {
      switch (command) {
        case "list_skills":
          return resolved ? refreshedSkills : INITIAL_SKILLS;
        case "refresh_skill_updates":
        case "check_new_repo_skills":
          return [];
        case "migrate_local_skills":
          return 0;
        case "update_skills":
          return {
            updated: [],
            blocked: [
              {
                name: "opencli-search",
                reason: "content_changed",
                suggested_local_name: "opencli-search.local",
                error: null,
              },
            ],
            failed: [],
            skipped: [],
          };
        case "resolve_skill_update":
          resolved = true;
          return {
            update: {
              skill: { ...refreshedSkills[1] },
              siblings_cleared: ["opencli-repair", "opencli-usage"],
              agent_link_failures: [],
            },
            local_copy: null,
            uninstalled: [],
            remaining_blocked: [],
          };
        default:
          return undefined;
      }
    });

    const { result } = renderHook(() => useSkills(), { wrapper: createWrapper() });
    await waitFor(() => expect(result.current.loading).toBe(false));

    let updatePromise!: Promise<Skill>;
    act(() => {
      updatePromise = result.current.updateSkill("opencli-repair");
    });
    await screen.findByRole("alertdialog");
    fireEvent.click(screen.getByText("丢弃本地修改"));
    fireEvent.click(screen.getByRole("button", { name: "丢弃修改并更新" }));

    let updated!: Skill;
    await act(async () => {
      updated = await updatePromise;
    });
    expect(updated.description).toBe("Updated repair");
    expect(updated.tree_hash).toBe("hash-after-pull");
  });

  it("asks about local edits and dropped Skills in separate dialogs", async () => {
    const blocked = [
      {
        name: "opencli-search",
        reason: "content_changed" as const,
        suggested_local_name: "opencli-search.local",
        error: null,
      },
      {
        name: "opencli-usage",
        reason: "source_removed" as const,
        suggested_local_name: "opencli-usage.local",
        error: null,
      },
    ];

    // The hub stops listing a Skill it removed, so the mock must too.
    let installed = INITIAL_SKILLS;

    mockedInvoke.mockImplementation(async (command, args) => {
      switch (command) {
        case "list_skills":
          return installed;
        case "refresh_skill_updates":
        case "check_new_repo_skills":
          return [];
        case "migrate_local_skills":
          return 0;
        case "update_skills":
          return { updated: [], blocked, failed: [], skipped: [] };
        case "resolve_skill_update": {
          const name = (args as { name: string }).name;
          if (name === "opencli-search") {
            return { update: null, local_copy: null, uninstalled: [], remaining_blocked: [blocked[1]] };
          }
          installed = installed.filter((skill) => skill.name !== "opencli-usage");
          return {
            update: {
              skill: { ...INITIAL_SKILLS[0], update_available: false },
              siblings_cleared: ["opencli-search"],
              agent_link_failures: [],
            },
            local_copy: null,
            uninstalled: ["opencli-usage"],
            remaining_blocked: [],
          };
        }
        default:
          return undefined;
      }
    });

    const { result } = renderHook(() => useSkills(), { wrapper: createWrapper() });
    await waitFor(() => expect(result.current.loading).toBe(false));

    let runPromise!: Promise<{ uninstalled: string[] }>;
    act(() => {
      runPromise = result.current.runSkillUpdate(["opencli-repair"]);
    });

    // The local edit is asked about on its own: a dialog holding both would
    // offer "discard", which cannot work for a Skill with no source left.
    await screen.findByRole("alertdialog");
    expect(screen.queryByText("opencli-usage")).not.toBeInTheDocument();
    fireEvent.click(screen.getByText("丢弃本地修改"));
    fireEvent.click(screen.getByRole("button", { name: "丢弃修改并更新" }));

    await screen.findByText("彻底移除该 Skill");
    expect(screen.getByRole("heading", { level: 2 })).toHaveTextContent("来源已不再提供");
    fireEvent.click(screen.getByText("彻底移除该 Skill"));
    fireEvent.click(screen.getByRole("button", { name: "移除该技能" }));

    let report!: { uninstalled: string[] };
    await act(async () => {
      report = await runPromise;
    });

    expect(report.uninstalled).toEqual(["opencli-usage"]);
    expect(mockedInvoke).toHaveBeenCalledWith("resolve_skill_update", {
      name: "opencli-usage",
      resolution: { kind: "uninstall" },
    });
    await waitFor(() => expect(result.current.skills.some((skill) => skill.name === "opencli-usage")).toBe(false));
  });
});
