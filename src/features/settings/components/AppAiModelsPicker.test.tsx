import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { AiConfig, ProviderEntryFlat } from "../../../types";
import { AppAiModelsPicker } from "./AppAiModelsPicker";

const mocks = vi.hoisted(() => ({
  providers: [] as ProviderEntryFlat[],
  updateProvider: vi.fn(),
  setAppAiProvider: vi.fn(),
  fetchModels: vi.fn(),
}));

vi.mock("react-i18next", () => ({
  useTranslation: () => ({
    t: (_key: string, options?: { defaultValue?: string }) => options?.defaultValue ?? _key,
  }),
}));

vi.mock("sonner", () => ({
  toast: {
    error: vi.fn(),
    success: vi.fn(),
  },
}));

vi.mock("../../models/components/shared/ProviderBrandIcon", () => ({
  ProviderBrandIcon: () => <span data-testid="provider-icon" />,
}));

vi.mock("../../models/api/appAi", () => ({
  useAppAiProvider: () => ({
    setAppAiProvider: mocks.setAppAiProvider,
    isSetting: false,
  }),
}));

vi.mock("../../models/api/modelCatalog", () => ({
  useModelFetch: () => ({
    fetchModels: mocks.fetchModels,
    isLoading: false,
  }),
}));

vi.mock("../../models/hooks/useProvidersFlat", () => ({
  useProvidersFlat: () => ({
    providers: mocks.providers,
    isLoading: false,
    updateProvider: mocks.updateProvider,
  }),
}));

const provider: ProviderEntryFlat = {
  id: "deepseek",
  name: "DeepSeek",
  base_url_openai: "https://api.deepseek.com/v1",
  base_url_anthropic: "https://api.deepseek.com/anthropic",
  models_url: "https://api.deepseek.com/v1/models",
  api_key: "sk-test",
  models: ["deepseek-chat", "deepseek-coder"],
  default_model: "deepseek-chat",
  sort_index: 0,
  preset_id: "deepseek",
  icon_color: "#4f7cff",
  meta: {},
};

const config: AiConfig = {
  enabled: true,
  api_format: "anthropic",
  provider_ref: { app_id: "claude", provider_id: "deepseek" },
  base_url: "",
  api_key: "",
  model: "deepseek-chat",
  target_language: "zh-CN",
  context_window_k: 128,
  max_concurrent_requests: 4,
  openai_preset: { base_url: "", api_key: "", model: "" },
  anthropic_preset: { base_url: "", api_key: "", model: "" },
  local_preset: { base_url: "http://127.0.0.1:11434/v1", api_key: "", model: "llama3.1:8b" },
};

describe("AppAiModelsPicker", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    mocks.providers = [provider];
    mocks.updateProvider.mockImplementation(async (_id: string, patch: Partial<ProviderEntryFlat>) => ({
      ...provider,
      ...patch,
    }));
    mocks.setAppAiProvider.mockResolvedValue(undefined);
    mocks.fetchModels.mockResolvedValue(["deepseek-chat", "deepseek-coder"]);
  });

  it("removes the local Ollama fallback button", () => {
    render(<AppAiModelsPicker config={config} onConfigChange={vi.fn()} />);

    expect(screen.queryByText("改回本地 Ollama")).not.toBeInTheDocument();
  });

  it("fetches models into the selected provider", async () => {
    render(<AppAiModelsPicker config={config} onConfigChange={vi.fn()} />);

    fireEvent.click(screen.getByRole("button", { name: "获取模型" }));

    await waitFor(() => {
      expect(mocks.fetchModels).toHaveBeenCalledWith("https://api.deepseek.com/v1/models", "sk-test");
    });
    expect(mocks.updateProvider).toHaveBeenCalledWith(
      "deepseek",
      expect.objectContaining({
        default_model: "deepseek-chat",
        models: ["deepseek-chat", "deepseek-coder"],
      }),
    );
  });

  it("saves the chosen model for application AI", async () => {
    const onConfigChange = vi.fn();
    render(<AppAiModelsPicker config={config} onConfigChange={onConfigChange} />);

    fireEvent.change(screen.getByLabelText("模型"), { target: { value: "deepseek-coder" } });
    fireEvent.click(screen.getByRole("button", { name: /保存连接配置/ }));

    await waitFor(() => {
      expect(mocks.updateProvider).toHaveBeenCalledWith(
        "deepseek",
        expect.objectContaining({
          default_model: "deepseek-coder",
          meta: expect.objectContaining({ claude_main_model: "deepseek-coder" }),
        }),
      );
    });
    expect(onConfigChange).toHaveBeenCalledWith(expect.objectContaining({ model: "deepseek-coder" }));
  });

  it("lets an incomplete provider be completed inline and bound", async () => {
    const incompleteProvider = {
      ...provider,
      id: "custom",
      name: "Custom Gateway",
      base_url_openai: "",
      base_url_anthropic: "",
      models_url: "",
      api_key: "",
      default_model: "",
      models: [],
      meta: {},
    };
    mocks.providers = [incompleteProvider];
    mocks.updateProvider.mockImplementation(async (_id: string, patch: Partial<ProviderEntryFlat>) => ({
      ...incompleteProvider,
      ...patch,
    }));
    const onConfigChange = vi.fn();

    render(
      <AppAiModelsPicker
        config={{ ...config, provider_ref: null, api_format: "openai", model: "" }}
        onConfigChange={onConfigChange}
      />,
    );

    expect(screen.getByText("缺 Key")).toBeInTheDocument();

    fireEvent.click(screen.getByRole("button", { name: /Custom Gateway/ }));
    fireEvent.change(screen.getByLabelText("API Key"), { target: { value: "sk-custom" } });
    fireEvent.change(screen.getByLabelText("OpenAI Base URL"), { target: { value: "https://api.example.com/v1" } });
    fireEvent.change(screen.getByLabelText("模型"), { target: { value: "gpt-test" } });
    fireEvent.click(screen.getByRole("button", { name: /保存连接配置/ }));

    await waitFor(() => {
      expect(mocks.updateProvider).toHaveBeenCalledWith(
        "custom",
        expect.objectContaining({
          api_key: "sk-custom",
          base_url_openai: "https://api.example.com/v1",
          default_model: "gpt-test",
        }),
      );
    });
    expect(mocks.setAppAiProvider).toHaveBeenCalledWith("codex", "custom", "Custom Gateway");
    expect(onConfigChange).toHaveBeenCalledWith(
      expect.objectContaining({
        api_format: "openai",
        model: "gpt-test",
        provider_ref: { app_id: "codex", provider_id: "custom" },
      }),
    );
  });
});
