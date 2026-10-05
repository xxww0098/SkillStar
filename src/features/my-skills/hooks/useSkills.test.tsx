import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { invoke } from "@tauri-apps/api/core";
import { act, renderHook, waitFor } from "@testing-library/react";
import type { ReactNode } from "react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { Skill, SkillUpdateReport } from "../../../types";
import { toast } from "../../../lib/toast";
import { SkillsProvider, useSkillBadgeCounts, useSkills } from "./useSkills";

vi.mock("../../../lib/toast", () => ({
  toast: {
    error: vi.fn(),
    warning: vi.fn(),
    info: vi.fn(),
    success: vi.fn(),
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
        case "migrate_local_skills":
          return 0;
        case "update_skills": {
          // The backend reports one UpdateResult per requested name.
          expect(args).toEqual({ names: INITIAL_SKILLS.map((skill) => skill.name) });

          defaultUpdateApplied = true;
          const report: SkillUpdateReport = {
            updated: INITIAL_SKILLS.map((skill, index) => ({
              skill: {
                ...skill,
                description: index === 1 ? "Search adapters after shared pull" : skill.description,
                update_available: false,
                last_updated: "2026-04-08T08:00:00.000Z",
              },
              siblings_cleared: [],
              agent_link_failures: [],
            })),
            failed: [],
            skipped: [],
            channel_managed: [],
          };
          return report;
        }
        default:
          return undefined;
      }
    });
  });

  it("clears every card the backend reports as updated after update-all", async () => {
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
          return [];
        case "migrate_local_skills":
          return 0;
        case "scan_github_repo":
          expect(args).toEqual({ url: sourceUrl, fullDepth: true });
          return { source, source_url: sourceUrl, skills: targets };
        case "install_from_scan":
          expect(args).toEqual({
            spec: { source, source_url: sourceUrl, skills: targets },
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

  it("reinstalls only the requested skill identity from a multi-skill repository", async () => {
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
          return [];
        case "migrate_local_skills":
          return 0;
        case "scan_github_repo":
          expect(args).toEqual({ url: sourceUrl, fullDepth: true });
          return { source, source_url: sourceUrl, skills: targets };
        case "install_from_scan":
          expect(args).toEqual({
            spec: { source, source_url: sourceUrl, skills: targets },
            skills: [{ id: "opencli-search", folder_path: "skills/opencli-search" }],
          });
          return ["opencli-search"];
        default:
          return undefined;
      }
    });

    const { result } = renderHook(() => useSkills(), { wrapper: createWrapper() });
    await waitFor(() => expect(result.current.loading).toBe(false));

    let installed: string[] | undefined;
    await act(async () => {
      installed = await result.current.reinstallSkill(sourceUrl, "opencli-search");
    });

    expect(installed).toEqual(["opencli-search"]);
  });

  it("fails closed when the source repository no longer contains that skill identity", async () => {
    const sourceUrl = "https://github.com/jackwener/opencli.git";
    mockedInvoke.mockImplementation(async (command) => {
      switch (command) {
        case "list_skills":
          return INITIAL_SKILLS;
        case "refresh_skill_updates":
          return [];
        case "migrate_local_skills":
          return 0;
        case "scan_github_repo":
          return {
            source: "jackwener/opencli",
            source_url: sourceUrl,
            skills: [
              {
                id: "opencli-repair",
                folder_path: "skills/opencli-repair",
                description: "Repair adapters",
                already_installed: true,
              },
            ],
          };
        default:
          throw new Error(`Unexpected IPC: ${command}`);
      }
    });

    const { result } = renderHook(() => useSkills(), { wrapper: createWrapper() });
    await waitFor(() => expect(result.current.loading).toBe(false));

    await expect(result.current.reinstallSkill(sourceUrl, "opencli-search")).rejects.toThrow(/opencli-search/);
    expect(mockedInvoke).not.toHaveBeenCalledWith("install_from_scan", expect.anything());
  });

  it("surfaces update failures for every page using the shared hook", async () => {
    mockedInvoke.mockImplementation(async (command) => {
      switch (command) {
        case "list_skills":
          return INITIAL_SKILLS;
        case "refresh_skill_updates":
          return [];
        case "migrate_local_skills":
          return 0;
        case "update_skills":
          return {
            updated: [],
            failed: [{ name: "opencli-repair", error: "remote authentication failed" }],
            skipped: [],
            channel_managed: [],
          };
        default:
          return undefined;
      }
    });

    const { result } = renderHook(() => useSkills(), { wrapper: createWrapper() });
    await waitFor(() => expect(result.current.loading).toBe(false));

    await expect(result.current.updateSkill("opencli-repair")).rejects.toThrow("remote authentication failed");
    // runSkillUpdate surfaced the failure in a toast once; updateSkill must not
    // duplicate it.
    expect(toast.error).toHaveBeenCalledTimes(1);
    expect(toast.error).toHaveBeenCalledWith(expect.stringContaining("remote authentication failed"));
  });

  it("lists upstream-removed names in an info toast instead of failing the run", async () => {
    mockedInvoke.mockImplementation(async (command) => {
      switch (command) {
        case "list_skills":
          return INITIAL_SKILLS;
        case "refresh_skill_updates":
          return [];
        case "migrate_local_skills":
          return 0;
        case "update_skills":
          return {
            updated: [
              {
                skill: { ...INITIAL_SKILLS[0], update_available: false },
                siblings_cleared: [],
                agent_link_failures: [],
              },
            ],
            failed: [],
            skipped: ["opencli-usage"],
            channel_managed: [],
          };
        default:
          return undefined;
      }
    });

    const { result } = renderHook(() => useSkills(), { wrapper: createWrapper() });
    await waitFor(() => expect(result.current.loading).toBe(false));

    await act(async () => {
      await result.current.runSkillUpdate(["opencli-repair", "opencli-usage"]);
    });

    expect(toast.info).toHaveBeenCalledWith(expect.stringContaining("opencli-usage"));
  });

  it("keeps the skills context identity across a no-op list refetch", async () => {
    const { result } = renderHook(() => useSkills(), { wrapper: createWrapper() });
    await waitFor(() => expect(result.current.loading).toBe(false));
    const first = result.current;

    await act(async () => {
      await result.current.refresh(true, false);
    });
    await waitFor(() => expect(result.current.loading).toBe(false));

    expect(result.current).toBe(first);
  });

  it("forwards a carousel agentId to install_skill and only that icon is pending", async () => {
    let release!: (skill: Skill) => void;
    const gate = new Promise<Skill>((resolve) => {
      release = resolve;
    });
    mockedInvoke.mockImplementation(async (command, args) => {
      if (command === "install_skill") {
        expect(args).toEqual({
          url: INITIAL_SKILLS[0].git_url,
          name: "opencli-repair",
          agentId: "cursor",
          sessionId: expect.any(String),
        });
        return gate;
      }
      if (command === "list_skills") return INITIAL_SKILLS;
      if (command === "refresh_skill_updates") return [];
      if (command === "migrate_local_skills") return 0;
      return undefined;
    });

    const { result } = renderHook(() => useSkills(), { wrapper: createWrapper() });
    await waitFor(() => expect(result.current.loading).toBe(false));

    let finished = false;
    act(() => {
      void result.current.installSkill(INITIAL_SKILLS[0].git_url, "opencli-repair", "cursor").then(() => {
        finished = true;
      });
    });

    await waitFor(() => {
      expect(result.current.pendingAgentToggleKeys.has("opencli-repair::cursor")).toBe(true);
    });
    expect(result.current.pendingAgentToggleKeys.has("opencli-repair::deepseek")).toBe(false);

    await act(async () => {
      release({ ...INITIAL_SKILLS[0], agent_links: ["Cursor"] });
    });
    await waitFor(() => expect(finished).toBe(true));
    await waitFor(() => expect(result.current.pendingAgentToggleKeys.size).toBe(0));
  });

  it("exposes sidebar badge counts from the skills list", async () => {
    const { result } = renderHook(() => useSkillBadgeCounts(), { wrapper: createWrapper() });
    await waitFor(() => expect(result.current.pendingUpdatesCount).toBe(3));
  });
});
