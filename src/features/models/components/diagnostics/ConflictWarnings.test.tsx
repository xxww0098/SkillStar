import { invoke } from "@tauri-apps/api/core";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { ConflictWarnings } from "./ConflictWarnings";

afterEach(cleanup);

const mockInvoke = vi.mocked(invoke);

describe("ConflictWarnings", () => {
  beforeEach(() => {
    mockInvoke.mockReset();
  });

  it("renders nothing when no conflicts are detected", async () => {
    mockInvoke.mockResolvedValue([]);
    const { container } = render(<ConflictWarnings providerId="test-id" />);
    await waitFor(() => {
      expect(mockInvoke).toHaveBeenCalledWith("detect_provider_conflicts", { providerId: "test-id" });
    });
    expect(container.querySelector("[data-testid='conflict-warnings']")).not.toBeInTheDocument();
  });

  it("displays warning banner for EnvVarOverride conflict", async () => {
    mockInvoke.mockResolvedValue([
      {
        conflict_type: "EnvVarOverride",
        description: "环境变量 ANTHROPIC_API_KEY 已设置",
        file_path: null,
        details: "ANTHROPIC_API_KEY=sk-a***",
      },
    ]);

    render(<ConflictWarnings providerId="test-id" />);

    await waitFor(() => {
      expect(screen.getByText(/检测到环境变量 ANTHROPIC_API_KEY 可能覆盖配置文件设置/)).toBeInTheDocument();
    });
  });

  it("displays warning banner for LegacyConfig conflict", async () => {
    mockInvoke.mockResolvedValue([
      {
        conflict_type: "LegacyConfig",
        description: "检测到旧版 ~/.claude.json 配置文件",
        file_path: "~/.claude.json",
        details: null,
      },
    ]);

    render(<ConflictWarnings providerId="test-id" />);

    await waitFor(() => {
      expect(screen.getByText(/检测到旧版 ~\/\.claude\.json 配置文件可能产生冲突/)).toBeInTheDocument();
    });
  });

  it("displays multiple warning banners for multiple conflicts", async () => {
    mockInvoke.mockResolvedValue([
      {
        conflict_type: "EnvVarOverride",
        description: "环境变量 ANTHROPIC_API_KEY 已设置",
        file_path: null,
        details: "ANTHROPIC_API_KEY=sk-a***",
      },
      {
        conflict_type: "EnvVarOverride",
        description: "环境变量 OPENAI_API_KEY 已设置",
        file_path: null,
        details: "OPENAI_API_KEY=sk-o***",
      },
    ]);

    render(<ConflictWarnings providerId="test-id" />);

    await waitFor(() => {
      expect(screen.getByText(/检测到环境变量 ANTHROPIC_API_KEY 可能覆盖配置文件设置/)).toBeInTheDocument();
      expect(screen.getByText(/检测到环境变量 OPENAI_API_KEY 可能覆盖配置文件设置/)).toBeInTheDocument();
    });
  });

  it("allows dismissing a warning banner", async () => {
    mockInvoke.mockResolvedValue([
      {
        conflict_type: "EnvVarOverride",
        description: "环境变量 ANTHROPIC_API_KEY 已设置",
        file_path: null,
        details: "ANTHROPIC_API_KEY=sk-a***",
      },
    ]);

    render(<ConflictWarnings providerId="test-id" />);

    await waitFor(() => {
      expect(screen.getByText(/检测到环境变量 ANTHROPIC_API_KEY/)).toBeInTheDocument();
    });

    const dismissButton = screen.getByLabelText("关闭警告");
    fireEvent.click(dismissButton);

    expect(screen.queryByText(/检测到环境变量 ANTHROPIC_API_KEY/)).not.toBeInTheDocument();
  });

  it("shows ExternalModification conflict as a dialog", async () => {
    mockInvoke.mockResolvedValue([
      {
        conflict_type: "ExternalModification",
        description: "配置文件在上次同步后被外部修改",
        file_path: "~/.claude/settings.json",
        details: null,
      },
    ]);

    render(<ConflictWarnings providerId="test-id" />);

    await waitFor(() => {
      expect(screen.getByText("配置文件冲突")).toBeInTheDocument();
    });

    // Dialog should show three action buttons
    expect(screen.getByText("覆盖")).toBeInTheDocument();
    expect(screen.getByText("取消")).toBeInTheDocument();
    expect(screen.getByText("打开目录")).toBeInTheDocument();
  });

  it("closes ExternalModification dialog on cancel", async () => {
    mockInvoke.mockResolvedValue([
      {
        conflict_type: "ExternalModification",
        description: "配置文件在上次同步后被外部修改",
        file_path: "~/.claude/settings.json",
        details: null,
      },
    ]);

    render(<ConflictWarnings providerId="test-id" />);

    await waitFor(() => {
      expect(screen.getByText("配置文件冲突")).toBeInTheDocument();
    });

    fireEvent.click(screen.getByText("取消"));

    await waitFor(() => {
      expect(screen.queryByText("配置文件冲突")).not.toBeInTheDocument();
    });
  });

  it("re-detects conflicts when providerId changes", async () => {
    mockInvoke.mockResolvedValue([]);

    const { rerender } = render(<ConflictWarnings providerId="provider-1" />);

    await waitFor(() => {
      expect(mockInvoke).toHaveBeenCalledTimes(1);
    });

    mockInvoke.mockResolvedValue([
      {
        conflict_type: "LegacyConfig",
        description: "检测到旧版配置",
        file_path: "~/.claude.json",
        details: null,
      },
    ]);

    rerender(<ConflictWarnings providerId="provider-2" />);

    await waitFor(() => {
      expect(mockInvoke).toHaveBeenCalledTimes(2);
      expect(screen.getByText(/检测到旧版 ~\/\.claude\.json 配置文件可能产生冲突/)).toBeInTheDocument();
    });
  });

  it("does not show ExternalModification as a banner", async () => {
    mockInvoke.mockResolvedValue([
      {
        conflict_type: "ExternalModification",
        description: "配置文件在上次同步后被外部修改",
        file_path: "~/.claude/settings.json",
        details: null,
      },
      {
        conflict_type: "EnvVarOverride",
        description: "环境变量 ANTHROPIC_API_KEY 已设置",
        file_path: null,
        details: "ANTHROPIC_API_KEY=sk-a***",
      },
    ]);

    render(<ConflictWarnings providerId="test-id" />);

    await waitFor(() => {
      // EnvVarOverride should appear as banner
      expect(screen.getByText(/检测到环境变量 ANTHROPIC_API_KEY/)).toBeInTheDocument();
    });

    // ExternalModification should NOT appear as a banner text in the warnings container
    const warningsContainer = screen.getByTestId("conflict-warnings");
    expect(warningsContainer).not.toHaveTextContent("配置文件在上次同步后被外部修改");
  });

  it("handles invoke failure gracefully", async () => {
    mockInvoke.mockRejectedValue(new Error("Backend error"));

    const { container } = render(<ConflictWarnings providerId="test-id" />);

    // Should not crash, just render nothing
    await waitFor(() => {
      expect(mockInvoke).toHaveBeenCalled();
    });
    expect(container.querySelector("[data-testid='conflict-warnings']")).not.toBeInTheDocument();
  });
});
