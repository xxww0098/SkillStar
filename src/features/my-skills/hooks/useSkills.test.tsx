import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { invoke } from "@tauri-apps/api/core";
import { act, renderHook, waitFor } from "@testing-library/react";
import type { ReactNode } from "react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { Skill, SkillUpdateReport } from "../../../types";
import { SkillsProvider, useSkills } from "./useSkills";

const mockedInvoke = vi.mocked(invoke);

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

    mockedInvoke.mockImplementation(async (command, args) => {
      switch (command) {
        case "list_skills":
          return INITIAL_SKILLS;
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
  });
});
