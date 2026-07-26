import { fireEvent, render, screen } from "@testing-library/react";
import { useState } from "react";
import { describe, expect, it } from "vitest";
import {
  ModelFormField,
  ModelSegmentedControl,
  modelCompactInputClass,
  modelInputClass,
} from "./ProviderConfigPrimitives";

describe("ProviderConfigPrimitives", () => {
  it("keeps standard and compact controls on the same token surface", () => {
    expect(modelInputClass).toContain("h-10");
    expect(modelInputClass).toContain("rounded-[10px]");
    expect(modelInputClass).toContain("border-input-border");
    expect(modelInputClass).toContain("bg-input");

    expect(modelCompactInputClass).toContain("h-9");
    expect(modelCompactInputClass).toContain("rounded-[9px]");
    expect(modelCompactInputClass).toContain("border-input-border");
    expect(modelCompactInputClass).toContain("bg-input");
  });

  it("associates a field label and renders hint, error and required state", () => {
    render(
      <ModelFormField id="provider-name" label="Name" hint="Visible hint" error="Required" required>
        <input id="provider-name" aria-describedby="provider-name-hint provider-name-error" aria-invalid />
      </ModelFormField>,
    );

    expect(screen.getByLabelText(/Name/)).toHaveAttribute("aria-invalid");
    expect(screen.getByText("Visible hint")).toHaveAttribute("id", "provider-name-hint");
    expect(screen.getByRole("alert")).toHaveAttribute("id", "provider-name-error");
  });

  it("uses a keyboard-ready radio group for segmented choices", () => {
    function Harness() {
      const [value, setValue] = useState("responses");
      return (
        <ModelSegmentedControl
          value={value}
          onChange={setValue}
          ariaLabel="API format"
          options={[
            { value: "responses", label: "Responses" },
            { value: "chat", label: "Chat Completions" },
          ]}
        />
      );
    }

    render(<Harness />);
    const group = screen.getByRole("radiogroup", { name: "API format" });
    const responses = screen.getByRole("radio", { name: "Responses" });
    const chat = screen.getByRole("radio", { name: "Chat Completions" });

    expect(group).toBeInTheDocument();
    expect(responses).toHaveAttribute("data-state", "checked");
    fireEvent.click(chat);
    expect(chat).toHaveAttribute("data-state", "checked");
  });
});
