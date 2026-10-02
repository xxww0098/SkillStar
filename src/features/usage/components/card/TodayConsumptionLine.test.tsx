import { render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import type { ConsumptionTotals } from "../../types";
import { TodayConsumptionLine, formatEstimateUsd } from "./TodayConsumptionLine";

vi.mock("react-i18next", () => ({
  useTranslation: () => ({
    t: (key: string, opts?: Record<string, unknown>) => {
      if (key === "usage.todayConsumptionLabel") return "今日消耗";
      if (key === "usage.todayConsumptionEmpty") return "还没有记录";
      if (key === "usage.todayConsumptionCost") return `${opts?.cost}（估算）· 经网关`;
      return key;
    },
  }),
}));

function totals(overrides: Partial<ConsumptionTotals> = {}): ConsumptionTotals {
  return {
    calls: 3,
    errors: 0,
    input: 300,
    output: 120,
    cache_read: 30,
    cache_write: 0,
    reasoning: 0,
    cost_usd: 0.0184,
    unpriced: 0,
    mean_latency_ms: 1_450,
    ...overrides,
  };
}

describe("TodayConsumptionLine", () => {
  it("renders big mono tokens with the cost always marked as an estimate", () => {
    render(<TodayConsumptionLine today={totals()} />);

    const line = screen.getByTestId("today-consumption-line");
    // 300 + 120 + 30 tokens, formatted as one plain count.
    expect(screen.getByText("450")).toBeInTheDocument();
    expect(screen.getByText("$0.0184（估算）· 经网关")).toBeInTheDocument();
    expect(screen.getByText("今日消耗")).toBeInTheDocument();
    expect(line.tagName).toBe("DIV");
  });

  it("keeps reasoning tokens out of the headline (they bill inside output)", () => {
    render(<TodayConsumptionLine today={totals({ reasoning: 80, output: 120 })} />);

    expect(screen.getByText("450")).toBeInTheDocument();
  });

  it("renders the fixed no-records sentence for a read that landed empty", () => {
    render(<TodayConsumptionLine today={null} />);
    expect(screen.getByText("还没有记录")).toBeInTheDocument();

    // A provider with zero calls today reads the same as one never metered.
    render(<TodayConsumptionLine today={totals({ calls: 0 })} />);
    expect(screen.getAllByText("还没有记录")).toHaveLength(2);
  });

  it("never hides a zero-cost read behind the empty sentence", () => {
    // Calls happened but nothing priced: tokens still lead, cost shows $0.
    render(<TodayConsumptionLine today={totals({ cost_usd: 0, unpriced: 3 })} />);
    expect(screen.getByText("450")).toBeInTheDocument();
    expect(screen.getByText("$0（估算）· 经网关")).toBeInTheDocument();
    expect(screen.queryByText("还没有记录")).not.toBeInTheDocument();
  });
});

describe("formatEstimateUsd", () => {
  it("uses four decimals under a dollar and two above", () => {
    expect(formatEstimateUsd(0.0184)).toBe("$0.0184");
    expect(formatEstimateUsd(0)).toBe("$0");
    expect(formatEstimateUsd(1.5)).toBe("$1.50");
    expect(formatEstimateUsd(Number.NaN)).toBe("$0");
  });
});
