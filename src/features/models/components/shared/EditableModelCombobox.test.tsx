import { fireEvent, render, screen } from "@testing-library/react";
import { useState } from "react";
import { describe, expect, it } from "vitest";
import { EditableModelCombobox } from "./EditableModelCombobox";

describe("EditableModelCombobox", () => {
  it("accepts a suggested model and still allows custom ids", async () => {
    function Harness() {
      const [value, setValue] = useState("");
      return (
        <EditableModelCombobox
          value={value}
          options={["claude-sonnet-4-6", "claude-opus-4-7"]}
          onChange={setValue}
          ariaLabel="Main model"
        />
      );
    }

    render(<Harness />);
    const input = screen.getByRole("combobox", { name: "Main model" });

    fireEvent.focus(input);
    fireEvent.click(await screen.findByRole("option", { name: "claude-sonnet-4-6" }));
    expect(input).toHaveValue("claude-sonnet-4-6");

    fireEvent.change(input, { target: { value: "custom-model" } });
    expect(input).toHaveValue("custom-model");

    fireEvent.click(screen.getByRole("button", { name: "Main model" }));
    expect(await screen.findByRole("option", { name: "claude-opus-4-7" })).toBeInTheDocument();
  });

  it("supports keyboard navigation while input focus stays in the combobox", async () => {
    function Harness() {
      const [value, setValue] = useState("");
      return (
        <EditableModelCombobox
          value={value}
          options={["claude-sonnet-4-6", "claude-opus-4-7"]}
          onChange={setValue}
          ariaLabel="Main model"
        />
      );
    }

    render(<Harness />);
    const input = screen.getByRole("combobox", { name: "Main model" });

    fireEvent.focus(input);
    fireEvent.keyDown(input, { key: "ArrowDown" });
    expect(input).toHaveAttribute("aria-activedescendant");
    expect(screen.getByRole("option", { name: "claude-sonnet-4-6" })).toHaveClass("bg-muted/55");

    fireEvent.keyDown(input, { key: "ArrowDown" });
    fireEvent.keyDown(input, { key: "Enter" });
    expect(input).toHaveValue("claude-opus-4-7");
    expect(input).toHaveAttribute("aria-expanded", "false");

    fireEvent.keyDown(input, { key: "ArrowDown" });
    fireEvent.keyDown(input, { key: "Escape" });
    expect(input).toHaveAttribute("aria-expanded", "false");
  });
});
