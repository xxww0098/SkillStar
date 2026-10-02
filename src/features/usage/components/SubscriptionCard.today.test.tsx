import { render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import type { CatalogEntry, ConsumptionTotals, Subscription } from "../types";
import { SubscriptionCard } from "./SubscriptionCard";

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

const catalog: CatalogEntry = {
  id: "deepseek",
  display_name: "DeepSeek",
  description: "",
  tier: "api-key",
  auth_modes: ["api-key"],
  brand_color: "1E6FFF",
  default_currency: "CNY",
  subscription_url: "",
  warning: null,
  regions: [],
};

const subscription: Subscription = {
  id: "sub-1",
  catalog_id: "deepseek",
  display_name: "DeepSeek · work",
  auth_mode: "api-key",
  plan_tier: null,
  monthly_price: null,
  currency: "CNY",
  billing_cycle: "api-key",
  start_date: 0,
  renew_date: 0,
  auto_renew: false,
  has_credential: true,
  has_platform_token: false,
  requires_reauth: false,
  is_active: false,
  supports_cli_switch: false,
  manual_quota: null,
  note: null,
  sort_index: 0,
  created_at: 0,
  updated_at: 0,
  usage: null,
};

const today: ConsumptionTotals = {
  calls: 4,
  errors: 1,
  input: 2_400,
  output: 600,
  cache_read: 100,
  cache_write: 0,
  reasoning: 0,
  cost_usd: 0.0184,
  unpriced: 0,
  mean_latency_ms: 1_450,
};

const callbacks = {
  onRefresh: vi.fn(),
  onEdit: vi.fn(),
  onDelete: vi.fn(),
};

describe("SubscriptionCard today line", () => {
  it("renders the today line inside the body when totals arrive", () => {
    render(<SubscriptionCard subscription={subscription} catalog={catalog} todayConsumption={today} {...callbacks} />);

    expect(screen.getByTestId("today-consumption-line")).toBeInTheDocument();
    // 2400 + 600 + 100 tokens fold to 3.1k.
    expect(screen.getByText("3.1k")).toBeInTheDocument();
    expect(screen.getByText("$0.0184（估算）· 经网关")).toBeInTheDocument();
  });

  it("shows the fixed no-records sentence when the read landed empty", () => {
    render(<SubscriptionCard subscription={subscription} catalog={catalog} todayConsumption={null} {...callbacks} />);

    expect(screen.getByTestId("today-consumption-line")).toBeInTheDocument();
    expect(screen.getByText("还没有记录")).toBeInTheDocument();
  });

  it("hides the line entirely while the summary read is in flight", () => {
    render(<SubscriptionCard subscription={subscription} catalog={catalog} {...callbacks} />);

    expect(screen.queryByTestId("today-consumption-line")).not.toBeInTheDocument();
  });
});
