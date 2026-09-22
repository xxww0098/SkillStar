import { fireEvent, render, screen, within } from "@testing-library/react";
import type { ReactNode } from "react";
import { describe, expect, it, vi } from "vitest";
import { Mcp } from "./Mcp";

vi.mock("../features/mcp/components/McpManager", () => ({
  McpManager: ({
    title,
    onOpenTools,
    onOpenStore,
  }: {
    title?: ReactNode;
    onOpenTools?: () => void;
    onOpenStore?: () => void;
  }) => (
    <div data-testid="mcp-config">
      {title}
      <button type="button" onClick={() => onOpenTools?.()} data-testid="open-tools">
        打开工具检查器
      </button>
      <button type="button" onClick={() => onOpenStore?.()} data-testid="open-store">
        打开商店视图
      </button>
    </div>
  ),
}));
vi.mock("../features/mcp/components/McpMarketPage", () => ({
  McpMarketPage: ({ title, onOpenSources }: { title?: ReactNode; onOpenSources?: () => void }) => (
    <div data-testid="mcp-store">
      {title}
      <button type="button" onClick={() => onOpenSources?.()} data-testid="open-sources">
        打开源检查器
      </button>
    </div>
  ),
}));
vi.mock("../features/mcp/components/McpToolStatusPanel", () => ({
  McpToolStatusPanel: () => <div data-testid="mcp-tools" />,
}));
vi.mock("../features/mcp/components/McpSourcesPanel", () => ({
  McpSourcesPanel: () => <div data-testid="mcp-sources" />,
}));

describe("MCP page", () => {
  it("is a two-view page: config and store, with no dashboard tabs", () => {
    render(<Mcp />);

    expect(screen.getByRole("tab", { name: "配置" })).toBeInTheDocument();
    expect(screen.getByRole("tab", { name: "商店" })).toBeInTheDocument();
    expect(screen.queryByRole("tab", { name: "机群" })).not.toBeInTheDocument();
    expect(screen.queryByRole("tab", { name: "官方" })).not.toBeInTheDocument();
    expect(screen.queryByRole("tab", { name: "工具" })).not.toBeInTheDocument();
    expect(screen.queryByRole("tab", { name: "目录源" })).not.toBeInTheDocument();
  });

  it("keeps the config view mounted while browsing the store", () => {
    render(<Mcp />);
    fireEvent.click(screen.getByRole("tab", { name: "商店" }));
    expect(screen.getByTestId("mcp-store")).toBeInTheDocument();
    expect(screen.getByTestId("mcp-config")).toBeInTheDocument();
  });

  it("opens the Tools inspector with the mocked tool-status panel", async () => {
    render(<Mcp />);

    fireEvent.click(screen.getByTestId("open-tools"));

    const dialog = await screen.findByRole("dialog", { name: "Agent 配置" });
    expect(within(dialog).getByTestId("mcp-tools")).toBeInTheDocument();
  });

  it("opens the Sources inspector after switching to the store view", async () => {
    render(<Mcp />);

    fireEvent.click(screen.getByRole("tab", { name: "商店" }));
    fireEvent.click(screen.getByTestId("open-sources"));

    const dialog = await screen.findByRole("dialog", { name: "目录源" });
    expect(within(dialog).getByTestId("mcp-sources")).toBeInTheDocument();
  });
});
