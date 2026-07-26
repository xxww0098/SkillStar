import { describe, expect, it } from "vitest";
import type { Subscription } from "../types";
import { mergeActiveSubscriptionUpdate } from "./useUsageData";

function subscription(id: string, active: boolean): Subscription {
  return { id, catalog_id: "xai", is_active: active } as Subscription;
}

describe("mergeActiveSubscriptionUpdate", () => {
  it("keeps the previous active sibling when backend rolled the target back", () => {
    const bob = subscription("bob", true);
    const alice = subscription("alice", false);

    const merged = mergeActiveSubscriptionUpdate([bob, subscription("alice", false)], alice);

    expect(merged.find((row) => row.id === "bob")?.is_active).toBe(true);
    expect(merged.find((row) => row.id === "alice")?.is_active).toBe(false);
  });

  it("demotes the previous sibling only after backend confirms the target active", () => {
    const bob = subscription("bob", true);
    const alice = subscription("alice", true);

    const merged = mergeActiveSubscriptionUpdate([bob, subscription("alice", false)], alice);

    expect(merged.find((row) => row.id === "bob")?.is_active).toBe(false);
    expect(merged.find((row) => row.id === "alice")?.is_active).toBe(true);
  });
});
