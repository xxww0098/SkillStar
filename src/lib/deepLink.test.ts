import { describe, expect, it } from "vitest";
import { deepLinkNavTarget } from "./deepLink";

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
