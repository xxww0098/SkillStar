import { describe, expect, it } from "vitest";
import { isCookieMode, selectableAuthModes } from "./authModes";

describe("selectableAuthModes", () => {
  it("keeps cookie so cookie-only catalogs can be bound at all", () => {
    // `catalog.rs` gives stepfun / opencode `[Cookie, Manual]`. Dropping cookie
    // here left the dialog with an empty list, whose `?? "o-auth"` fallback
    // pointed a cookie-only provider at `start_oauth_login`.
    expect(selectableAuthModes(["cookie", "manual"])).toEqual(["cookie"]);
  });

  it("drops manual, which still has no form", () => {
    expect(selectableAuthModes(["manual"])).toEqual([]);
  });

  it("preserves catalog order for the modes it can drive", () => {
    expect(selectableAuthModes(["o-auth", "api-key", "cookie", "manual"])).toEqual(["o-auth", "api-key", "cookie"]);
  });

  it("leaves the single-mode catalogs untouched", () => {
    expect(selectableAuthModes(["o-auth"])).toEqual(["o-auth"]);
    expect(selectableAuthModes(["api-key"])).toEqual(["api-key"]);
    expect(selectableAuthModes([])).toEqual([]);
  });
});

describe("isCookieMode", () => {
  it("is true only for the pasted-cookie credential", () => {
    expect(isCookieMode("cookie")).toBe(true);
    for (const mode of ["o-auth", "api-key", "manual"] as const) {
      expect(isCookieMode(mode)).toBe(false);
    }
  });
});
