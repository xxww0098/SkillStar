import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { act, renderHook } from "@testing-library/react";
import { invoke } from "@tauri-apps/api/core";
import type { ReactNode } from "react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { MCP_FLEET_PROBE_CAP, useMcpProbe } from "./useMcpProbe";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(),
  isTauri: vi.fn(() => true),
}));

function wrapper({ children }: { children: ReactNode }) {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false }, mutations: { retry: false } } });
  return <QueryClientProvider client={client}>{children}</QueryClientProvider>;
}

describe("useMcpProbe", () => {
  beforeEach(() => {
    vi.mocked(invoke).mockReset();
  });

  it("probes the fleet sequentially and stops at the cap", async () => {
    const seen: string[] = [];
    vi.mocked(invoke).mockImplementation(async (_cmd, args) => {
      const id = String((args as { id: string }).id);
      seen.push(id);
      return {
        serverId: id,
        serverName: id,
        status: "healthy",
        cachePrivate: false,
        checkedAt: 1,
        tools: [],
      };
    });
    const { result } = renderHook(() => useMcpProbe(), { wrapper });
    const ids = Array.from({ length: MCP_FLEET_PROBE_CAP + 3 }, (_, i) => `s${i}`);
    await act(async () => {
      await result.current.probeFleet(ids);
    });
    expect(seen).toHaveLength(MCP_FLEET_PROBE_CAP);
    expect(seen).toEqual(ids.slice(0, MCP_FLEET_PROBE_CAP));
  });
});
