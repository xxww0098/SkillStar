import { describe, expect, it } from "vitest";
import { monthlyEquivalentPrice, totalSpendForSubscription } from "./pricing";
import type { Subscription } from "../types";

function sub(overrides: Partial<Subscription>): Subscription {
  return {
    id: "s1",
    catalog_id: "deepseek",
    display_name: "DS",
    auth_mode: "api-key",
    plan_tier: null,
    monthly_price: 100,
    currency: "CNY",
    billing_cycle: "monthly",
    start_date: 1_700_000_000,
    renew_date: 0,
    auto_renew: false,
    has_credential: true,
    requires_reauth: false,
    manual_quota: null,
    note: null,
    sort_index: 0,
    created_at: 1_700_000_000,
    updated_at: 1_700_000_000,
    usage: null,
    ...overrides,
  };
}

describe("pricing billing cycles", () => {
  it("api-key has no monthly equivalent", () => {
    expect(monthlyEquivalentPrice(sub({ billing_cycle: "api-key", monthly_price: 50 }))).toBeNull();
  });

  it("api-key counts package price once as total spend", () => {
    expect(totalSpendForSubscription(sub({ billing_cycle: "api-key", monthly_price: 50 }))).toBe(50);
  });

  it("monthly returns price as monthly burn", () => {
    expect(monthlyEquivalentPrice(sub({ billing_cycle: "monthly", monthly_price: 20 }))).toBe(20);
  });
});
