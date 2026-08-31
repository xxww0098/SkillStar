import { describe, expect, it, vi } from "vitest";
import { displayAccountIdentity, readHideAccountEmails, writeHideAccountEmails } from "./accountPrivacy";

describe("account privacy", () => {
  it("masks email identities without changing custom labels", () => {
    expect(displayAccountIdentity("alice@example.com", true)).toBe("••••••••");
    expect(displayAccountIdentity("Personal account", true)).toBe("Personal account");
    expect(displayAccountIdentity("alice@example.com", false)).toBe("alice@example.com");
  });

  it("persists the visibility preference", () => {
    const storage = new Map<string, string>();
    vi.stubGlobal("localStorage", {
      getItem: (key: string) => storage.get(key) ?? null,
      setItem: (key: string, value: string) => storage.set(key, value),
    });

    expect(readHideAccountEmails()).toBe(false);
    writeHideAccountEmails(true);
    expect(readHideAccountEmails()).toBe(true);
    writeHideAccountEmails(false);
    expect(readHideAccountEmails()).toBe(false);

    vi.unstubAllGlobals();
  });
});
