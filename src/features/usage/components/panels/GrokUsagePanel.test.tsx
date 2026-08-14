import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import type { SubscriptionUsage } from "../../types";
import { GrokUsagePanel, parseMonthSpendCents } from "./GrokUsagePanel";

function baseUsage(overrides: Partial<SubscriptionUsage> = {}): SubscriptionUsage {
  return {
    subscription_id: "sub-xai",
    fetched_at: Math.floor(Date.now() / 1000),
    plan_name: "Grok",
    hourly: null,
    weekly: null,
    monthly: null,
    balance: null,
    credits: [],
    error: null,
    api_keys: [],
    ...overrides,
  };
}

describe("parseMonthSpendCents", () => {
  it("parses used/total cents", () => {
    expect(parseMonthSpendCents("7006/20000")).toEqual({ used: 7006, total: 20000 });
  });

  it("parses used-only cents", () => {
    expect(parseMonthSpendCents("1234")).toEqual({ used: 1234, total: null });
  });

  it("rejects dollar strings", () => {
    expect(parseMonthSpendCents("$20")).toBeNull();
  });
});

describe("GrokUsagePanel", () => {
  it("renders weekly remaining percent and secondary month spend", () => {
    const resetAt = Math.floor(Date.now() / 1000) + 4 * 86_400;
    render(
      <GrokUsagePanel
        usage={baseUsage({
          weekly: {
            label: "Weekly credits",
            used: 0,
            total: undefined,
            percent: 30,
            reset_at: resetAt,
            breakdown: [],
          },
          credits: [
            { credit_type: "grok-month-spend", credit_amount: "7006/20000" },
            { credit_type: "grok-on-demand-cap", credit_amount: "$20" },
          ],
        })}
      />,
    );

    // Remaining-only figure (used % is not shown to avoid twin labels).
    expect(screen.getByText(/70%/)).toBeInTheDocument();
    expect(screen.queryByText(/30%/)).not.toBeInTheDocument();
    expect(screen.getByText(/\$70\.06/)).toBeInTheDocument();
    expect(screen.getByText(/\$200/)).toBeInTheDocument();
    expect(screen.getByText("$20")).toBeInTheDocument();
  });

  it("does not invent a monthly quota bar for weekly plans", () => {
    render(
      <GrokUsagePanel
        usage={baseUsage({
          weekly: {
            label: "Weekly credits",
            used: 0,
            total: undefined,
            percent: 10,
            reset_at: undefined,
            breakdown: [],
          },
          monthly: null,
          credits: [{ credit_type: "grok-month-spend", credit_amount: "100/5000" }],
        })}
      />,
    );

    // Month spend is text, not a second progress "Monthly credits" bar.
    expect(screen.queryByText(/Monthly credits|本月(?!花费)/)).not.toBeInTheDocument();
    expect(screen.getByText(/\$1/)).toBeInTheDocument();
  });
});
