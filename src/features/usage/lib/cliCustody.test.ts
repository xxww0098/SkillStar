import { describe, expect, it } from "vitest";
import type { CliAccountState, Subscription } from "../types";
import { cliAccountBadge, cliAccountBadgeFor, isDegradedCopyBinding } from "./cliCustody";

function row(id: string, isActive: boolean, catalogId = "xai") {
  return { id, catalog_id: catalogId, is_active: isActive } as Pick<Subscription, "id" | "catalog_id" | "is_active">;
}

const linkedTo = (subscriptionId: string): CliAccountState => ({ kind: "linkedTo", subscriptionId });

describe("cliAccountBadge", () => {
  it("gives the badge to the account the CLI is serving, not the one the pin names", () => {
    // The user switched back inside the CLI; the pin never heard about it.
    expect(cliAccountBadge(row("alice", true), linkedTo("bob"))).toBe("diverged");
    expect(cliAccountBadge(row("bob", false), linkedTo("bob"))).toBe("current");
  });

  it("says the CLI is signed out rather than letting the pin claim it is current", () => {
    expect(cliAccountBadge(row("alice", true), { kind: "missing" })).toBe("missing");
  });

  it("says diverged when somebody unknown is signed in", () => {
    expect(cliAccountBadge(row("alice", true), { kind: "diverged" })).toBe("diverged");
  });

  it("keeps quiet on rows that never claimed anything", () => {
    expect(cliAccountBadge(row("carol", false), { kind: "missing" })).toBe("none");
    expect(cliAccountBadge(row("carol", false), { kind: "diverged" })).toBe("none");
    expect(cliAccountBadge(row("carol", false), linkedTo("bob"))).toBe("none");
  });

  it("falls back to the pin when there is no live state to read", () => {
    // Catalogs with no CLI behind them, and the first render before the
    // reconcile lands: the pin is all there is, and it is not wrong there.
    expect(cliAccountBadge(row("alice", true), undefined)).toBe("current");
    expect(cliAccountBadge(row("alice", false), undefined)).toBe("none");
  });

  it("reads the live state of its own catalog only", () => {
    const states = { xai: linkedTo("bob"), codex: linkedTo("dana") };
    expect(cliAccountBadgeFor(row("alice", true, "xai"), states)).toBe("diverged");
    expect(cliAccountBadgeFor(row("dana", true, "codex"), states)).toBe("current");
    expect(cliAccountBadgeFor(row("glm-1", true, "glm"), states)).toBe("current");
  });
});

describe("isDegradedCopyBinding", () => {
  it("only flags a copy binding", () => {
    expect(isDegradedCopyBinding({ linkMode: "copy" })).toBe(true);
    expect(isDegradedCopyBinding({ linkMode: "symlink" })).toBe(false);
    expect(isDegradedCopyBinding({ linkMode: null })).toBe(false);
    expect(isDegradedCopyBinding(null)).toBe(false);
    expect(isDegradedCopyBinding(undefined)).toBe(false);
  });
});
