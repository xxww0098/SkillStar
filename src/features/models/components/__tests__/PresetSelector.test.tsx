import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { ProviderEntryFlat } from "../../../../types";

// Mock useProvidersFlat hook
const mockCreateProvider = vi.fn();

vi.mock("../../hooks/useProvidersFlat", () => ({
  useProvidersFlat: () => ({
    createProvider: mockCreateProvider,
  }),
}));

import { PresetSelector } from "../PresetSelector";

afterEach(cleanup);

describe("PresetSelector", () => {
  const mockOnProviderCreated = vi.fn();
  const mockOnCancel = vi.fn();

  beforeEach(() => {
    vi.clearAllMocks();
  });

  // ── Category display tests (Requirement 4.1) ──

  describe("preset categories display", () => {
    it("displays all 3 categories: 国内模型, 官方中转站, OpenAI 兼容", () => {
      render(<PresetSelector onProviderCreated={mockOnProviderCreated} />);

      // Category headers are h4 elements
      expect(screen.getByRole("heading", { level: 4, name: "国内模型" })).toBeInTheDocument();
      expect(screen.getByRole("heading", { level: 4, name: "官方中转站" })).toBeInTheDocument();
      expect(screen.getByRole("heading", { level: 4, name: "OpenAI 兼容" })).toBeInTheDocument();
    });

    it("displays domestic presets under 国内模型 category", () => {
      render(<PresetSelector onProviderCreated={mockOnProviderCreated} />);

      expect(screen.getByText("DeepSeek")).toBeInTheDocument();
      expect(screen.getByText("MiniMax")).toBeInTheDocument();
      expect(screen.getByText("通义千问")).toBeInTheDocument();
      expect(screen.getByText("智谱 GLM")).toBeInTheDocument();
      expect(screen.getByText("火山方舟")).toBeInTheDocument();
      expect(screen.getByText("小米 MiMo")).toBeInTheDocument();
    });

    it("displays relay presets under 官方中转站 category", () => {
      render(<PresetSelector onProviderCreated={mockOnProviderCreated} />);

      expect(screen.getByText("OpenRouter")).toBeInTheDocument();
      expect(screen.getByText("SiliconFlow")).toBeInTheDocument();
    });
  });

  // ── Form display after selecting a preset (Requirement 4.5) ──

  describe("form display after preset selection", () => {
    it("shows form with pre-filled Base URL when a preset is clicked", () => {
      render(<PresetSelector onProviderCreated={mockOnProviderCreated} />);

      fireEvent.click(screen.getByText("DeepSeek"));

      // Should show the form with DeepSeek's base URL pre-filled
      const openaiInput = screen.getByDisplayValue("https://api.deepseek.com/v1");
      expect(openaiInput).toBeInTheDocument();

      // Should show Anthropic URL too
      const anthropicInput = screen.getByDisplayValue("https://api.deepseek.com/anthropic");
      expect(anthropicInput).toBeInTheDocument();

      // Should show API Key input
      expect(screen.getByPlaceholderText("sk-...")).toBeInTheDocument();
    });

    it("shows preset name in the form header", () => {
      render(<PresetSelector onProviderCreated={mockOnProviderCreated} />);

      fireEvent.click(screen.getByText("MiniMax"));

      // Form header should show the preset name
      expect(screen.getByRole("heading", { level: 3, name: "MiniMax" })).toBeInTheDocument();
    });

    it("does not show hard-coded preset model chips", () => {
      render(<PresetSelector onProviderCreated={mockOnProviderCreated} />);

      fireEvent.click(screen.getByText("DeepSeek"));

      expect(screen.queryByText("预设模型")).not.toBeInTheDocument();
      expect(screen.queryByText("deepseek-chat")).not.toBeInTheDocument();
      expect(screen.queryByText("deepseek-coder")).not.toBeInTheDocument();
      expect(screen.queryByText("deepseek-reasoner")).not.toBeInTheDocument();
    });
  });

  // ── Form submission tests ──

  describe("form submission", () => {
    it("shows error when submitting with empty API Key", async () => {
      render(<PresetSelector onProviderCreated={mockOnProviderCreated} />);

      fireEvent.click(screen.getByText("DeepSeek"));

      // The "创建" button should be disabled when API Key is empty
      const createButton = screen.getByRole("button", { name: "创建" });
      expect(createButton).toBeDisabled();
      expect(mockCreateProvider).not.toHaveBeenCalled();
    });

    it("calls onProviderCreated on successful creation", async () => {
      const createdProvider: ProviderEntryFlat = {
        id: "new-uuid",
        name: "DeepSeek",
        base_url_openai: "https://api.deepseek.com/v1",
        base_url_anthropic: "https://api.deepseek.com/anthropic",
        models_url: "https://api.deepseek.com/v1/models",
        api_key: "sk-test-key",
        models: [],
        default_model: "",
        sort_index: 0,
        preset_id: "deepseek",
        icon_color: "#4D6BFE",
      };

      mockCreateProvider.mockResolvedValue(createdProvider);

      render(<PresetSelector onProviderCreated={mockOnProviderCreated} />);

      fireEvent.click(screen.getByText("DeepSeek"));

      // Enter API Key
      const apiKeyInput = screen.getByPlaceholderText("sk-...");
      fireEvent.change(apiKeyInput, { target: { value: "sk-test-key" } });

      // Submit
      fireEvent.click(screen.getByText("创建"));

      await waitFor(() => {
        expect(mockCreateProvider).toHaveBeenCalledWith(
          expect.objectContaining({
            name: "DeepSeek",
            base_url_openai: "https://api.deepseek.com/v1",
            base_url_anthropic: "https://api.deepseek.com/anthropic",
            api_key: "sk-test-key",
            models: [],
            default_model: "",
            preset_id: "deepseek",
          }),
        );
      });

      await waitFor(() => {
        expect(mockOnProviderCreated).toHaveBeenCalledWith(createdProvider);
      });
    });

    it("shows inline error and preserves input on failed creation", async () => {
      mockCreateProvider.mockRejectedValue(new Error("Provider name already exists"));

      render(<PresetSelector onProviderCreated={mockOnProviderCreated} />);

      fireEvent.click(screen.getByText("DeepSeek"));

      // Enter API Key
      const apiKeyInput = screen.getByPlaceholderText("sk-...");
      fireEvent.change(apiKeyInput, { target: { value: "sk-my-key" } });

      // Submit
      fireEvent.click(screen.getByText("创建"));

      await waitFor(() => {
        expect(screen.getByText("Provider name already exists")).toBeInTheDocument();
      });

      // Input should be preserved
      expect(screen.getByDisplayValue("sk-my-key")).toBeInTheDocument();
      expect(mockOnProviderCreated).not.toHaveBeenCalled();
    });
  });

  // ── Navigation tests ──

  describe("navigation", () => {
    it("goes back to preset grid when '返回' button is clicked", () => {
      render(<PresetSelector onProviderCreated={mockOnProviderCreated} />);

      // Select a preset to go to form
      fireEvent.click(screen.getByText("DeepSeek"));
      expect(screen.getByPlaceholderText("sk-...")).toBeInTheDocument();

      // Click 返回 button
      fireEvent.click(screen.getByText("返回"));

      // Should be back to the grid — check category headers
      expect(screen.getByRole("heading", { level: 4, name: "国内模型" })).toBeInTheDocument();
      expect(screen.getByRole("heading", { level: 4, name: "官方中转站" })).toBeInTheDocument();
      expect(screen.getByRole("heading", { level: 4, name: "OpenAI 兼容" })).toBeInTheDocument();
    });

    it("goes back to preset grid when back arrow is clicked", () => {
      render(<PresetSelector onProviderCreated={mockOnProviderCreated} />);

      fireEvent.click(screen.getByText("DeepSeek"));

      // Click the back arrow button
      const backButton = screen.getByLabelText("返回预设列表");
      fireEvent.click(backButton);

      // Should be back to the grid
      expect(screen.getByRole("heading", { level: 4, name: "国内模型" })).toBeInTheDocument();
    });

    it("calls onCancel when cancel button is clicked in grid view", () => {
      render(<PresetSelector onProviderCreated={mockOnProviderCreated} onCancel={mockOnCancel} />);

      fireEvent.click(screen.getByText("取消"));

      expect(mockOnCancel).toHaveBeenCalled();
    });
  });
});
