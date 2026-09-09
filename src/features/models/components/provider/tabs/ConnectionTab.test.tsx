import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { copyToClipboard } from "../../../../../lib/utils";
import type { ProviderForm } from "../../../hooks/useProviderForm";
import { ConnectionTab } from "./ConnectionTab";

vi.mock("react-i18next", () => ({ useTranslation: () => ({ t: (key: string) => key }) }));

vi.mock("../../../../../lib/utils", async (importOriginal) => ({
  ...(await importOriginal<typeof import("../../../../../lib/utils")>()),
  copyToClipboard: vi.fn().mockResolvedValue(true),
}));

beforeEach(() => vi.clearAllMocks());

function form(overrides: Partial<ProviderForm> = {}, values: Partial<ProviderForm["values"]> = {}): ProviderForm {
  return {
    values: {
      name: "",
      apiKey: "secret-key",
      baseUrlOpenai: "https://api.example.com/v1",
      baseUrlAnthropic: "",
      modelsUrl: "https://api.example.com/v1/models",
      ...values,
    },
    setField: vi.fn(),
    validationErrorCode: null,
    ...overrides,
  } as unknown as ProviderForm;
}

describe("ConnectionTab", () => {
  it("associates labels and renders the invalid field inline", () => {
    render(<ConnectionTab form={form({ validationErrorCode: "nameRequired" })} />);

    const name = screen.getByLabelText(/models.connectionTab.name/);
    expect(name).toHaveAttribute("aria-invalid", "true");
    expect(screen.getByRole("alert")).toHaveTextContent("models.errors.nameRequired");
  });

  it("keeps existing endpoint values when additional settings are collapsed", async () => {
    const providerForm = form();
    render(<ConnectionTab form={providerForm} />);
    const openai = screen.getByLabelText("models.connectionTab.openaiEndpoint");
    const modelsUrl = screen.getByLabelText("models.connectionTab.modelsUrl");
    const details = openai.closest("details");
    expect(details).toHaveAttribute("open");
    expect(modelsUrl.closest("details")).toBe(details);
    const anthropic = screen.getByLabelText("models.connectionTab.anthropicEndpoint");
    expect(anthropic.closest("details")).toBeNull();
    expect(anthropic.compareDocumentPosition(openai) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();

    fireEvent.click(screen.getByText("models.claudeWorkbench.additionalSettings"));
    await waitFor(() => expect(details).not.toHaveAttribute("open"));
    expect(openai).toHaveValue("https://api.example.com/v1");
    expect(modelsUrl).toHaveValue("https://api.example.com/v1/models");
    expect(providerForm.setField).not.toHaveBeenCalled();

    fireEvent.click(screen.getByText("models.claudeWorkbench.additionalSettings"));
    await waitFor(() => expect(details).toHaveAttribute("open"));
    expect(openai).toHaveValue("https://api.example.com/v1");
    expect(modelsUrl).toHaveValue("https://api.example.com/v1/models");
  });

  it.each([
    ["invalidOpenaiUrl", "baseUrlOpenai", "models.connectionTab.openaiEndpoint"],
    ["invalidModelsUrl", "modelsUrl", "models.connectionTab.modelsUrl"],
  ] as const)("expands additional settings when %s needs attention", async (code, field, label) => {
    const { rerender } = render(<ConnectionTab form={form({}, { baseUrlOpenai: "", modelsUrl: "" })} />);
    const details = screen.getByLabelText(label).closest("details");
    expect(details).not.toHaveAttribute("open");

    rerender(
      <ConnectionTab
        form={form({ validationErrorCode: code }, { baseUrlOpenai: "", modelsUrl: "", [field]: "invalid-url" })}
      />,
    );

    await waitFor(() => expect(details).toHaveAttribute("open"));
    expect(screen.getByLabelText(label)).toHaveAttribute("aria-invalid", "true");
    expect(screen.getByRole("alert")).toHaveTextContent("models.errors." + code);
  });

  it("does not clear an empty API-key projection when editing another field", () => {
    const providerForm = form({}, { apiKey: "", baseUrlOpenai: "" });
    render(<ConnectionTab form={providerForm} />);
    expect(screen.getByRole("button", { name: "models.connectionTab.copy" })).toBeDisabled();
    expect(providerForm.setField).not.toHaveBeenCalled();

    fireEvent.click(screen.getByText("models.claudeWorkbench.additionalSettings"));
    fireEvent.change(screen.getByLabelText(/models.connectionTab.name/), { target: { value: "Renamed" } });
    expect(providerForm.setField).toHaveBeenCalledExactlyOnceWith("name", "Renamed");
  });

  it("preserves API-key editing, copying and visibility controls", async () => {
    const providerForm = form();
    render(<ConnectionTab form={providerForm} />);

    const apiKey = screen.getByLabelText("API Key");
    expect(apiKey).toHaveAttribute("type", "password");
    expect(apiKey).toHaveAttribute("autocomplete", "off");
    fireEvent.click(screen.getByRole("button", { name: "models.connectionTab.show" }));
    expect(apiKey).toHaveAttribute("type", "text");
    fireEvent.click(screen.getByRole("button", { name: "models.connectionTab.hide" }));
    expect(apiKey).toHaveAttribute("type", "password");
    fireEvent.click(screen.getByRole("button", { name: "models.connectionTab.copy" }));
    await waitFor(() => expect(copyToClipboard).toHaveBeenCalledExactlyOnceWith("secret-key"));
    fireEvent.change(apiKey, { target: { value: "new-secret" } });
    expect(providerForm.setField).toHaveBeenCalledExactlyOnceWith("apiKey", "new-secret");

    expect(screen.getByLabelText("models.connectionTab.anthropicEndpoint")).toBeInTheDocument();
    expect(screen.getByLabelText("models.connectionTab.openaiEndpoint").closest("details")).toHaveAttribute("open");
  });
});
