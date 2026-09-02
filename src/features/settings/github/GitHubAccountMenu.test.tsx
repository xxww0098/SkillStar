import { invoke } from "@tauri-apps/api/core";
import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { GITHUB_ACCOUNT_MENU_EVENT } from "../../../lib/utils";
import { GitHubAccountMenu } from "./GitHubAccountMenu";

const mockedInvoke = vi.mocked(invoke);

const openPanel = () => fireEvent.click(screen.getByRole("button", { name: /GitHub|octocat/ }));

describe("GitHubAccountMenu", () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it("opens from the sidebar entry and runs the device flow", async () => {
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
    render(<GitHubAccountMenu />);

    expect(await screen.findByText("登录 GitHub")).toBeInTheDocument();
    // The panel stays closed until the sidebar entry is used.
    expect(screen.queryByText("通过 GitHub 登录")).not.toBeInTheDocument();

    openPanel();
    expect(await screen.findByText("通过 GitHub 登录")).toBeInTheDocument();
    expect(screen.getByText("私有 GitHub 技能")).toBeInTheDocument();
    expect(screen.getByText(/Administration: write/)).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "用 GitHub 继续" }));
    expect(await screen.findByText("ABCD-EFGH")).toBeInTheDocument();
    expect(screen.getByText("等待你在 GitHub 完成授权…")).toBeInTheDocument();
    expect(screen.getByText("等待 GitHub 授权…")).toBeInTheDocument();

    fireEvent.click(screen.getByRole("button", { name: "关闭" }));
    await waitFor(() => expect(screen.queryByRole("dialog")).not.toBeInTheDocument());
    expect(screen.getByText("等待 GitHub 授权…")).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "等待 GitHub 授权…" }));
    expect(await screen.findByText("ABCD-EFGH")).toBeInTheDocument();
  });

  it("shows the signed-in identity and keeps entry and panel on one state", async () => {
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
    render(<GitHubAccountMenu />);

    // The sidebar entry itself reflects the connected account.
    expect(await screen.findByText("@octocat")).toBeInTheDocument();

    openPanel();
    fireEvent.click(screen.getByRole("button", { name: "退出登录" }));
    await waitFor(() => expect(screen.getByText("通过 GitHub 登录")).toBeInTheDocument());
    // Logging out inside the panel updates the sidebar entry too.
    expect(screen.getByText("登录 GitHub")).toBeInTheDocument();
    expect(screen.queryByText("@octocat")).not.toBeInTheDocument();
  });

  it("opens on the shared-channel sign-in event and surfaces expired state", async () => {
    mockedInvoke.mockImplementation(async (command) => {
      if (command === "github_auth_status") {
        return { state: "expired", identity: { id: 42, login: "octocat", avatar_url: null } };
      }
      // Silent auto-refresh fails (no valid refresh token) and keeps the
      // expired state visible with its explicit refresh/sign-in actions.
      if (command === "github_auth_refresh") {
        throw { code: "refresh_unavailable", message: "The GitHub session cannot be refreshed; sign in again" };
      }
      throw new Error(`Unexpected command: ${command}`);
    });
    render(<GitHubAccountMenu />);
    await screen.findByText("@octocat");

    fireEvent(window, new CustomEvent(GITHUB_ACCOUNT_MENU_EVENT));
    expect(await screen.findByText("GitHub 登录已失效")).toBeInTheDocument();
    expect(screen.getByText("登录已失效")).toBeInTheDocument();
  });

  it("reports a proxy failure and retries", async () => {
    mockedInvoke
      .mockRejectedValueOnce({ code: "proxy", message: "proxy failed" })
      .mockResolvedValueOnce({ state: "signed_out" });
    render(<GitHubAccountMenu />);

    openPanel();
    expect(await screen.findByText("无法连接 GitHub，请检查 SkillStar 代理设置。")).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "重试" }));
    await waitFor(() => {
      expect(screen.queryByText("无法连接 GitHub，请检查 SkillStar 代理设置。")).not.toBeInTheDocument();
    });
  });
});
