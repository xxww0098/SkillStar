import { act, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { ProviderEntryFlat } from "../../../../../types";
import { ClaudeProviderCreate } from "./ClaudeProviderCreate";

const { createProvider, activateTool } = vi.hoisted(() => ({
  createProvider: vi.fn(),
  activateTool: vi.fn(),
}));

vi.mock("../../../hooks/useProvidersFlat", () => ({
  useProvidersFlat: () => ({ createProvider, activateTool }),
}));

vi.mock("react-i18next", () => ({
  useTranslation: () => ({
    t: (key: string, options?: { message?: string }) => (options?.message ? key + ": " + options.message : key),
  }),
}));

const created: ProviderEntryFlat = {
  id: "created-provider",
  name: "Team Claude",
  api_key: "test-secret",
  base_url_anthropic: "https://claude.example.com/anthropic",
  base_url_openai: "",
  models_url: "https://claude.example.com/v1/models",
  default_model: "claude-test-model",
  models: ["claude-test-model"],
  sort_index: 4,
};

function fillRequiredFields() {
  fireEvent.change(screen.getByLabelText(/models.connectionTab.name/), { target: { value: "  Team Claude  " } });
  fireEvent.change(screen.getByLabelText(/models.claudeCreate.anthropicBaseUrl/), {
    target: { value: "  https://claude.example.com/anthropic  " },
  });
  fireEvent.change(screen.getByLabelText(/models.claudeCreate.apiKey/), { target: { value: "  test-secret  " } });
}

beforeEach(() => {
  vi.clearAllMocks();
  createProvider.mockResolvedValue(created);
});

describe("ClaudeProviderCreate", () => {
  it.each([
    ["models.connectionTab.name", "models.errors.nameRequired"],
    ["models.claudeCreate.anthropicBaseUrl", "models.claudeCreate.anthropicUrlRequired"],
    ["models.claudeCreate.apiKey", "models.claudeCreate.apiKeyRequired"],
  ])("rejects a whitespace-only required field: %s", (label, errorKey) => {
    const onCreated = vi.fn();
    render(<ClaudeProviderCreate onClose={vi.fn()} onCreated={onCreated} />);
    fillRequiredFields();
    const input = screen.getByLabelText(new RegExp(label));
    fireEvent.change(input, { target: { value: "   " } });
    fireEvent.blur(input);

    expect(input).toHaveAttribute("aria-invalid", "true");
    expect(screen.getByRole("alert")).toHaveTextContent(errorKey);
    expect(screen.getByRole("button", { name: "models.claudeCreate.save" })).toBeDisabled();
    fireEvent.submit(screen.getByRole("form"));
    expect(createProvider).not.toHaveBeenCalled();
    expect(onCreated).not.toHaveBeenCalled();
  });

  it.each([
    ["models.claudeCreate.anthropicBaseUrl", "ftp://example.com", "models.errors.invalidAnthropicUrl"],
    ["models.claudeCreate.anthropicBaseUrl", "not-a-url", "models.errors.invalidAnthropicUrl"],
    ["models.claudeCreate.anthropicBaseUrl", "https://", "models.errors.invalidAnthropicUrl"],
    ["models.connectionTab.modelsUrl", "file:///models.json", "models.errors.invalidModelsUrl"],
    ["models.connectionTab.modelsUrl", "not-a-url", "models.errors.invalidModelsUrl"],
  ])("rejects an invalid HTTP(S) endpoint: %s = %s", (label, url, errorKey) => {
    render(<ClaudeProviderCreate onClose={vi.fn()} onCreated={vi.fn()} />);
    fillRequiredFields();
    const input = screen.getByLabelText(new RegExp(label));
    fireEvent.change(input, { target: { value: url } });
    fireEvent.blur(input);

    expect(input).toHaveAttribute("aria-invalid", "true");
    expect(screen.getByRole("alert")).toHaveTextContent(errorKey);
    expect(screen.getByRole("button", { name: "models.claudeCreate.save" })).toBeDisabled();
    fireEvent.submit(screen.getByRole("form"));
    expect(createProvider).not.toHaveBeenCalled();
  });

  it("keeps the key private by default and leaves without creating", () => {
    const onClose = vi.fn();
    const onCreated = vi.fn();
    render(<ClaudeProviderCreate onClose={onClose} onCreated={onCreated} />);
    const key = screen.getByLabelText(/models.claudeCreate.apiKey/);
    expect(key).toHaveAttribute("type", "password");
    expect(key).toHaveAttribute("autocomplete", "off");
    fireEvent.change(key, { target: { value: "private-draft" } });
    fireEvent.click(screen.getByRole("button", { name: "models.connectionTab.show" }));
    expect(key).toHaveAttribute("type", "text");
    expect(key).toHaveValue("private-draft");
    fireEvent.click(screen.getByRole("button", { name: "models.connectionTab.hide" }));
    expect(key).toHaveAttribute("type", "password");
    fireEvent.click(screen.getByRole("button", { name: "models.common.back" }));

    expect(onClose).toHaveBeenCalledOnce();
    expect(createProvider).not.toHaveBeenCalled();
    expect(onCreated).not.toHaveBeenCalled();
  });

  it.each([
    "http://localhost:8080/anthropic",
    "https://claude.example.com/anthropic",
  ])("allows %s with the optional fields blank", async (url) => {
    const onCreated = vi.fn();
    render(<ClaudeProviderCreate onClose={vi.fn()} onCreated={onCreated} />);
    fillRequiredFields();
    fireEvent.change(screen.getByLabelText(/models.claudeCreate.anthropicBaseUrl/), { target: { value: url } });
    fireEvent.change(screen.getByLabelText("models.claudeCreate.defaultModel"), { target: { value: "   " } });
    fireEvent.change(screen.getByLabelText("models.connectionTab.modelsUrl"), { target: { value: "   " } });
    expect(screen.getByRole("button", { name: "models.claudeCreate.save" })).toBeEnabled();
    fireEvent.submit(screen.getByRole("form"));

    await waitFor(() => expect(onCreated).toHaveBeenCalledOnce());
    expect(createProvider).toHaveBeenCalledExactlyOnceWith({
      id: "",
      name: "Team Claude",
      api_key: "test-secret",
      base_url_anthropic: url,
      base_url_openai: "",
      models_url: "",
      default_model: "",
      models: [],
    });
  });

  it("prevents repeated submissions while creation is pending", async () => {
    let finish!: (provider: ProviderEntryFlat) => void;
    const pending = new Promise<ProviderEntryFlat>((resolve) => {
      finish = resolve;
    });
    createProvider.mockReturnValueOnce(pending);
    const onCreated = vi.fn();
    render(<ClaudeProviderCreate onClose={vi.fn()} onCreated={onCreated} />);
    fillRequiredFields();
    const form = screen.getByRole("form");

    act(() => {
      fireEvent.submit(form);
      fireEvent.submit(form);
    });
    expect(createProvider).toHaveBeenCalledOnce();
    expect(onCreated).not.toHaveBeenCalled();
    expect(form).toHaveAttribute("aria-busy", "true");
    expect(screen.getByRole("button", { name: "models.claudeCreate.saving" })).toBeDisabled();
    expect(screen.getByRole("button", { name: "models.common.back" })).toBeDisabled();
    expect(screen.getByLabelText(/models.claudeCreate.apiKey/)).toBeDisabled();
    fireEvent.submit(form);
    expect(createProvider).toHaveBeenCalledOnce();

    await act(async () => finish(created));
    expect(onCreated).toHaveBeenCalledExactlyOnceWith(created);
    expect(form).toHaveAttribute("aria-busy", "false");
  });

  it("preserves the complete draft after a failed save and allows retry", async () => {
    createProvider.mockRejectedValueOnce(new Error("Store unavailable"));
    const onCreated = vi.fn();
    render(<ClaudeProviderCreate onClose={vi.fn()} onCreated={onCreated} />);
    fillRequiredFields();
    fireEvent.change(screen.getByLabelText("models.claudeCreate.defaultModel"), {
      target: { value: "draft-model" },
    });
    fireEvent.change(screen.getByLabelText("models.connectionTab.modelsUrl"), {
      target: { value: "https://claude.example.com/models" },
    });
    fireEvent.click(screen.getByRole("button", { name: "models.claudeCreate.save" }));

    expect(await screen.findByRole("alert")).toHaveTextContent("models.claudeCreate.saveFailed: Store unavailable");
    expect(onCreated).not.toHaveBeenCalled();
    expect(screen.getByLabelText(/models.connectionTab.name/)).toHaveValue("  Team Claude  ");
    expect(screen.getByLabelText(/models.claudeCreate.anthropicBaseUrl/)).toHaveValue(
      "https://claude.example.com/anthropic",
    );
    expect(screen.getByLabelText(/models.claudeCreate.apiKey/)).toHaveValue("  test-secret  ");
    expect(screen.getByLabelText(/models.claudeCreate.apiKey/)).toHaveAttribute("type", "password");
    expect(screen.getByLabelText("models.claudeCreate.defaultModel")).toHaveValue("draft-model");
    expect(screen.getByLabelText("models.connectionTab.modelsUrl")).toHaveValue("https://claude.example.com/models");
    expect(screen.getByRole("button", { name: "models.claudeCreate.save" })).toBeEnabled();

    fireEvent.click(screen.getByRole("button", { name: "models.claudeCreate.save" }));
    await waitFor(() => expect(onCreated).toHaveBeenCalledExactlyOnceWith(created));
    expect(createProvider).toHaveBeenCalledTimes(2);
    expect(createProvider.mock.calls[1]).toEqual(createProvider.mock.calls[0]);
    expect(screen.queryByRole("alert")).not.toBeInTheDocument();
  });

  it("creates a trimmed Anthropic connection without a preset or automatic binding", async () => {
    const onCreated = vi.fn();
    const onClose = vi.fn();
    render(<ClaudeProviderCreate onClose={onClose} onCreated={onCreated} />);
    fillRequiredFields();
    fireEvent.change(screen.getByLabelText("models.claudeCreate.defaultModel"), {
      target: { value: "  claude-test-model  " },
    });
    fireEvent.change(screen.getByLabelText("models.connectionTab.modelsUrl"), {
      target: { value: "  https://claude.example.com/v1/models  " },
    });

    fireEvent.click(screen.getByRole("button", { name: "models.claudeCreate.save" }));

    await waitFor(() => expect(onCreated).toHaveBeenCalledWith(created));
    expect(createProvider).toHaveBeenCalledExactlyOnceWith({
      id: "",
      name: "Team Claude",
      api_key: "test-secret",
      base_url_anthropic: "https://claude.example.com/anthropic",
      base_url_openai: "",
      models_url: "https://claude.example.com/v1/models",
      default_model: "claude-test-model",
      models: ["claude-test-model"],
    });
    expect(activateTool).not.toHaveBeenCalled();
    expect(onClose).not.toHaveBeenCalled();
  });
});
