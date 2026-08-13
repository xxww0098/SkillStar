import { describe, expect, it } from "vitest";
import { deckInstallFailureDetails, installErrorMessage } from "./deckInstallFailures";

describe("deckInstallFailureDetails", () => {
  it("lists every failed Skill and preserves the first backend reason", () => {
    expect(
      deckInstallFailureDetails([
        { name: "old-matt-skill", error: "source may have been deleted or renamed" },
        { name: "another-skill", error: "network failed" },
      ]),
    ).toEqual({
      names: "old-matt-skill, another-skill",
      reason: "source may have been deleted or renamed",
    });
  });
});

describe("installErrorMessage", () => {
  it("extracts Error and string reasons", () => {
    expect(installErrorMessage(new Error("backend detail"))).toBe("backend detail");
    expect(installErrorMessage("plain detail")).toBe("plain detail");
  });
});
