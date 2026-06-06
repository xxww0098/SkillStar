import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import type { AiConfig } from "../../../types";
import { AiProviderSection } from "./AiProviderSection";

vi.mock("react-i18next", () => ({
  useTranslation: () => ({
    t: (_key: string, options?: { defaultValue?: string }) => options?.defaultValue ?? _key,
  }),
}));

vi.mock("../components/AppAiModelsPicker", () => ({
  AppAiModelsPicker: ({ config }: { config: AiConfig }) => <div data-testid="models-picker">{config.api_format}</div>,
}));

const baseConfig: AiConfig = {
  enabled: true,
  api_format: "local",
  provider_ref: null,
  base_url: "http://127.0.0.1:11434/v1",
  api_key: "",
  model: "llama3.1:8b",
  target_language: "zh-CN",
  context_window_k: 128,
  max_concurrent_requests: 4,
  openai_preset: { base_url: "", api_key: "", model: "" },
  anthropic_preset: { base_url: "", api_key: "", model: "" },
  local_preset: { base_url: "http://127.0.0.1:11434/v1", api_key: "", model: "llama3.1:8b" },
};

function renderSection(config: AiConfig, onConfigChange = vi.fn()) {
  const view = render(
    <AiProviderSection
      localAiConfig={config}
      ready
      aiExpanded
      aiSaving={false}
      aiSaved={false}
      aiTesting={false}
      aiTestResult={null}
      aiTestLatency={null}
      onToggleExpanded={vi.fn()}
      onEnabledChange={vi.fn()}
      onConfigChange={onConfigChange}
      onTestConnection={vi.fn()}
    />,
  );
  return { ...view, onConfigChange };
}

describe("AiProviderSection", () => {
  it("allows switching from local Ollama to Models provider before a provider is selected", () => {
    const { onConfigChange, rerender } = renderSection(baseConfig);

    fireEvent.click(screen.getByRole("button", { name: "Models 供应商" }));

    expect(onConfigChange).toHaveBeenCalledWith(
      expect.objectContaining({ api_format: "anthropic", provider_ref: null }),
    );

    const nextConfig = onConfigChange.mock.calls[0][0] as AiConfig;
    rerender(
      <AiProviderSection
        localAiConfig={nextConfig}
        ready
        aiExpanded
        aiSaving={false}
        aiSaved={false}
        aiTesting={false}
        aiTestResult={null}
        aiTestLatency={null}
        onToggleExpanded={vi.fn()}
        onEnabledChange={vi.fn()}
        onConfigChange={onConfigChange}
        onTestConnection={vi.fn()}
      />,
    );

    expect(screen.getByTestId("models-picker")).toHaveTextContent("anthropic");
  });
});
