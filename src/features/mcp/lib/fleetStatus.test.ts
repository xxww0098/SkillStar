import { describe, expect, it } from "vitest";
import type { McpProbeReport } from "../../../types";
import type { McpProbeEntry } from "../hooks/useMcpProbe";
import { mcpFleetStatus } from "./fleetStatus";

function entry(partial: Partial<McpProbeEntry> = {}): McpProbeEntry {
  return { report: null, error: null, pending: false, ...partial };
}

function report(status: McpProbeReport["status"]): McpProbeReport {
  return {
    serverId: "a",
    serverName: "a",
    status,
    cachePrivate: false,
    checkedAt: 1,
  };
}

describe("mcpFleetStatus", () => {
  it("treats authorization-required as sign-in, not error", () => {
    expect(mcpFleetStatus(entry({ report: report("authorization-required") }))).toBe("needs-auth");
  });

  it("maps unreachable and missing runtime", () => {
    expect(mcpFleetStatus(entry({ report: report("unreachable") }))).toBe("error");
    expect(mcpFleetStatus(entry({ report: report("runtime-missing") }))).toBe("runtime-missing");
    expect(mcpFleetStatus(entry({ report: report("healthy") }))).toBe("ok");
  });

  it("reports probing while a probe is in flight", () => {
    expect(mcpFleetStatus(entry({ pending: true }))).toBe("probing");
  });

  it("only reports error when the probe failed without a report to read", () => {
    expect(mcpFleetStatus(entry({ error: "connection refused" }))).toBe("error");
    // A report that arrived alongside an error is still the better answer.
    expect(mcpFleetStatus(entry({ error: "connection refused", report: report("healthy") }))).toBe("ok");
  });

  it("falls back to unknown before anything has been probed", () => {
    expect(mcpFleetStatus(entry())).toBe("unknown");
  });
});
