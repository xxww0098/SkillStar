import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { invoke } from "@tauri-apps/api/core";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { McpPreset } from "../../../types";
import { McpAddDialog, type McpAddDialogProps } from "./McpAddDialog";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(),
  isTauri: vi.fn(() => true),
}));

function renderDialog(overrides: Partial<McpAddDialogProps> = {}) {
  const props: McpAddDialogProps = {
    mode: "paste",
    onModeChange: vi.fn(),
    presets: [],
    installedNames: new Set<string>(),
    formKey: 0,
    pasteSeed: { key: 0, text: "" },
    submitting: false,
    importing: false,
    targets: [],
    onPickPreset: vi.fn(),
    onSubmit: vi.fn(),
    onImport: vi.fn(),
    onParsed: vi.fn(),
    ...overrides,
  };
  render(<McpAddDialog {...props} />);
  return props;
}

const preset = (patch: Partial<McpPreset> = {}): McpPreset =>
  ({
    id: "p1",
    name: "codegraph",
    args: [],
    env: {},
    headers: {},
    ...patch,
  }) as McpPreset;

describe("McpAddDialog", () => {
  beforeEach(() => {
    vi.mocked(invoke).mockReset();
  });

  it("reviews a paste and never claims to install", async () => {
    vi.mocked(invoke).mockResolvedValue({
      kind: "url",
      drafts: [{ name: "example-com", transport: "http", url: "https://example.com/mcp" }],
      warnings: [],
    });
    const props = renderDialog();

    fireEvent.change(screen.getByPlaceholderText(/skillstar:\/\/mcp/), {
      target: { value: "https://example.com/mcp" },
    });
    fireEvent.click(screen.getByRole("button", { name: "查看并确认" }));

    await waitFor(() => expect(props.onParsed).toHaveBeenCalled());
    expect(invoke).toHaveBeenCalledWith("parse_mcp_paste", { text: "https://example.com/mcp" });
    expect(screen.queryByRole("button", { name: /安装/ })).toBeNull();
  });

  it("surfaces an unknown paste without forwarding it", async () => {
    vi.mocked(invoke).mockResolvedValue({ kind: "unknown", drafts: [], warnings: [], error: "nope" });
    const props = renderDialog();

    fireEvent.change(screen.getByPlaceholderText(/skillstar:\/\/mcp/), { target: { value: "hello world" } });
    fireEvent.click(screen.getByRole("button", { name: "查看并确认" }));

    await waitFor(() => expect(screen.getByText("nope")).toBeInTheDocument());
    expect(props.onParsed).not.toHaveBeenCalled();
  });

  it("runs the tool import from the import mode", () => {
    const props = renderDialog({ mode: "import" });

    fireEvent.click(screen.getByRole("button", { name: "从工具导入" }));

    expect(props.onImport).toHaveBeenCalledTimes(1);
  });

  it("says so when there is nothing left to recommend", () => {
    renderDialog({ mode: "recommended", presets: [] });

    expect(screen.getByText(/没有可用的推荐服务器/)).toBeInTheDocument();
  });

  it("hides an already-installed recommendation and offers the rest", () => {
    renderDialog({
      mode: "recommended",
      presets: [preset({ id: "a", name: "codegraph" }), preset({ id: "b", name: "playwright" })],
      installedNames: new Set(["codegraph"]),
    });

    expect(screen.getByRole("button", { name: /playwright/i })).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: /codegraph/i })).not.toBeInTheDocument();
  });

  it("switches modes through the mode buttons", () => {
    const props = renderDialog({ mode: "recommended" });

    fireEvent.click(screen.getByRole("button", { name: "手动填写" }));

    expect(props.onModeChange).toHaveBeenCalledWith("manual");
  });
});
