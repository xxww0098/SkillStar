import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import type { AgentProfile, McpProbeReport, McpServerEntry } from "../../../types";
import type { McpAgentTarget } from "../lib/agentTargets";
import { McpFleetCard } from "./McpFleetCard";

function testServer(patch: Partial<McpServerEntry> = {}): McpServerEntry {
  return {
    id: "srv-codegraph",
    name: "codegraph",
    description: "Codebase graph exploration",
    transport: "stdio",
    command: "npx",
    args: ["-y", "codegraph"],
    env: {},
    url: null,
    headers: {},
    homepage: null,
    tags: [],
    enabled: { "claude-code": true },
    autoApproveAll: false,
    disabledTools: [],
    sortIndex: 0,
    createdAt: 1700000000,
    updatedAt: 1700000000,
    registryName: null,
    installedVersion: "1.0.0",
    sourceId: null,
    runtimeKind: "stdio",
    ...patch,
  };
}

function mockProfile(patch: Partial<AgentProfile> = {}): AgentProfile {
  return {
    id: "claude-code",
    display_name: "Claude Code",
    icon: "claude",
    enabled: true,
    global_skills_dir: "/path/to/global",
    project_skills_rel: ".claude",
    installed: true,
    synced_count: 0,
    ...patch,
  };
}

function mockReport(overrides: Partial<McpProbeReport> = {}): McpProbeReport {
  return {
    serverId: "srv-codegraph",
    serverName: "codegraph",
    status: "healthy",
    epoch: "modern",
    protocolVersion: "2024-11-05",
    tools: ["search", "explore"],
    cachePrivate: false,
    checkedAt: 1700000000,
    schemaTokens: 428,
    ...overrides,
  };
}

const mockTargets: McpAgentTarget[] = [
  {
    toolId: "claude-code",
    profile: mockProfile({
      id: "claude-code",
      display_name: "Claude Code",
      icon: "claude",
    }),
  },
  {
    toolId: "cursor",
    profile: mockProfile({
      id: "cursor",
      display_name: "Cursor",
      icon: "cursor",
    }),
  },
];

describe("McpFleetCard", () => {
  it("renders identity, transport, description, and agent targets", () => {
    const onOpen = vi.fn();
    const onToggleTool = vi.fn();

    render(
      <McpFleetCard
        server={testServer()}
        agentTargets={mockTargets}
        probe={{
          report: mockReport(),
          error: null,
          pending: false,
        }}
        onOpen={onOpen}
        onToggleTool={onToggleTool}
      />,
    );

    // Identity & transport
    expect(screen.getByText("codegraph")).toBeInTheDocument();
    expect(screen.getByText(/stdio/i)).toBeInTheDocument();
    expect(screen.getByText("Codebase graph exploration")).toBeInTheDocument();
    expect(screen.getByText("健康")).toBeInTheDocument();

    // Tool counts, schema tokens, and a second status line stay off the card.
    expect(screen.queryByText(/2 个工具/i)).not.toBeInTheDocument();
    expect(screen.queryByText(/~428 tok/i)).not.toBeInTheDocument();

    // No probe button: probing lives in the editor's probe panel.
    expect(screen.queryByTitle("立即检查")).not.toBeInTheDocument();

    // Toggle agent
    const cursorBtn = screen.getByLabelText(/Cursor/i);
    fireEvent.click(cursorBtn);
    expect(onToggleTool).toHaveBeenCalledWith("cursor", true);
    expect(onOpen).not.toHaveBeenCalled();

    // Click card body
    fireEvent.click(screen.getByText("codegraph"));
    expect(onOpen).toHaveBeenCalledTimes(1);
  });

  it("renders update and YOLO badges when present", () => {
    render(
      <McpFleetCard
        server={testServer({ autoApproveAll: true })}
        agentTargets={[]}
        updateVersion="2.0.0"
        onOpen={vi.fn()}
        onToggleTool={vi.fn()}
      />,
    );

    expect(screen.getByText("有更新")).toBeInTheDocument();
    expect(screen.getByText("YOLO")).toBeInTheDocument();
  });

  it("renders only Settings-enabled Agents on the rail", () => {
    const targets: McpAgentTarget[] = [
      {
        toolId: "claude-code",
        profile: mockProfile({ id: "claude", display_name: "Claude Code", icon: "claude", enabled: true }),
      },
      {
        toolId: "cursor",
        profile: mockProfile({ id: "cursor", display_name: "Cursor", icon: "cursor", enabled: false }),
      },
      {
        toolId: "grok",
        profile: mockProfile({ id: "grok", display_name: "Grok", icon: "grok", enabled: true }),
      },
    ];

    render(<McpFleetCard server={testServer()} agentTargets={targets} onOpen={vi.fn()} onToggleTool={vi.fn()} />);

    expect(screen.getByLabelText(/Claude Code/i)).toBeInTheDocument();
    expect(screen.getByLabelText(/Grok/i)).toBeInTheDocument();
    expect(screen.queryByLabelText(/Cursor/i)).not.toBeInTheDocument();
  });

  it("lets the agent rail fill leftover footer width instead of packing against the right edge", () => {
    const { container } = render(
      <McpFleetCard server={testServer()} agentTargets={mockTargets} onOpen={vi.fn()} onToggleTool={vi.fn()} />,
    );

    const rail = container.querySelector(".hscroll-row-wrapper");
    expect(rail?.parentElement).toHaveClass("flex-1");
    expect(rail?.parentElement).toHaveClass("overflow-hidden");
    expect(rail?.parentElement).not.toHaveClass("justify-end");
  });

  it("shows unreachable status once, in the header, not again in the footer", () => {
    render(
      <McpFleetCard
        server={testServer()}
        agentTargets={mockTargets}
        probe={{
          report: mockReport({ status: "unreachable", tools: [], schemaTokens: undefined }),
          error: "connection refused",
          pending: false,
        }}
        onOpen={vi.fn()}
        onToggleTool={vi.fn()}
      />,
    );

    expect(screen.getAllByText("无法连接")).toHaveLength(1);
    expect(screen.queryByText(/个工具/)).not.toBeInTheDocument();
  });

  it("omits the footer when no Settings-enabled Agent is on the rail", () => {
    const { container } = render(
      <McpFleetCard server={testServer()} agentTargets={[]} onOpen={vi.fn()} onToggleTool={vi.fn()} />,
    );

    expect(container.querySelector(".hscroll-row-wrapper")).not.toBeInTheDocument();
    expect(screen.queryByText(/个工具/)).not.toBeInTheDocument();
    expect(screen.queryByText(/tok/i)).not.toBeInTheDocument();
  });

  it("falls back to command line when server description is empty", () => {
    render(
      <McpFleetCard
        server={testServer({ description: "", command: "npx", args: ["-y", "my-server"] })}
        agentTargets={[]}
        onOpen={vi.fn()}
        onToggleTool={vi.fn()}
      />,
    );

    expect(screen.getByText(/npx -y my-server/)).toBeInTheDocument();
  });
});
