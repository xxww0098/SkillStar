import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { invoke } from "@tauri-apps/api/core";
import { act, renderHook, waitFor } from "@testing-library/react";
import type { ReactNode } from "react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { Skill, UpdateResult } from "../../../types";
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
        case "update_skill": {
          expect(args).toEqual({ name: "opencli-repair" });

          const result: UpdateResult = {
            skill: {
              ...INITIAL_SKILLS[0],
              update_available: false,
              last_updated: "2026-04-08T08:00:00.000Z",
            },
            siblings_cleared: [],
          };
          return result;
        }
        default:
          return undefined;
      }
    });
  });

  it("clears UI-known same-repo siblings immediately after update-all", async () => {
    const { result } = renderHook(() => useSkills(), { wrapper: createWrapper() });

    await waitFor(() => {
      expect(result.current.loading).toBe(false);
    });

    expect(result.current.skills).toHaveLength(3);
    expect(result.current.skills.every((skill) => skill.update_available)).toBe(true);

    await act(async () => {
      await result.current.updateSkill(
        "opencli-repair",
        INITIAL_SKILLS.map((skill) => skill.name),
      );
    });

    await waitFor(() => {
      expect(result.current.skills.every((skill) => !skill.update_available)).toBe(true);
    });
  });
});
