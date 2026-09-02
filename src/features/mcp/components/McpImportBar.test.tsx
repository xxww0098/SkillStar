import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { invoke } from "@tauri-apps/api/core";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { McpImportBar } from "./McpImportBar";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(),
  isTauri: vi.fn(() => true),
}));

describe("McpImportBar", () => {
  beforeEach(() => {
    vi.mocked(invoke).mockReset();
  });

  it("reviews a paste and never claims to install", async () => {
    vi.mocked(invoke).mockResolvedValue({
      kind: "url",
      drafts: [{ name: "example-com", transport: "http", url: "https://example.com/mcp" }],
      warnings: [],
    });
    const onParsed = vi.fn();
    render(<McpImportBar onParsed={onParsed} />);
    fireEvent.change(screen.getByPlaceholderText(/skillstar:\/\/mcp/), {
      target: { value: "https://example.com/mcp" },
    });
    fireEvent.click(screen.getByRole("button", { name: "查看并确认" }));
    await waitFor(() => expect(onParsed).toHaveBeenCalled());
    expect(invoke).toHaveBeenCalledWith("parse_mcp_paste", { text: "https://example.com/mcp" });
    expect(screen.queryByRole("button", { name: /安装/ })).toBeNull();
  });

  it("surfaces an unknown paste without calling onParsed", async () => {
    vi.mocked(invoke).mockResolvedValue({ kind: "unknown", drafts: [], warnings: [], error: "nope" });
    const onParsed = vi.fn();
    render(<McpImportBar onParsed={onParsed} />);
    fireEvent.change(screen.getByPlaceholderText(/skillstar:\/\/mcp/), { target: { value: "hello world" } });
    fireEvent.click(screen.getByRole("button", { name: "查看并确认" }));
    await waitFor(() => expect(screen.getByText("nope")).toBeInTheDocument());
    expect(onParsed).not.toHaveBeenCalled();
  });
});
