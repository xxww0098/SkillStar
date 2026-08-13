import { describe, expect, it } from "vitest";
import {
  activeMcpFilterCount,
  buildMcpServerQuery,
  clampMcpOffset,
  DEFAULT_MCP_MARKET_FILTERS,
  hasActiveMcpNarrowing,
  type McpMarketFilterState,
  mcpPageWindow,
  statusesFor,
  toggleFilterValue,
} from "./marketQuery";

const filters = (patch: Partial<McpMarketFilterState> = {}): McpMarketFilterState => ({
  ...DEFAULT_MCP_MARKET_FILTERS,
  ...patch,
});

describe("buildMcpServerQuery", () => {
  it("sends only pagination and the status filter for a default state", () => {
    expect(buildMcpServerQuery({ filters: filters(), limit: 60, offset: 0 })).toEqual({
      limit: 60,
      offset: 0,
      statuses: ["active"],
    });
  });

  it("always sends statuses explicitly, because the backend default includes deprecated", () => {
    // An omitted `statuses` means "no status filter", which returns deprecated
    // rows. Leaving it out would silently invert the UI's default.
    expect(buildMcpServerQuery({ filters: filters(), limit: 10, offset: 0 }).statuses).toEqual(["active"]);
    expect(
      buildMcpServerQuery({ filters: filters({ includeDeprecated: true }), limit: 10, offset: 0 }).statuses,
    ).toEqual(["active", "deprecated"]);
    expect(statusesFor(false)).toEqual(["active"]);
  });

  it("trims the search term and drops it when blank", () => {
    expect(buildMcpServerQuery({ filters: filters({ search: "  fs  " }), limit: 10, offset: 0 }).search).toBe("fs");
    expect(buildMcpServerQuery({ filters: filters({ search: "   " }), limit: 10, offset: 0 })).not.toHaveProperty(
      "search",
    );
  });

  it("compiles every filter control into the query", () => {
    const query = buildMcpServerQuery({
      filters: filters({
        kinds: ["remote"],
        runtimes: ["npx", "uvx"],
        licenses: ["MIT"],
        recommendedOnly: true,
        latestOnly: true,
        minStars: 100,
        maxStars: 5000,
        sort: "stars",
        descending: false,
      }),
      limit: 20,
      offset: 40,
      publisherId: "github",
    });

    expect(query).toEqual({
      limit: 20,
      offset: 40,
      publisherId: "github",
      kinds: ["remote"],
      runtimes: ["npx", "uvx"],
      licenses: ["MIT"],
      statuses: ["active"],
      recommendedOnly: true,
      latestOnly: true,
      minStars: 100,
      maxStars: 5000,
      sort: "stars",
      descending: false,
    });
  });

  it("keeps a zero star bound instead of treating it as unset", () => {
    expect(buildMcpServerQuery({ filters: filters({ minStars: 0 }), limit: 10, offset: 0 }).minStars).toBe(0);
  });

  it("omits the default sort key so the historical order stays implicit", () => {
    expect(buildMcpServerQuery({ filters: filters(), limit: 10, offset: 0 })).not.toHaveProperty("sort");
  });
});

describe("activeMcpFilterCount", () => {
  it("counts nothing for a default state", () => {
    expect(activeMcpFilterCount(filters())).toBe(0);
    expect(hasActiveMcpNarrowing(filters())).toBe(false);
  });

  it("counts a stars range once, not twice", () => {
    expect(activeMcpFilterCount(filters({ minStars: 10, maxStars: 20 }))).toBe(1);
  });

  it("treats a search term as narrowing but not as a filter chip", () => {
    expect(activeMcpFilterCount(filters({ search: "fs" }))).toBe(0);
    expect(hasActiveMcpNarrowing(filters({ search: "fs" }))).toBe(true);
  });
});

describe("toggleFilterValue", () => {
  it("adds at the end and removes in place", () => {
    expect(toggleFilterValue(["npx"], "uvx")).toEqual(["npx", "uvx"]);
    expect(toggleFilterValue(["npx", "uvx"], "npx")).toEqual(["uvx"]);
  });
});

describe("mcpPageWindow", () => {
  it("describes the first page of a large catalog", () => {
    expect(mcpPageWindow(21363, 0, 60, 60)).toEqual({
      from: 1,
      to: 60,
      total: 21363,
      pageIndex: 0,
      pageCount: 357,
      hasPrev: false,
      hasNext: true,
    });
  });

  it("describes a short last page", () => {
    const window = mcpPageWindow(125, 120, 60, 5);
    expect([window.from, window.to, window.hasNext, window.hasPrev]).toEqual([121, 125, false, true]);
  });

  it("reports an empty range rather than 1–0 when the page has no rows", () => {
    expect(mcpPageWindow(0, 0, 60, 0)).toMatchObject({ from: 0, to: 0, pageCount: 0, hasNext: false });
  });
});

describe("clampMcpOffset", () => {
  it("snaps to the last page when the result set shrank", () => {
    expect(clampMcpOffset(600, 125, 60)).toBe(120);
  });

  it("returns 0 for an empty result set", () => {
    expect(clampMcpOffset(600, 0, 60)).toBe(0);
  });

  it("leaves a valid offset untouched", () => {
    expect(clampMcpOffset(60, 21363, 60)).toBe(60);
  });

  it("never returns a negative offset", () => {
    expect(clampMcpOffset(-60, 21363, 60)).toBe(0);
  });
});
