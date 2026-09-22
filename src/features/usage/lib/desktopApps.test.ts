import { describe, expect, it } from "vitest";
import { INSTANCE_CATALOG_IDS, desktopAppIdForCatalog, desktopAppsForFilter, isGrokBotFilter } from "./desktopApps";
import { LOCAL_IMPORT_CATALOG_IDS } from "../types";

describe("desktopAppIdForCatalog", () => {
  it("maps Cursor and Antigravity quota cards to desktop apps", () => {
    expect(desktopAppIdForCatalog("cursor")).toBe("cursor");
    expect(desktopAppIdForCatalog("antigravity")).toBe("antigravity");
  });

  it("does not treat xai or anthropic quota cards as launchers", () => {
    expect(desktopAppIdForCatalog("xai")).toBeNull();
    expect(desktopAppIdForCatalog("anthropic")).toBeNull();
    expect(desktopAppIdForCatalog("grok-bot")).toBeNull();
  });
});

describe("desktopAppsForFilter", () => {
  it("keeps Grok Bot off the xai provider page", () => {
    expect(desktopAppsForFilter("xai")).toBeNull();
    expect(isGrokBotFilter("xai")).toBe(false);
    expect(desktopAppsForFilter("grok-bot")).toEqual(["grok-bot"]);
  });

  it("does not list pending or blocked apps", () => {
    expect(INSTANCE_CATALOG_IDS).toEqual(["cursor", "antigravity"]);
    expect(LOCAL_IMPORT_CATALOG_IDS).toContain("windsurf");
    expect(LOCAL_IMPORT_CATALOG_IDS).not.toContain("github-copilot");
    expect(desktopAppsForFilter("__all__")).toEqual(["cursor", "grok-bot", "antigravity"]);
    for (const id of [
      "windsurf",
      "kiro",
      "qoder",
      "codebuddy",
      "codebuddy-cn",
      "zcode",
      "trae",
      "trae-solo",
      "trae-cn",
      "trae-solo-cn",
      "zed",
      "github-copilot",
    ]) {
      expect(desktopAppIdForCatalog(id)).toBeNull();
      expect(desktopAppsForFilter(id)).toBeNull();
    }
  });
});
