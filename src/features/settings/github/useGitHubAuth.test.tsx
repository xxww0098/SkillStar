import { invoke } from "@tauri-apps/api/core";
import { act, renderHook, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { useGitHubAuth } from "./useGitHubAuth";

const mockedInvoke = vi.mocked(invoke);

describe("useGitHubAuth", () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it("moves from signed out through device guidance to a connected identity and logout", async () => {
    mockedInvoke.mockImplementation(async (command) => {
      if (command === "github_auth_status") return { state: "signed_out" };
      if (command === "github_auth_start") {
        return {
          user_code: "ABCD-EFGH",
          verification_uri: "https://github.com/login/device",
          expires_at: "2026-08-05T10:15:00Z",
          interval_seconds: 5,
        };
      }
      if (command === "github_auth_poll") {
        return {
          state: "connected",
          connection: {
            state: "connected",
            identity: { id: 42, login: "octocat", avatar_url: null },
            access_expires_at: "2026-08-05T18:00:00Z",
          },
        };
      }
      if (command === "github_auth_logout") return undefined;
      throw new Error(`Unexpected command: ${command}`);
    });

    const { result } = renderHook(() => useGitHubAuth());
    await waitFor(() => expect(result.current.status?.state).toBe("signed_out"));

    await act(() => result.current.start());
    expect(result.current.authorization?.user_code).toBe("ABCD-EFGH");

    await act(() => result.current.pollNow());
    expect(result.current.status).toMatchObject({
      state: "connected",
      identity: { login: "octocat" },
    });
    expect(result.current.authorization).toBeNull();

    await act(() => result.current.logout());
    expect(result.current.status?.state).toBe("signed_out");
  });

  it("preserves structured proxy outcomes", async () => {
    mockedInvoke.mockRejectedValueOnce({
      code: "proxy",
      message: "Unable to create the GitHub client; check proxy settings",
    });
    const proxy = renderHook(() => useGitHubAuth());
    await waitFor(() => expect(proxy.result.current.error?.code).toBe("proxy"));
    proxy.unmount();
  });

  it("silently recovers an expired session at mount when a refresh token is still valid", async () => {
    mockedInvoke.mockImplementation(async (command) => {
      if (command === "github_auth_status") {
        return { state: "expired", identity: { id: 42, login: "octocat", avatar_url: null } };
      }
      if (command === "github_auth_refresh") {
        return {
          state: "connected",
          identity: { id: 42, login: "octocat", avatar_url: null },
          access_expires_at: null,
        };
      }
      throw new Error(`Unexpected command: ${command}`);
    });

    const { result } = renderHook(() => useGitHubAuth());
    await waitFor(() => expect(result.current.status).toMatchObject({ state: "connected" }));
    expect(result.current.error).toBeNull();
  });

  it("keeps the expired state when silent recovery cannot refresh", async () => {
    mockedInvoke.mockImplementation(async (command) => {
      if (command === "github_auth_status") {
        return { state: "expired", identity: { id: 42, login: "octocat", avatar_url: null } };
      }
      if (command === "github_auth_refresh") {
        throw { code: "refresh_unavailable", message: "The GitHub session cannot be refreshed; sign in again" };
      }
      throw new Error(`Unexpected command: ${command}`);
    });

    const { result } = renderHook(() => useGitHubAuth());
    await waitFor(() => expect(result.current.status?.state).toBe("expired"));
    await waitFor(() => expect(result.current.busy).toBe(false));
    // A silent background attempt must not surface an error banner.
    expect(result.current.error).toBeNull();
  });
});
