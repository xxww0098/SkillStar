import { render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import type { McpProbeReport } from "../../../types";
import { McpProbePanel } from "./McpProbePanel";

function report(overrides: Partial<McpProbeReport> = {}): McpProbeReport {
  return {
    serverId: "srv",
    serverName: "demo",
    status: "healthy",
    epoch: "modern",
    protocolVersion: "2026-07-28",
    tools: ["search"],
    cachePrivate: false,
    checkedAt: 1,
    ...overrides,
  };
}

describe("McpProbePanel", () => {
  it("renders a modern epoch as the 2026-07-28 stateless protocol, not the internal word", () => {
    render(<McpProbePanel entry={{ report: report(), error: null, pending: false }} onProbe={vi.fn()} />);
    expect(screen.getByText("无状态 · 2026-07-28")).toBeInTheDocument();
    expect(screen.getByText(/无需握手/)).toBeInTheDocument();
    expect(screen.queryByText(/^modern/)).toBeNull();
  });

  it("surfaces tools/list cache ttl when the server sent one", () => {
    render(
      <McpProbePanel
        entry={{ report: report({ cacheTtlMs: 60_000 }), error: null, pending: false }}
        onProbe={vi.fn()}
      />,
    );
    expect(screen.getByText("工具列表缓存 60000 ms")).toBeInTheDocument();
  });
});
