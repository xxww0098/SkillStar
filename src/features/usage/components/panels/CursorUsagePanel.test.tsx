import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import type { SubscriptionUsage } from "../../types";
import { CursorUsagePanel, parseCentsProgress } from "./CursorUsagePanel";

function baseUsage(overrides: Partial<SubscriptionUsage> = {}): SubscriptionUsage {
  return {
    subscription_id: "sub-cursor",
    fetched_at: Math.floor(Date.now() / 1000),
    plan_name: "PRO",
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

describe("parseCentsProgress", () => {
  it("parses used/total cents", () => {
    expect(parseCentsProgress("1200/5000")).toEqual({ used: 1200, total: 5000 });
  });

  it("rejects dollar-only amounts", () => {
    expect(parseCentsProgress("$23.09")).toBeNull();
  });
});

describe("CursorUsagePanel", () => {
  it("renders plan bar plus on-demand and bonus", () => {
    render(
      <CursorUsagePanel
        usage={baseUsage({
          monthly: {
            label: "Total",
            used: 4653,
            total: 9495,
            percent: 49,
            reset_at: Math.floor(Date.now() / 1000) + 86_400,
            breakdown: [
              { label: "Auto + Composer", used: 51, total: 100, percent: 51, reset_at: null, breakdown: [] },
              { label: "API", used: 43, total: 100, percent: 43, reset_at: null, breakdown: [] },
            ],
          },
          credits: [
            { credit_type: "cursor-bonus", credit_amount: "$20" },
            { credit_type: "cursor-on-demand", credit_amount: "1200/5000" },
          ],
        })}
      />,
    );

    // Monetary plan totals (USD cents → dollars). The unified meter renders
    // used + total as one `$used / $total` figure, so total lives in the unit node.
    expect(screen.getByText("$46.53")).toBeInTheDocument();
    expect(screen.getByText((content) => content.includes("$94.95"))).toBeInTheDocument();
    // Secondary credits (bonus $20; on-demand $12 / $50 may be split across nodes).
    expect(screen.getByText("$20")).toBeInTheDocument();
    expect(screen.getByText("$12")).toBeInTheDocument();
    expect(screen.getByText((content) => content.includes("$50"))).toBeInTheDocument();
  });
});
