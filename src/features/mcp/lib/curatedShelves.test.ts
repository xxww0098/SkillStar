import { describe, expect, it } from "vitest";
import type { McpMarketEntry } from "../../../types";
import { CURATED_MCP_PRESENTATION, groupMcpMarketShelves } from "./curatedShelves";

function entry(id: string, source: string | null): McpMarketEntry {
  return {
    id,
    name: id,
    namespace: id,
    description: "",
    repoUrl: "",
    stars: 0,
    license: null,
    version: null,
    kind: "stdio",
    runtimes: [],
    updatedAt: null,
    recommended: true,
    source,
    title: null,
    websiteUrl: null,
    iconUrl: null,
    status: "active",
    isLatest: true,
    registrySource: null,
  };
}

describe("groupMcpMarketShelves", () => {
  it("groups by source, keeping first-seen shelf order", () => {
    const sections = groupMcpMarketShelves([
      entry("filesystem", "core"),
      entry("context7", "context"),
      entry("git", "core"),
      entry("playwright", "browser"),
      entry("deepwiki", "context"),
    ]);

    expect(sections.map((s) => s.key)).toEqual(["core", "context", "browser"]);
    expect(sections[0].entries.map((e) => e.id)).toEqual(["filesystem", "git"]);
    expect(sections[1].entries.map((e) => e.id)).toEqual(["context7", "deepwiki"]);
    expect(sections[2].entries.map((e) => e.id)).toEqual(["playwright"]);
  });

  it("lands source-less rows under the fallback key", () => {
    const sections = groupMcpMarketShelves([entry("registry-row", null)]);
    expect(sections).toEqual([{ key: "", entries: [expect.objectContaining({ id: "registry-row" })] }]);
  });
});

describe("CURATED_MCP_PRESENTATION", () => {
  it("covers every row of the curated shortlist", () => {
    // The catalog (`seeds::catalog`) is a fixed code registry; if a curated id
    // is missing here the card silently falls back to the kind glyph.
    for (const id of [
      "filesystem",
      "git",
      "github",
      "context7",
      "deepwiki",
      "codegraph",
      "serena",
      "playwright",
      "chrome-devtools",
      "figma",
      "blender",
      "photoshop",
      "after-effects",
    ]) {
      expect(CURATED_MCP_PRESENTATION[id], `missing presentation for '${id}'`).toBeDefined();
    }
  });
});
