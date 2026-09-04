import { describe, expect, it } from "vitest";
import { desktopAppIdForCatalog, desktopAppsForFilter, isGrokBotFilter } from "./desktopApps";

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
});
