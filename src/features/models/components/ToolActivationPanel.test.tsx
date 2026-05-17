import { invoke } from "@tauri-apps/api/core";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { ToolActivationPanel } from "./ToolActivationPanel";

afterEach(cleanup);

const mockInvoke = vi.mocked(invoke);

// Default props for a fully configured provider
const defaultProps = {
  providerId: "provider-1",
  providerModels: ["deepseek-chat", "deepseek-reasoner"],
  defaultModel: "deepseek-chat",
  baseUrlOpenai: "https://api.deepseek.com/v1",
  baseUrlAnthropic: "https://api.deepseek.com/anthropic",
};

describe("ToolActivationPanel", () => {
  beforeEach(() => {
    mockInvoke.mockReset();
  });

  describe("tool installation detection", () => {
    it("calls detect_tool_installation for each known tool on mount", async () => {
      mockInvoke.mockImplementation(async (cmd: string) => {
        if (cmd === "get_tool_activations") return {};
        if (cmd === "detect_tool_installation") return { installed: true, binary_found: true, config_dir_found: true };
        return undefined;
      });

      render(<ToolActivationPanel {...defaultProps} />);

      await waitFor(() => {
        expect(mockInvoke).toHaveBeenCalledWith("detect_tool_installation", {
          toolId: "claude-code",
        });
        expect(mockInvoke).toHaveBeenCalledWith("detect_tool_installation", {
          toolId: "codex",
        });
      });
    });

    it("shows '○ 未安装' label and disables toggle when tool is not installed", async () => {
      mockInvoke.mockImplementation(async (cmd: string) => {
        if (cmd === "get_tool_activations") return {};
        if (cmd === "detect_tool_installation")
          return { installed: false, binary_found: false, config_dir_found: false };
        return undefined;
      });

      render(<ToolActivationPanel {...defaultProps} />);

      await waitFor(() => {
        const statusTexts = screen.getAllByText("○ 未安装");
        expect(statusTexts.length).toBeGreaterThan(0);
      });

      // Toggles should be disabled
      const switches = screen.getAllByRole("switch");
      for (const sw of switches) {
        expect(sw).toBeDisabled();
      }
    });

    it("shows installation docs link when tool is not installed and panel is expanded", async () => {
      mockInvoke.mockImplementation(async (cmd: string) => {
        if (cmd === "get_tool_activations") return {};
        if (cmd === "detect_tool_installation")
          return { installed: false, binary_found: false, config_dir_found: false };
        return undefined;
      });

      render(<ToolActivationPanel {...defaultProps} />);

      await waitFor(() => {
        expect(screen.getAllByText("○ 未安装").length).toBeGreaterThan(0);
      });

      // Expand the Claude Code panel
      const claudePanel = screen.getByLabelText("Claude Code 工具面板");
      fireEvent.click(claudePanel);

      await waitFor(() => {
        expect(screen.getByText("安装文档")).toBeInTheDocument();
      });

      // Verify the link points to the correct docs URL
      const link = screen.getByText("安装文档").closest("a");
      expect(link).toHaveAttribute("href", "https://docs.anthropic.com/en/docs/claude-code/overview");
    });
  });

  describe("base_url missing - disabled state", () => {
    it("disables Claude Code toggle when base_url_anthropic is empty", async () => {
      mockInvoke.mockImplementation(async (cmd: string) => {
        if (cmd === "get_tool_activations") return {};
        if (cmd === "detect_tool_installation") return { installed: true, binary_found: true, config_dir_found: true };
        return undefined;
      });

      render(
        <ToolActivationPanel
          {...defaultProps}
          baseUrlAnthropic="" // Empty anthropic URL
        />,
      );

      await waitFor(() => {
        // Claude Code should show disabled status
        const switches = screen.getAllByRole("switch");
        // First switch is Claude Code
        expect(switches[0]).toBeDisabled();
      });
    });

    it("disables Codex toggle when base_url_openai is empty", async () => {
      mockInvoke.mockImplementation(async (cmd: string) => {
        if (cmd === "get_tool_activations") return {};
        if (cmd === "detect_tool_installation") return { installed: true, binary_found: true, config_dir_found: true };
        return undefined;
      });

      render(
        <ToolActivationPanel
          {...defaultProps}
          baseUrlOpenai="" // Empty openai URL
        />,
      );

      await waitFor(() => {
        // Codex should show disabled status
        const switches = screen.getAllByRole("switch");
        // Second switch is Codex
        expect(switches[1]).toBeDisabled();
      });
    });

    it("shows tooltip message when URL is missing and panel is expanded", async () => {
      mockInvoke.mockImplementation(async (cmd: string) => {
        if (cmd === "get_tool_activations") return {};
        if (cmd === "detect_tool_installation") return { installed: true, binary_found: true, config_dir_found: true };
        return undefined;
      });

      render(
        <ToolActivationPanel
          {...defaultProps}
          baseUrlAnthropic="" // Empty anthropic URL
        />,
      );

      await waitFor(() => {
        expect(screen.getAllByRole("switch").length).toBe(2);
      });

      // Expand Claude Code panel
      const claudePanel = screen.getByLabelText("Claude Code 工具面板");
      fireEvent.click(claudePanel);

      await waitFor(() => {
        expect(screen.getByText("此供应商未提供 Anthropic 兼容端点")).toBeInTheDocument();
      });
    });
  });

  describe("toggle activation/deactivation", () => {
    it("calls activate_tool when toggle is turned on", async () => {
      mockInvoke.mockImplementation(async (cmd: string, args?: unknown) => {
        if (cmd === "get_tool_activations") return {};
        if (cmd === "detect_tool_installation") return { installed: true, binary_found: true, config_dir_found: true };
        if (cmd === "activate_tool") return { tool_id: "claude-code", success: true, message: "ok" };
        return undefined;
      });

      render(<ToolActivationPanel {...defaultProps} />);

      await waitFor(() => {
        expect(screen.getAllByRole("switch").length).toBe(2);
      });

      // Toggle on Claude Code
      const claudeSwitch = screen.getByLabelText("启用 Claude Code");
      fireEvent.click(claudeSwitch);

      await waitFor(() => {
        expect(mockInvoke).toHaveBeenCalledWith("activate_tool", {
          providerId: "provider-1",
          toolId: "claude-code",
          model: "deepseek-chat",
        });
      });
    });

    it("calls deactivate_tool when toggle is turned off", async () => {
      mockInvoke.mockImplementation(async (cmd: string) => {
        if (cmd === "get_tool_activations")
          return {
            "claude-code": { provider_id: "provider-1", model: "deepseek-chat" },
          };
        if (cmd === "detect_tool_installation") return { installed: true, binary_found: true, config_dir_found: true };
        if (cmd === "deactivate_tool") return undefined;
        return undefined;
      });

      render(<ToolActivationPanel {...defaultProps} />);

      await waitFor(() => {
        expect(screen.getByText(/● 已启用/)).toBeInTheDocument();
      });

      // Toggle off Claude Code
      const claudeSwitch = screen.getByLabelText("停用 Claude Code");
      fireEvent.click(claudeSwitch);

      await waitFor(() => {
        expect(mockInvoke).toHaveBeenCalledWith("deactivate_tool", {
          toolId: "claude-code",
        });
      });
    });

    it("shows active status when provider is activated for a tool", async () => {
      mockInvoke.mockImplementation(async (cmd: string) => {
        if (cmd === "get_tool_activations")
          return {
            "claude-code": { provider_id: "provider-1", model: "deepseek-chat" },
          };
        if (cmd === "detect_tool_installation") return { installed: true, binary_found: true, config_dir_found: true };
        return undefined;
      });

      render(<ToolActivationPanel {...defaultProps} />);

      await waitFor(() => {
        expect(screen.getByText("● 已启用 · deepseek-chat")).toBeInTheDocument();
      });
    });
  });

  describe("graceful fallback on detection failure", () => {
    it("assumes tool is installed when detection fails", async () => {
      mockInvoke.mockImplementation(async (cmd: string) => {
        if (cmd === "get_tool_activations") return {};
        if (cmd === "detect_tool_installation") throw new Error("Detection failed");
        return undefined;
      });

      render(<ToolActivationPanel {...defaultProps} />);

      await waitFor(() => {
        // Should not show "未安装" since we fallback to installed=true
        expect(screen.queryByText("○ 未安装")).not.toBeInTheDocument();
      });

      // Toggles should be enabled (not disabled due to install check)
      const switches = screen.getAllByRole("switch");
      for (const sw of switches) {
        expect(sw).not.toBeDisabled();
      }
    });
  });
});
