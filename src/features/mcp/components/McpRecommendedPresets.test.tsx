import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import type { McpPreset } from "../../../types";
import { McpRecommendedPresets } from "./McpRecommendedPresets";

const MOCK_PRESETS: McpPreset[] = [
  {
    id: "cua-driver",
    name: "cua-driver",
    description: "Cua Driver - Computer Use",
    homepage: "https://cua.ai",
    transport: "stdio",
    command: "cua-driver",
    args: ["mcp"],
    env: {},
    headers: {},
    tags: ["computer-use"],
    requiredEnv: [],
  },
  {
    id: "playwright",
    name: "playwright",
    description: "Playwright browser automation",
    homepage: "https://github.com/microsoft/playwright",
    transport: "stdio",
    command: "npx",
    args: ["-y", "@playwright/mcp@latest"],
    env: {},
    headers: {},
    tags: ["browser"],
    requiredEnv: [],
  },
  {
    id: "github",
    catalogId: "mkt-github",
    name: "github",
    description: "GitHub MCP",
    homepage: "https://github.com/github/github-mcp-server",
    transport: "http",
    url: "https://api.githubcopilot.com/mcp/",
    args: [],
    env: {},
    headers: {},
    tags: ["git"],
    requiredEnv: ["GITHUB_TOKEN"],
  },
];

describe("McpRecommendedPresets", () => {
  it("renders nothing when all presets are already installed", () => {
    const installed = new Set(["cua-driver", "playwright", "github"]);
    const { container } = render(
      <McpRecommendedPresets presets={MOCK_PRESETS} installedNames={installed} onPick={vi.fn()} />,
    );
    expect(container.firstChild).toBeNull();
  });

  it("filters out installed presets and displays available ones", () => {
    const installed = new Set(["cua-driver"]);
    render(<McpRecommendedPresets presets={MOCK_PRESETS} installedNames={installed} onPick={vi.fn()} />);

    expect(screen.queryByText("cua-driver")).toBeNull();
    expect(screen.getByText("playwright")).toBeInTheDocument();
    expect(screen.getByText("github")).toBeInTheDocument();
  });

  it("calls onPick with the clicked preset", () => {
    const onPick = vi.fn();
    render(<McpRecommendedPresets presets={MOCK_PRESETS} installedNames={new Set()} onPick={onPick} />);

    fireEvent.click(screen.getByRole("button", { name: /playwright/i }));
    expect(onPick).toHaveBeenCalledTimes(1);
    expect(onPick).toHaveBeenCalledWith(MOCK_PRESETS[1]);
  });

  it("highlights the selected preset and shows reset button", () => {
    const onReset = vi.fn();
    render(
      <McpRecommendedPresets
        presets={MOCK_PRESETS}
        installedNames={new Set()}
        selectedPresetId="playwright"
        onPick={vi.fn()}
        onReset={onReset}
      />,
    );

    const button = screen.getByRole("button", { name: /playwright/i });
    expect(button).toHaveAttribute("aria-pressed", "true");

    const resetButton = screen.getByRole("button", { name: /重置|Reset/i });
    expect(resetButton).toBeInTheDocument();
    fireEvent.click(resetButton);
    expect(onReset).toHaveBeenCalledTimes(1);
  });
});
