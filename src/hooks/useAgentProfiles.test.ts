import { invoke } from "@tauri-apps/api/core";
import { act, renderHook, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { AgentProfile } from "../types";
import { useAgentProfiles } from "./useAgentProfiles";

const mockedInvoke = vi.mocked(invoke);

const MOCK_PROFILES: AgentProfile[] = [
  {
    id: "claude",
    display_name: "Claude",
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
    installed: true,
    synced_count: 0,
  },
];

describe("useAgentProfiles", () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it("should load profiles on mount", async () => {
    mockedInvoke.mockResolvedValueOnce(MOCK_PROFILES);

    const { result } = renderHook(() => useAgentProfiles());
    expect(result.current.loading).toBe(true);

    await waitFor(() => {
      expect(result.current.loading).toBe(false);
    });

    expect(result.current.profiles).toHaveLength(2);
    expect(result.current.profiles[0].id).toBe("claude");
    expect(mockedInvoke).toHaveBeenCalledWith("list_agent_profiles");
  });

  it("should handle load failure gracefully", async () => {
    mockedInvoke.mockRejectedValueOnce(new Error("Backend error"));

    const { result } = renderHook(() => useAgentProfiles());

    await waitFor(() => {
      expect(result.current.loading).toBe(false);
    });

    expect(result.current.profiles).toHaveLength(0);
  });

  it("toggleProfile should update local state optimistically", async () => {
    mockedInvoke.mockResolvedValueOnce(MOCK_PROFILES); // initial load
    mockedInvoke.mockResolvedValueOnce(true); // toggle response

    const { result } = renderHook(() => useAgentProfiles());

    await waitFor(() => {
      expect(result.current.loading).toBe(false);
    });

    let newState: boolean | undefined;
    await act(async () => {
      newState = await result.current.toggleProfile("cursor");
    });

    expect(newState).toBe(true);
    expect(mockedInvoke).toHaveBeenCalledWith("toggle_agent_profile", { id: "cursor" });

    // Local state should be updated
    const cursor = result.current.profiles.find((p: AgentProfile) => p.id === "cursor");
    expect(cursor?.enabled).toBe(true);
  });

  it("unlinkAllFromAgent should update synced_count", async () => {
    mockedInvoke.mockResolvedValueOnce(MOCK_PROFILES); // initial load
    mockedInvoke.mockResolvedValueOnce(3); // unlink returns count removed

    const { result } = renderHook(() => useAgentProfiles());

    await waitFor(() => {
      expect(result.current.loading).toBe(false);
    });

    let removed: number | undefined;
    await act(async () => {
      removed = await result.current.unlinkAllFromAgent("claude");
    });

    expect(removed).toBe(3);
    const claude = result.current.profiles.find((p: AgentProfile) => p.id === "claude");
    expect(claude?.synced_count).toBe(0);
  });

  it("addCustomProfile should refresh the list", async () => {
    mockedInvoke.mockResolvedValueOnce(MOCK_PROFILES); // initial load

    const { result } = renderHook(() => useAgentProfiles());

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
        enabled: true,
        global_skills_dir: "/home/user/.custom/skills",
        project_skills_rel: "",
        installed: true,
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

    expect(result.current.profiles).toHaveLength(3);
  });
});
