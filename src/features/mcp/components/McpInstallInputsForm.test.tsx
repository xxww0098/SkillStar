import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import type { McpInput, McpInstallInput } from "../../../types";
import { buildInstallFields } from "../lib/installForm";
import { McpInstallInputsForm } from "./McpInstallInputsForm";

function input(patch: Partial<McpInput> = {}): McpInput {
  return { isRequired: false, isSecret: false, format: "string", ...patch };
}

function declared(patch: Partial<McpInstallInput> = {}): McpInstallInput {
  return { key: "API_KEY", scope: "environment", index: 0, input: input(), prefilled: "", mustAsk: false, ...patch };
}

function renderForm(inputs: McpInstallInput[], errors: Parameters<typeof McpInstallInputsForm>[0]["errors"] = []) {
  const onFieldChange = vi.fn();
  const onVariableChange = vi.fn();
  render(
    <McpInstallInputsForm
      fields={buildInstallFields(inputs)}
      errors={errors}
      onFieldChange={onFieldChange}
      onVariableChange={onVariableChange}
    />,
  );
  return { onFieldChange, onVariableChange };
}

describe("McpInstallInputsForm", () => {
  it("renders a secret input masked, and only reveals it on request", () => {
    renderForm([declared({ key: "API_KEY", input: input({ isSecret: true, isRequired: true }), mustAsk: true })]);

    const field = screen.getByLabelText(/API_KEY/);
    expect(field).toHaveAttribute("type", "password");
    expect(field).toHaveAttribute("autocomplete", "off");

    fireEvent.click(screen.getByTitle("显示值"));
    expect(screen.getByLabelText(/API_KEY/)).toHaveAttribute("type", "text");
  });

  it("renders choices as a closed select rather than a free-text box", () => {
    renderForm([declared({ key: "REGION", input: input({ choices: ["us", "eu"] }) })]);

    const select = screen.getByLabelText(/REGION/);
    expect(select.tagName).toBe("SELECT");
    expect([...select.querySelectorAll("option")].map((o) => o.getAttribute("value"))).toEqual(["", "us", "eu"]);
  });

  it("treats placeholder as a hint, never as a value", () => {
    renderForm([declared({ key: "ROOT", input: input({ placeholder: "/path/to/dir" }) })]);

    const field = screen.getByLabelText(/ROOT/);
    expect(field).toHaveAttribute("placeholder", "/path/to/dir");
    expect(field).toHaveValue("");
  });

  it("prefills a default and reports edits", () => {
    const { onFieldChange } = renderForm([declared({ key: "PORT", prefilled: "50325" })]);

    const field = screen.getByLabelText(/PORT/);
    expect(field).toHaveValue("50325");

    fireEvent.change(field, { target: { value: "50326" } });
    expect(onFieldChange).toHaveBeenCalledWith("environment", 0, "50326");
  });

  it("locks a publisher-pinned value and edits its variable instead", () => {
    const { onVariableChange } = renderForm([
      declared({
        key: "Authorization",
        scope: "header",
        prefilled: "Bearer {TOKEN}",
        input: input({ value: "Bearer {TOKEN}" }),
        variables: [{ name: "TOKEN", variable: { isRequired: true, isSecret: true, format: "string" }, prefilled: "" }],
      }),
    ]);

    // The template itself is rendered as read-only text, not as a control.
    expect(screen.queryByLabelText("Authorization")).toBeNull();
    expect(screen.getByText("Bearer {TOKEN}")).toBeInTheDocument();

    const variable = screen.getByLabelText(/\{TOKEN\}/);
    expect(variable).toHaveAttribute("type", "password");
    fireEvent.change(variable, { target: { value: "ghp_x" } });
    expect(onVariableChange).toHaveBeenCalledWith("header", 0, "TOKEN", "ghp_x");
  });

  it("shows the publisher's own description for a field", () => {
    renderForm([declared({ key: "ROOT", input: input({ description: "Directory the agent may read" }) })]);
    expect(screen.getByText("Directory the agent may read")).toBeInTheDocument();
  });

  it("renders a per-field validation error", () => {
    renderForm(
      [declared({ key: "API_KEY", mustAsk: true, input: input({ isRequired: true }) })],
      [{ scope: "environment", index: 0, code: "required" }],
    );
    expect(screen.getByText("必填")).toBeInTheDocument();
  });

  it("renders nothing when the plan declares no inputs", () => {
    const { container } = render(
      <McpInstallInputsForm fields={[]} errors={[]} onFieldChange={vi.fn()} onVariableChange={vi.fn()} />,
    );
    expect(container).toBeEmptyDOMElement();
  });
});
