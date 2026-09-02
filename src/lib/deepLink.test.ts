import { describe, expect, it } from "vitest";
import { deepLinkNavTarget, mcpImportPasteText, mcpImportQuery } from "./deepLink";

describe("deepLinkNavTarget", () => {
  it("maps host segments to navigation pages", () => {
    expect(deepLinkNavTarget("my-skills", "")).toBe("my-skills");
    expect(deepLinkNavTarget("marketplace", "")).toBe("marketplace");
    expect(deepLinkNavTarget("skill-cards", "")).toBe("skill-cards");
    expect(deepLinkNavTarget("projects", "")).toBe("projects");
    expect(deepLinkNavTarget("mcp", "")).toBe("mcp");
    expect(deepLinkNavTarget("settings", "")).toBe("settings");
    expect(deepLinkNavTarget("models", "")).toBe("models");
  });

  it("maps the first path segment when host is absent", () => {
    expect(deepLinkNavTarget(null, "/marketplace/skill")).toBe("marketplace");
    expect(deepLinkNavTarget(null, "/models/cloud-sync")).toBe("models");
  });

  it("accepts case variations", () => {
    expect(deepLinkNavTarget("Models", "")).toBe("models");
    expect(deepLinkNavTarget(null, "/MY-SKILLS")).toBe("my-skills");
  });

  it("returns null for unknown targets", () => {
    expect(deepLinkNavTarget("unknown", "")).toBeNull();
    expect(deepLinkNavTarget(null, "/nope")).toBeNull();
    expect(deepLinkNavTarget(null, "")).toBeNull();
  });
});

describe("mcpImportQuery", () => {
  it("treats url/catalog/config/command as an install intent", () => {
    expect(mcpImportQuery("url=https://example.com/mcp")).toBe("url=https://example.com/mcp");
    expect(mcpImportQuery("catalog=io.github.foo/bar")).toBe("catalog=io.github.foo/bar");
    expect(mcpImportQuery("config=%7B%7D")).toBe("config=%7B%7D");
    expect(mcpImportQuery("command=npx+-y+demo")).toBe("command=npx+-y+demo");
  });

  it("ignores empty or unrelated query strings", () => {
    expect(mcpImportQuery(null)).toBeNull();
    expect(mcpImportQuery("")).toBeNull();
    expect(mcpImportQuery("tab=fleet")).toBeNull();
  });

  it("reconstructs parser input from the OS payload", () => {
    expect(
      mcpImportPasteText({ url: "skillstar://mcp?catalog=io.github.foo/bar", query: "catalog=io.github.foo/bar" }),
    ).toBe("skillstar://mcp?catalog=io.github.foo/bar");
    expect(mcpImportPasteText({ url: null, query: "url=https://example.com/mcp" })).toBe(
      "skillstar://mcp?url=https://example.com/mcp",
    );
  });
});
