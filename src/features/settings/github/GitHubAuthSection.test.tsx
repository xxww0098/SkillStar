import { invoke } from "@tauri-apps/api/core";
import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { GitHubAuthSection } from "./GitHubAuthSection";

const mockedInvoke = vi.mocked(invoke);

describe("GitHubAuthSection", () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it("shows login and permission guidance, then the in-progress device code", async () => {
    mockedInvoke.mockImplementation(async (command) => {
      if (command === "github_auth_status") return { state: "signed_out" };
      if (command === "github_auth_start") {
        return {
          user_code: "ABCD-EFGH",
          verification_uri: "https://github.com/login/device",
          expires_at: "2026-08-05T10:15:00Z",
          interval_seconds: 60,
        };
      }
      throw new Error(`Unexpected command: ${command}`);
    });
    render(<GitHubAuthSection />);

    expect(await screen.findByText("通过 GitHub 登录")).toBeInTheDocument();
    expect(screen.getByText(/Administration: write/)).toBeInTheDocument();
    expect(screen.getByText(/Contents: write/)).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "开始登录" }));
    expect(await screen.findByText("ABCD-EFGH")).toBeInTheDocument();
    expect(screen.getByText("等待你在 GitHub 完成授权…")).toBeInTheDocument();
  });

  it("shows connected, expired, proxy failure, and logout states", async () => {
    mockedInvoke.mockImplementation(async (command) => {
      if (command === "github_auth_status") {
        return {
          state: "connected",
          identity: { id: 42, login: "octocat", avatar_url: null },
          access_expires_at: "2026-08-05T18:00:00Z",
        };
      }
      if (command === "github_auth_logout") return undefined;
      throw new Error(`Unexpected command: ${command}`);
    });
    const connected = render(<GitHubAuthSection />);
    expect(await screen.findByText("@octocat")).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "退出登录" }));
    await waitFor(() => expect(screen.getByText("通过 GitHub 登录")).toBeInTheDocument());
    connected.unmount();

    mockedInvoke.mockReset();
    mockedInvoke.mockResolvedValueOnce({
      state: "expired",
      identity: { id: 42, login: "octocat", avatar_url: null },
    });
    const expired = render(<GitHubAuthSection />);
    expect(await screen.findByText("GitHub 登录已失效")).toBeInTheDocument();
    expired.unmount();

    mockedInvoke.mockReset();
    mockedInvoke
      .mockRejectedValueOnce({ code: "proxy", message: "proxy failed" })
      .mockResolvedValueOnce({ state: "signed_out" });
    render(<GitHubAuthSection />);
    expect(await screen.findByText("无法连接 GitHub，请检查 SkillStar 代理设置。")).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "重试" }));
    await waitFor(() => {
      expect(screen.queryByText("无法连接 GitHub，请检查 SkillStar 代理设置。")).not.toBeInTheDocument();
    });
  });
});
