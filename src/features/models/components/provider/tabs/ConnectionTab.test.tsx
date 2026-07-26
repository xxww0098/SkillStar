import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import type { ProviderForm } from "../../../hooks/useProviderForm";
import { ConnectionTab } from "./ConnectionTab";

function form(overrides: Partial<ProviderForm> = {}): ProviderForm {
  return {
    values: {
      name: "",
      apiKey: "secret-key",
      baseUrlOpenai: "https://api.example.com/v1",
      baseUrlAnthropic: "",
      modelsUrl: "https://api.example.com/v1/models",
    },
    setField: vi.fn(),
    validationErrorCode: null,
    ...overrides,
  } as unknown as ProviderForm;
}

describe("ConnectionTab", () => {
  it("associates labels and renders the invalid field inline", () => {
    render(<ConnectionTab form={form({ validationErrorCode: "nameRequired" })} />);

    const name = screen.getByLabelText(/名称/);
    expect(name).toHaveAttribute("aria-invalid", "true");
    expect(screen.getByRole("alert")).toHaveTextContent("供应商名称不能为空");
  });

  it("reveals the API key and progressively adds the Anthropic endpoint", () => {
    render(<ConnectionTab form={form()} />);

    const apiKey = screen.getByLabelText("API Key");
    expect(apiKey).toHaveAttribute("type", "password");
    fireEvent.click(screen.getByRole("button", { name: "显示" }));
    expect(apiKey).toHaveAttribute("type", "text");

    expect(screen.queryByLabelText("Anthropic 兼容端点")).not.toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: /添加 Anthropic 端点/ }));
    expect(screen.getByLabelText("Anthropic 兼容端点")).toBeInTheDocument();
  });
});
