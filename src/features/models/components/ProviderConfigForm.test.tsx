import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { invoke } from "@tauri-apps/api/core";
import { afterEach, describe, expect, it, vi } from "vitest";
import type { ProviderEntryFlat } from "../../../types";
import { ProviderConfigForm } from "./ProviderConfigForm";

afterEach(() => {
  cleanup();
  vi.clearAllMocks();
  vi.useRealTimers();
});

const mockProvider: ProviderEntryFlat = {
  id: "test-id",
  name: "TestProvider",
  base_url_openai: "https://api.example.com/v1",
  base_url_anthropic: "https://api.example.com/anthropic",
  models_url: "https://api.example.com/v1/models",
  api_key: "sk-test-key",
  models: ["model-a", "model-b"],
  default_model: "model-a",
  sort_index: 0,
};

const noModelsUrlProvider: ProviderEntryFlat = {
  ...mockProvider,
  models_url: "",
};

describe("ProviderConfigForm - Auto-Fetch Models UI", () => {
  // Single, unified entry point shared by every agent config (Claude / Codex …).
  const getFetchButton = () => screen.getByRole("button", { name: "获取可用模型列表" });
  const getCodexModelInput = () => screen.getByLabelText("Codex 模型名称") as HTMLInputElement;
  const getClaudeMainModelInput = () => screen.getByLabelText("Claude 主模型") as HTMLInputElement;
  const getModelsUrlInput = () => screen.getByPlaceholderText("https://api.example.com/v1/models") as HTMLInputElement;

  const expandClaudePanel = () => {
    if (!screen.queryByRole("button", { name: "折叠 Claude" })) {
      fireEvent.click(screen.getByRole("button", { name: "展开 Claude" }));
    }
  };

  const expandCodexPanel = () => {
    if (!screen.queryByRole("button", { name: "折叠 Codex" })) {
      fireEvent.click(screen.getByRole("button", { name: "展开 Codex" }));
    }
  };

  it("shows Claude and Codex model panels alongside the unified fetch action", () => {
    render(<ProviderConfigForm provider={mockProvider} onSave={vi.fn()} />);

    expect(screen.getByRole("heading", { name: "Claude" })).toBeInTheDocument();
    expect(screen.getByRole("heading", { name: "Codex" })).toBeInTheDocument();
    expect(getFetchButton()).toBeEnabled();
    expect(getModelsUrlInput()).toHaveValue("https://api.example.com/v1/models");
  });

  it("collapses and expands Claude and Codex model panels", () => {
    render(<ProviderConfigForm provider={mockProvider} onSave={vi.fn()} />);

    expandClaudePanel();
    expandCodexPanel();
    fireEvent.click(screen.getByRole("button", { name: "折叠 Claude" }));
    fireEvent.click(screen.getByRole("button", { name: "折叠 Codex" }));

    expect(screen.queryByLabelText("Claude 主模型")).not.toBeInTheDocument();
    expect(screen.queryByLabelText("Codex 模型名称")).not.toBeInTheDocument();

    fireEvent.click(screen.getByRole("button", { name: "展开 Claude" }));
    fireEvent.click(screen.getByRole("button", { name: "展开 Codex" }));

    expect(screen.getByLabelText("Claude 主模型")).toBeInTheDocument();
    expect(screen.getByLabelText("Codex 模型名称")).toBeInTheDocument();
  });

  it("disables fetching when models_url is empty", () => {
    render(<ProviderConfigForm provider={noModelsUrlProvider} onSave={vi.fn()} />);

    expect(getFetchButton()).toBeDisabled();
  });

  it("disables fetching when api_key is empty", () => {
    const provider = { ...mockProvider, api_key: "" };
    render(<ProviderConfigForm provider={provider} onSave={vi.fn()} />);

    expect(getFetchButton()).toBeDisabled();
  });

  it("shows loading state while fetching models", async () => {
    vi.mocked(invoke).mockReturnValue(new Promise(() => {}));

    render(<ProviderConfigForm provider={mockProvider} onSave={vi.fn()} />);

    fireEvent.click(getFetchButton());

    await waitFor(() => {
      expect(screen.getByText("获取中...")).toBeInTheDocument();
    });
  });

  it("calls fetch_provider_models with the unified models_url and shared api_key", async () => {
    vi.mocked(invoke).mockResolvedValue(["gpt-4", "gpt-3.5-turbo"]);

    render(<ProviderConfigForm provider={mockProvider} onSave={vi.fn()} />);

    fireEvent.click(getFetchButton());

    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith(
        "fetch_provider_models",
        expect.objectContaining({
          url: "https://api.example.com/v1/models",
          apiKey: "sk-test-key",
        }),
      );
    });
  });

  it("injects fetched models into both Claude and Codex datalists without replacing values", async () => {
    vi.mocked(invoke).mockResolvedValue(["gpt-4", "gpt-3.5-turbo", "gpt-4o"]);

    render(<ProviderConfigForm provider={mockProvider} onSave={vi.fn()} />);

    expandCodexPanel();
    expect(getCodexModelInput()).toHaveValue("model-a");
    fireEvent.click(getFetchButton());

    await waitFor(() => {
      expect(screen.getByText(/已更新 3 个/)).toBeInTheDocument();
    });

    expandClaudePanel();
    expect(getCodexModelInput()).toHaveValue("model-a");
    expect(document.querySelector('datalist#codex-model-options option[value="gpt-4"]')).toBeInTheDocument();
    expect(document.querySelector('datalist#claude-model-options option[value="gpt-4"]')).toBeInTheDocument();
    expect(document.querySelector('datalist#claude-model-options option[value="gpt-4o"]')).toBeInTheDocument();
  });

  it("autosaves a fetched option after the user chooses it in the Codex input", async () => {
    vi.mocked(invoke).mockResolvedValue(["new-model-1", "new-model-2"]);
    const onSave = vi.fn().mockResolvedValue(undefined);

    render(<ProviderConfigForm provider={mockProvider} onSave={onSave} />);

    expandCodexPanel();
    expect(getCodexModelInput()).toHaveValue("model-a");
    fireEvent.click(getFetchButton());

    await waitFor(() => {
      expect(document.querySelector('datalist#codex-model-options option[value="new-model-1"]')).toBeInTheDocument();
    });

    fireEvent.change(getCodexModelInput(), { target: { value: "new-model-1" } });
    expect(getCodexModelInput()).toHaveValue("new-model-1");

    await waitFor(
      () => {
        expect(onSave).toHaveBeenCalledWith(
          expect.objectContaining({
            default_model: "new-model-1",
            models: ["model-a", "model-b", "new-model-1", "new-model-2"],
          }),
        );
      },
      { timeout: 1500 },
    );
  });

  it("shows inline error on fetch failure and preserves existing models", async () => {
    vi.mocked(invoke).mockRejectedValue("Network timeout");

    render(<ProviderConfigForm provider={mockProvider} onSave={vi.fn()} />);

    fireEvent.click(getFetchButton());

    await waitFor(() => {
      expect(screen.getByText(/获取模型列表失败/)).toBeInTheDocument();
    });

    expect(getCodexModelInput()).toHaveValue("model-a");
  });

  it("shows empty state when fetch returns no models", async () => {
    vi.mocked(invoke).mockResolvedValue([]);

    render(<ProviderConfigForm provider={mockProvider} onSave={vi.fn()} />);

    fireEvent.click(getFetchButton());

    await waitFor(() => {
      expect(screen.getByText("未发现可用模型")).toBeInTheDocument();
    });
  });

  it("autosaves the edited models_url back through onSave", async () => {
    const onSave = vi.fn().mockResolvedValue(undefined);
    render(<ProviderConfigForm provider={mockProvider} onSave={onSave} />);

    fireEvent.change(getModelsUrlInput(), { target: { value: "https://api.example.com/custom/models" } });

    await waitFor(
      () => {
        expect(onSave).toHaveBeenCalledWith(
          expect.objectContaining({
            models_url: "https://api.example.com/custom/models",
          }),
        );
      },
      { timeout: 1500 },
    );
  });

  it("autosaves Claude model fields in provider meta", async () => {
    const onSave = vi.fn().mockResolvedValue(undefined);

    render(<ProviderConfigForm provider={mockProvider} onSave={onSave} />);

    expandClaudePanel();
    fireEvent.change(screen.getByLabelText("Claude 主模型"), { target: { value: "claude-sonnet-custom" } });
    fireEvent.change(screen.getByLabelText("Claude Haiku 默认模型"), { target: { value: "claude-haiku-custom" } });

    await waitFor(
      () => {
        expect(onSave).toHaveBeenCalledWith(
          expect.objectContaining({
            models: ["model-a", "model-b", "claude-sonnet-custom", "claude-haiku-custom"],
            meta: expect.objectContaining({
              claude_main_model: "claude-sonnet-custom",
              claude_haiku_model: "claude-haiku-custom",
            }),
          }),
        );
      },
      { timeout: 1500 },
    );
  });

  it("does not autosave on initial mount", async () => {
    const onSave = vi.fn().mockResolvedValue(undefined);
    render(<ProviderConfigForm provider={mockProvider} onSave={onSave} />);

    await new Promise((resolve) => setTimeout(resolve, 700));
    expect(onSave).not.toHaveBeenCalled();
  });

});
