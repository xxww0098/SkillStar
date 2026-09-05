import { describe, expect, it } from "vitest";
import type { McpServerEntry } from "../../../types";
import type { McpProbeEntry } from "../hooks/useMcpProbe";
import { mcpFleetStatus, mcpFleetStatusMatches, summarizeMcpFleetHealth } from "./fleetStatus";

function entry(partial: Partial<McpProbeEntry> = {}): McpProbeEntry {
  return { report: null, error: null, pending: false, ...partial };
}

function server(id: string): McpServerEntry {
  return {
    id,
    name: id,
    transport: "stdio",
    args: [],
    env: {},
    headers: {},
    tags: [],
    enabled: {},
    autoApproveAll: false,
    sortIndex: 0,
  } as McpServerEntry;
}

describe("mcpFleetStatus", () => {
  it("treats authorization-required as sign-in, not error", () => {
    const status = mcpFleetStatus(
      entry({
        report: {
          serverId: "a",
          serverName: "a",
          status: "authorization-required",
          cachePrivate: false,
          checkedAt: 1,
        },
      }),
    );
    expect(status).toBe("needs-auth");
    expect(mcpFleetStatusMatches(status, "auth")).toBe(true);
    expect(mcpFleetStatusMatches(status, "attention")).toBe(false);
  });

  it("buckets unreachable and missing runtime as attention", () => {
    expect(
      mcpFleetStatusMatches(
        mcpFleetStatus(
          entry({
            report: {
              serverId: "a",
              serverName: "a",
              status: "unreachable",
              cachePrivate: false,
              checkedAt: 1,
            },
          }),
        ),
        "attention",
      ),
    ).toBe(true);
    expect(
      mcpFleetStatusMatches(
        mcpFleetStatus(
          entry({
            report: {
              serverId: "a",
              serverName: "a",
              status: "runtime-missing",
              cachePrivate: false,
              checkedAt: 1,
            },
          }),
        ),
        "attention",
      ),
    ).toBe(true);
  });
});

describe("summarizeMcpFleetHealth", () => {
  it("sums schema tokens only from healthy probes and never labels auth as attention", () => {
    const stats = summarizeMcpFleetHealth([server("a"), server("b")], (id) =>
      id === "a"
        ? entry({
            report: {
              serverId: "a",
              serverName: "a",
              status: "authorization-required",
              cachePrivate: false,
              checkedAt: 1,
            },
          })
        : entry({
            report: {
              serverId: "b",
              serverName: "b",
              status: "healthy",
              cachePrivate: false,
              schemaTokens: 1200,
              schemaBytes: 4800,
              checkedAt: 1,
            },
          }),
    );
    expect(stats.auth).toBe(1);
    expect(stats.healthy).toBe(1);
    expect(stats.attention).toBe(0);
    expect(stats.schemaTokens).toBe(1200);
  });
});
