import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { invoke } from "@tauri-apps/api/core";
import { act, renderHook, waitFor } from "@testing-library/react";
import type { ReactNode } from "react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { AgentProfile } from "../types";
import { useAgentProfiles } from "./useAgentProfiles";

const mockedInvoke = vi.mocked(invoke);

const MOCK_PROFILES: AgentProfile[] = [
  {
    id: "claude",
    display_name: "Claude Code",
    icon: "claude.svg",
    enabled: true,
    global_skills_dir: "/home/user/.claude/skills",
    project_skills_rel: ".claude/skills",
    installed: true,
    synced_count: 3,
  },
  {
    id: "cursor",
    display_name: "Cursor",
    icon: "cursor.svg",
    enabled: false,
    global_skills_dir: "/home/user/.cursor/rules/skills",
    project_skills_rel: ".cursor/rules/skills",
    installed: false,
    synced_count: 0,
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
    return <QueryClientProvider client={queryClient}>{children}</QueryClientProvider>;
  };
}

describe("useAgentProfiles", () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it("should load profiles on mount", async () => {
    mockedInvoke.mockResolvedValueOnce(MOCK_PROFILES);

    const { result } = renderHook(() => useAgentProfiles(), { wrapper: createWrapper() });
    expect(result.current.loading).toBe(true);

    await waitFor(() => {
      expect(result.current.loading).toBe(false);
    });

    expect(result.current.profiles).toHaveLength(2);
    expect(result.current.profiles[0].id).toBe("claude");
    expect(mockedInvoke).toHaveBeenCalledWith("list_agent_profiles");
  });

  it("should share one request and one cache across concurrent instances", async () => {
    mockedInvoke.mockResolvedValue(MOCK_PROFILES);

    const wrapper = createWrapper();
    const first = renderHook(() => useAgentProfiles(), { wrapper });
    const second = renderHook(() => useAgentProfiles(), { wrapper });

    await waitFor(() => {
      expect(first.result.current.loading).toBe(false);
      expect(second.result.current.loading).toBe(false);
    });

    // Both instances observe the same query: exactly one invoke total.
    expect(mockedInvoke).toHaveBeenCalledTimes(1);
    expect(mockedInvoke).toHaveBeenCalledWith("list_agent_profiles");
    expect(first.result.current.profiles).toHaveLength(2);
    expect(second.result.current.profiles).toBe(first.result.current.profiles);
  });

  it("should handle load failure gracefully", async () => {
    mockedInvoke.mockRejectedValueOnce(new Error("Backend error"));

    const { result } = renderHook(() => useAgentProfiles(), { wrapper: createWrapper() });

    await waitFor(() => {
      expect(result.current.loading).toBe(false);
    });

    expect(result.current.profiles).toHaveLength(0);
  });

  it("toggleProfile should update local state optimistically", async () => {
    mockedInvoke.mockResolvedValueOnce(MOCK_PROFILES); // initial load
    mockedInvoke.mockResolvedValueOnce(true); // toggle response

    const { result } = renderHook(() => useAgentProfiles(), { wrapper: createWrapper() });

    await waitFor(() => {
      expect(result.current.loading).toBe(false);
    });

    let newState: boolean | undefined;
    await act(async () => {
      newState = await result.current.toggleProfile("cursor");
    });

    expect(newState).toBe(true);
    expect(mockedInvoke).toHaveBeenCalledWith("toggle_agent_profile", { id: "cursor" });

    // Shared cache should be updated (observer notification lands on the next tick)
    await waitFor(() => {
      const cursor = result.current.profiles.find((p: AgentProfile) => p.id === "cursor");
      expect(cursor?.enabled).toBe(true);
      expect(cursor?.installed).toBe(true);
    });
  });

  it("toggleProfile should propagate the update to other instances", async () => {
    mockedInvoke.mockResolvedValueOnce(MOCK_PROFILES); // initial load (shared)
    mockedInvoke.mockResolvedValueOnce(true); // toggle response

    const wrapper = createWrapper();
    const first = renderHook(() => useAgentProfiles(), { wrapper });
    const second = renderHook(() => useAgentProfiles(), { wrapper });

    await waitFor(() => {
      expect(first.result.current.loading).toBe(false);
      expect(second.result.current.loading).toBe(false);
    });

    await act(async () => {
      await first.result.current.toggleProfile("cursor");
    });

    await waitFor(() => {
      const cursor = second.result.current.profiles.find((p: AgentProfile) => p.id === "cursor");
      expect(cursor?.enabled).toBe(true);
    });
  });

  it("unlinkAllFromAgent should update synced_count", async () => {
    mockedInvoke.mockResolvedValueOnce(MOCK_PROFILES); // initial load
    mockedInvoke.mockResolvedValueOnce(3); // unlink returns count removed

    const { result } = renderHook(() => useAgentProfiles(), { wrapper: createWrapper() });

    await waitFor(() => {
      expect(result.current.loading).toBe(false);
    });

    let removed: number | undefined;
    await act(async () => {
      removed = await result.current.unlinkAllFromAgent("claude");
    });

    expect(removed).toBe(3);
    await waitFor(() => {
      const claude = result.current.profiles.find((p: AgentProfile) => p.id === "claude");
      expect(claude?.synced_count).toBe(0);
    });
  });

  it("addCustomProfile should refresh the list", async () => {
    mockedInvoke.mockResolvedValueOnce(MOCK_PROFILES); // initial load

    const { result } = renderHook(() => useAgentProfiles(), { wrapper: createWrapper() });

    await waitFor(() => {
      expect(result.current.loading).toBe(false);
    });

    // add_custom_agent_profile call
    mockedInvoke.mockResolvedValueOnce(undefined);
    // refresh (list_agent_profiles) call
    mockedInvoke.mockResolvedValueOnce([
      ...MOCK_PROFILES,
      {
        id: "custom-1",
        display_name: "Custom Agent",
        icon: "custom.svg",
        enabled: false,
        global_skills_dir: "/home/user/.custom/skills",
        project_skills_rel: "",
        installed: false,
        synced_count: 0,
      },
    ]);

    await act(async () => {
      await result.current.addCustomProfile({
        id: "custom-1",
        display_name: "Custom Agent",
        global_skills_dir: "/home/user/.custom/skills",
        project_skills_rel: "",
        icon_data_uri: null,
      });
    });

    await waitFor(() => {
      expect(result.current.profiles).toHaveLength(3);
    });
  });
});
