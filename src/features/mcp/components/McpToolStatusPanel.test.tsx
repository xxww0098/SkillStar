import { act, fireEvent, render, screen, within } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { McpToolStatusPanel } from "./McpToolStatusPanel";

const { mockTauriInvoke } = vi.hoisted(() => ({
  mockTauriInvoke: vi.fn(async (..._args: unknown[]) => undefined),
}));

vi.mock("../../../lib/ipc", () => ({
  tauriInvoke: (...args: unknown[]) => mockTauriInvoke(...args),
}));

const mockRefetch = vi.fn();
let mockStatusesData = {
  statuses: [
    {
      toolId: "claude-code" as const,
      label: "Claude Code",
      configPath: "/Users/testuser/.claude.json",
      installed: true,
      serverCount: 2,
    },
    {
      toolId: "cursor" as const,
      label: "Cursor",
      configPath: "/Users/testuser/.cursor/mcp.json",
      installed: true,
      serverCount: 0,
    },
    {
      toolId: "zed" as const,
      label: "Zed",
      configPath: "/Users/testuser/.config/zed/settings.json",
      installed: false,
      serverCount: 0,
    },
    {
      toolId: "claude-desktop-chat" as const,
      label: "Claude Desktop Chat",
      configPath: "/Users/testuser/.claude-desktop-chat.json",
      installed: true,
      serverCount: 0,
    },
  ],
  installedCount: 3,
  isLoading: false,
  isFetching: false,
  refetch: mockRefetch,
};

vi.mock("../hooks/useMcpToolStatuses", () => ({
  useMcpToolStatuses: () => mockStatusesData,
}));

vi.mock("../../../hooks/useAgentProfiles", () => ({
  useAgentProfiles: () => ({
    profiles: [
      {
        id: "claude",
        display_name: "Claude Code",
        icon: "claude",
        enabled: true,
        installed: true,
      },
    ],
  }),
}));

const mockCopyToClipboard = vi.fn().mockResolvedValue(true);
vi.mock("../../../lib/utils", async () => {
  const actual = await vi.importActual("../../../lib/utils");
  return {
    ...actual,
    copyToClipboard: (...args: unknown[]) => mockCopyToClipboard(...args),
  };
});

describe("McpToolStatusPanel", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    mockStatusesData = {
      statuses: [
        {
          toolId: "claude-code",
          label: "Claude Code",
          configPath: "/Users/testuser/.claude.json",
          installed: true,
          serverCount: 2,
        },
        {
          toolId: "cursor",
          label: "Cursor",
          configPath: "/Users/testuser/.cursor/mcp.json",
          installed: true,
          serverCount: 0,
        },
        {
          toolId: "zed",
          label: "Zed",
          configPath: "/Users/testuser/.config/zed/settings.json",
          installed: false,
          serverCount: 0,
        },
        {
          toolId: "claude-desktop-chat",
          label: "Claude Desktop Chat",
          configPath: "/Users/testuser/.claude-desktop-chat.json",
          installed: true,
          serverCount: 0,
        },
      ],
      installedCount: 3,
      isLoading: false,
      isFetching: false,
      refetch: mockRefetch,
    };
  });

  it("lists each agent config path and live server count", () => {
    render(<McpToolStatusPanel />);

    expect(screen.getByText("Claude Code", { selector: "span" })).toBeInTheDocument();
    expect(screen.getByText("Cursor", { selector: "span" })).toBeInTheDocument();
    expect(screen.getByText("Zed", { selector: "span" })).toBeInTheDocument();
    expect(screen.getByText("~/.claude.json")).toBeInTheDocument();
    expect(screen.getByText("~/.cursor/mcp.json")).toBeInTheDocument();
    expect(screen.getByText("~/.config/zed/settings.json")).toBeInTheDocument();
    expect(screen.getByText("配置中有 2 个 server")).toBeInTheDocument();
  });

  it("is a flat inspector, not a filterable dashboard", () => {
    render(<McpToolStatusPanel />);
    expect(screen.queryByPlaceholderText(/搜索 Agent 或路径/)).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: /有 Server/ })).not.toBeInTheDocument();
  });

  it("copies config path to clipboard when copy button clicked", async () => {
    render(<McpToolStatusPanel />);

    const copyButtons = screen.getAllByRole("button", { name: "复制完整路径" });
    await act(async () => {
      fireEvent.click(copyButtons[0]);
    });

    expect(mockCopyToClipboard).toHaveBeenCalledWith("/Users/testuser/.claude.json");
  });

  it("opens the config file's directory (not the file) via open_folder IPC", async () => {
    render(<McpToolStatusPanel />);

    const openButtons = screen.getAllByRole("button", { name: "在文件管理器中打开" });
    await act(async () => {
      fireEvent.click(openButtons[0]);
    });

    expect(mockTauriInvoke).toHaveBeenCalledWith("open_folder", { path: "/Users/testuser" });
  });

  it("does not render the open-folder button for an uninstalled tool", () => {
    render(<McpToolStatusPanel />);

    const zedCard = screen.getByText("~/.config/zed/settings.json").closest(".rounded-xl");
    expect(zedCard).not.toBeNull();
    expect(
      within(zedCard as HTMLElement).queryByRole("button", { name: "在文件管理器中打开" }),
    ).not.toBeInTheDocument();

    // Sanity: an installed tool still exposes the button.
    const claudeCard = screen.getByText("~/.claude.json").closest(".rounded-xl");
    expect(within(claudeCard as HTMLElement).getByRole("button", { name: "在文件管理器中打开" })).toBeInTheDocument();
  });

  it("renders the no-Agent-profile hint only for tools without a reachable profile", () => {
    // Isolate the gating: compare a tool WITH a profile (claude-code, mapped from
    // the mocked `claude` profile) against one WITHOUT (claude-desktop-chat, which
    // has no row in MCP_TOOL_BY_AGENT_ID). Under the claude-only mock, cursor/zed
    // would also be unreachable, so the fixture is scoped to just these two.
    mockStatusesData = {
      statuses: [
        {
          toolId: "claude-code",
          label: "Claude Code",
          configPath: "/Users/testuser/.claude.json",
          installed: true,
          serverCount: 2,
        },
        {
          toolId: "claude-desktop-chat",
          label: "Claude Desktop Chat",
          configPath: "/Users/testuser/.claude-desktop-chat.json",
          installed: true,
          serverCount: 0,
        },
      ],
      installedCount: 2,
      isLoading: false,
      isFetching: false,
      refetch: mockRefetch,
    };
    render(<McpToolStatusPanel />);

    const claudeCard = screen.getByText("~/.claude.json").closest(".rounded-xl");
    expect(claudeCard).not.toBeNull();
    expect(within(claudeCard as HTMLElement).queryByText(/未在设置 → 智能体中启用/)).not.toBeInTheDocument();

    const desktopCard = screen.getByText("~/.claude-desktop-chat.json").closest(".rounded-xl");
    expect(desktopCard).not.toBeNull();
    expect(within(desktopCard as HTMLElement).getByText(/未在设置 → 智能体中启用/)).toBeInTheDocument();
  });
});
