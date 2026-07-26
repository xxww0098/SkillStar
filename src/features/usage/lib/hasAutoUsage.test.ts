import { describe, expect, it } from "vitest";
import type { Subscription } from "../types";
import { computeHasAutoUsage } from "./hasAutoUsage";

function sub(over: Partial<Subscription> & { usage?: Subscription["usage"] }): Subscription {
  return {
    id: "s1",
    catalog_id: "deepseek",
    display_name: "DS",
    auth_mode: "api-key",
    plan_tier: null,
    monthly_price: null,
    currency: "CNY",
    billing_cycle: "monthly",
    start_date: 0,
    renew_date: 0,
    auto_renew: false,
    has_credential: true,
    requires_reauth: false,
    is_active: true,
    manual_quota: null,
    note: null,
    sort_index: 0,
    created_at: 0,
    updated_at: 0,
    usage: null,
    ...over,
  } as Subscription;
}

describe("computeHasAutoUsage", () => {
  it("false when no usage", () => {
    expect(computeHasAutoUsage(sub({ usage: null }))).toBe(false);
  });

  it("true when hourly present", () => {
    expect(
      computeHasAutoUsage(
        sub({
          usage: {
            subscription_id: "s1",
            fetched_at: 1,
            plan_name: null,
            hourly: { label: "5h", used: 1, total: 10, percent: 10, reset_at: null },
            weekly: null,
            monthly: null,
            balance: null,
            credits: [],
            error: null,
            api_keys: [],
          } as Subscription["usage"],
        }),
      ),
    ).toBe(true);
  });

  it("ignores deepseek-balance credits alone", () => {
    expect(
      computeHasAutoUsage(
        sub({
          usage: {
            subscription_id: "s1",
            fetched_at: 1,
            plan_name: null,
            hourly: null,
            weekly: null,
            monthly: null,
            balance: null,
            credits: [{ credit_type: "deepseek-balance:cash", credit_amount: "10" }],
            error: null,
            api_keys: [],
          } as Subscription["usage"],
        }),
      ),
    ).toBe(false);
  });
});
