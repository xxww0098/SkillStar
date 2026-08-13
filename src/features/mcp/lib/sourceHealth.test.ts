import { describe, expect, it } from "vitest";
import type { McpSourceDescriptor, SyncStateEntry } from "../../../types";
import {
  buildMcpSourceStatuses,
  classifyMcpSourceHealth,
  parseMcpSourceScope,
  summarizeMcpCatalogHealth,
} from "./sourceHealth";

const NOW = Date.parse("2026-08-13T12:00:00.000Z");
const iso = (offsetMinutes: number) => new Date(NOW + offsetMinutes * 60_000).toISOString();

function state(patch: Partial<SyncStateEntry> = {}): SyncStateEntry {
  return {
    scope: "mcp_registry:official",
    last_success_at: iso(-10),
    last_attempt_at: iso(-10),
    last_error: null,
    next_refresh_at: iso(30),
    schema_version: 13,
    degraded_reason: null,
    ...patch,
  };
}

function source(patch: Partial<McpSourceDescriptor> = {}): McpSourceDescriptor {
  return {
    id: "official",
    displayName: "Official MCP Registry",
    baseUrl: "https://registry.modelcontextprotocol.io/v0.1/servers",
    kind: "registry",
    cursorStyle: "camel",
    listQuery: null,
    requiresKey: false,
    license: "cc0",
    mirrorable: true,
    enabled: true,
    builtin: true,
    priority: 0,
    maxPages: 400,
    ...patch,
  };
}

describe("parseMcpSourceScope", () => {
  it("extracts the source id from a per-source scope", () => {
    expect(parseMcpSourceScope("mcp_registry:github")).toBe("github");
    expect(parseMcpSourceScope("mcp_registry:custom:acme")).toBe("custom:acme");
  });

  it("returns null for the aggregate scope", () => {
    expect(parseMcpSourceScope("mcp_registry")).toBeNull();
  });
});

describe("classifyMcpSourceHealth", () => {
  it("is fresh only with data, no error and a refresh still ahead", () => {
    expect(classifyMcpSourceHealth(state(), NOW)).toBe("fresh");
  });

  it("reports a truncated sync as incomplete even though it succeeded", () => {
    // The failure mode a merged catalog has and a single-source one does not:
    // the sync "succeeded" and the result is still missing servers.
    expect(classifyMcpSourceHealth(state({ degraded_reason: "stopped after 50 pages" }), NOW)).toBe("degraded");
  });

  it("prefers the error over the degraded note when the last attempt failed", () => {
    expect(classifyMcpSourceHealth(state({ last_error: "ECONNREFUSED", degraded_reason: "partial" }), NOW)).toBe(
      "error",
    );
  });

  it("reports a source with no successful sync as never, not as an error", () => {
    expect(classifyMcpSourceHealth(state({ last_success_at: null }), NOW)).toBe("never");
    expect(classifyMcpSourceHealth(state({ last_success_at: null, last_error: "boom" }), NOW)).toBe("error");
  });

  it("goes stale once the scheduled refresh time has passed", () => {
    expect(classifyMcpSourceHealth(state({ next_refresh_at: iso(-1) }), NOW)).toBe("stale");
  });

  it("stays fresh when the refresh time is unparseable rather than guessing stale", () => {
    expect(classifyMcpSourceHealth(state({ next_refresh_at: "not-a-date" }), NOW)).toBe("fresh");
    expect(classifyMcpSourceHealth(state({ next_refresh_at: null }), NOW)).toBe("fresh");
  });
});

describe("buildMcpSourceStatuses", () => {
  it("joins states with descriptors and keeps a never-synced source visible", () => {
    const statuses = buildMcpSourceStatuses(
      [state({ scope: "mcp_registry:official" })],
      [source(), source({ id: "custom:acme", displayName: "Acme", builtin: false })],
      NOW,
    );

    expect(statuses.map((s) => [s.sourceId, s.health])).toEqual([
      ["official", "fresh"],
      ["custom:acme", "never"],
    ]);
    expect(statuses[1].descriptor?.displayName).toBe("Acme");
  });

  it("keeps a state whose source has since been removed", () => {
    const statuses = buildMcpSourceStatuses([state({ scope: "mcp_registry:custom:gone" })], [], NOW);
    expect(statuses[0]).toMatchObject({ sourceId: "custom:gone", descriptor: null });
  });

  it("ignores the aggregate scope, which is not a source", () => {
    expect(buildMcpSourceStatuses([state({ scope: "mcp_registry" })], [], NOW)).toEqual([]);
  });
});

describe("summarizeMcpCatalogHealth", () => {
  it("calls the catalog incomplete when a source is degraded", () => {
    const statuses = buildMcpSourceStatuses(
      [
        state({ scope: "mcp_registry:official" }),
        state({ scope: "mcp_registry:github", degraded_reason: "rate limited after 50 pages" }),
      ],
      [source(), source({ id: "github", displayName: "GitHub MCP Registry", mirrorable: false })],
      NOW,
    );
    const health = summarizeMcpCatalogHealth(statuses);

    expect(health.incomplete).toBe(true);
    expect(health.reasons).toEqual([
      {
        sourceId: "github",
        label: "GitHub MCP Registry",
        health: "degraded",
        detail: "rate limited after 50 pages",
      },
    ]);
    expect([health.freshCount, health.enabledCount]).toEqual([1, 2]);
  });

  it("does not blame a source the user switched off", () => {
    // A disabled source is not a gap in the catalog; counting it would make the
    // warning permanent and therefore ignorable.
    const statuses = buildMcpSourceStatuses(
      [state({ scope: "mcp_registry:github", last_error: "boom" })],
      [source({ id: "github", enabled: false })],
      NOW,
    );
    const health = summarizeMcpCatalogHealth(statuses);

    expect(health.incomplete).toBe(false);
    expect(health.reasons).toEqual([]);
  });

  it("treats a stale source as worth mentioning but not as incompleteness", () => {
    const statuses = buildMcpSourceStatuses(
      [state({ scope: "mcp_registry:official", next_refresh_at: iso(-1) })],
      [source()],
      NOW,
    );
    const health = summarizeMcpCatalogHealth(statuses);

    expect(health.incomplete).toBe(false);
    expect(health.reasons.map((r) => r.health)).toEqual(["stale"]);
  });

  it("reports the most recent successful sync across sources", () => {
    const statuses = buildMcpSourceStatuses(
      [
        state({ scope: "mcp_registry:official", last_success_at: iso(-100) }),
        state({ scope: "mcp_registry:github", last_success_at: iso(-5) }),
      ],
      [source(), source({ id: "github" })],
      NOW,
    );

    expect(summarizeMcpCatalogHealth(statuses).lastSuccessAt).toBe(iso(-5));
  });
});
