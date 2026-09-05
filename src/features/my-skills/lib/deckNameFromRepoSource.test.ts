import { describe, expect, it } from "vitest";
import { deckNameFromRepoSource } from "./deckNameFromRepoSource";

describe("deckNameFromRepoSource", () => {
  it("takes the name after the slash in owner/repo", () => {
    expect(deckNameFromRepoSource("owner/orca")).toBe("orca");
    expect(deckNameFromRepoSource("vercel-labs/agent-skills")).toBe("agent-skills");
  });

  it("strips a trailing slash and optional .git", () => {
    expect(deckNameFromRepoSource("owner/orca/")).toBe("orca");
    expect(deckNameFromRepoSource("owner/orca.git")).toBe("orca");
  });

  it("accepts a clone URL as a fallback", () => {
    expect(deckNameFromRepoSource("https://github.com/owner/orca")).toBe("orca");
    expect(deckNameFromRepoSource("https://github.com/owner/orca.git")).toBe("orca");
  });

  it("returns the whole token when there is no slash", () => {
    expect(deckNameFromRepoSource("orca")).toBe("orca");
  });

  it("returns empty for blank input", () => {
    expect(deckNameFromRepoSource("")).toBe("");
    expect(deckNameFromRepoSource("   ")).toBe("");
  });
});
