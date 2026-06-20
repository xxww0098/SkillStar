import { render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import type { SubscriptionUsage } from "../types";
import { DeepSeekUsagePanel } from "./DeepSeekUsagePanel";

vi.mock("react-i18next", () => ({
  useTranslation: () => ({
    t: (key: string, opts?: Record<string, unknown>) => {
      const labels: Record<string, string> = {
        "usage.deepseekAccountStatus": "账户状态",
        "usage.deepseekAvailable": "可调用",
        "usage.deepseekUnavailable": "余额不足",
        "usage.deepseekTotalBalance": "可用余额",
        "usage.deepseekPaygHint": "按量计费",
        "usage.deepseekBalanceBreakdown": "余额构成",
        "usage.deepseekGrantedBalance": "赠送余额",
        "usage.deepseekToppedUpBalance": "充值余额",
        "usage.deepseekOtherCurrencies": "其他币种",
        "usage.numberUnit10k": "万",
        "usage.deepseekAnalyticsHint": "配置平台 Token",
        "usage.deepseekTodayCost": "当日消耗",
        "usage.deepseekMonthCost": "本月消费",
        "usage.deepseekTokens": "Tokens",
        "usage.deepseekCacheHitRate": `缓存命中 ${opts?.rate}%`,
        "usage.deepseekTrendTitle": "近 7 天 Token 趋势",
        "usage.deepseekTrendSummary": `命中率 ${opts?.rate}% · 合计 ${opts?.total}`,
        "usage.deepseekCacheHit": "缓存命中",
        "usage.deepseekCacheMiss": "未命中",
        "usage.deepseekResponse": "输出",
      };
      return labels[key] ?? key;
    },
  }),
}));

const baseUsage: SubscriptionUsage = {
  subscription_id: "sub-deepseek",
  fetched_at: 1,
  plan_name: null,
  hourly: null,
  weekly: null,
  monthly: null,
  balance: {
    currency: "CNY",
    total: 48.5,
    granted: 5,
    topped_up: 43.5,
    is_available: true,
  },
  credits: [{ credit_type: "deepseek-balance:USD", credit_amount: "2.00" }],
  error: null,
  api_keys: [],
};

describe("DeepSeekUsagePanel", () => {
  it("renders account status, primary balance, breakdown, and secondary currency", () => {
    render(<DeepSeekUsagePanel usage={baseUsage} />);

    expect(screen.getByText("账户状态")).toBeInTheDocument();
    expect(screen.getByText("可调用")).toBeInTheDocument();
    expect(screen.getByText("可用余额")).toBeInTheDocument();
    expect(screen.getByText("¥48.50")).toBeInTheDocument();
    expect(screen.getByText("赠送余额")).toBeInTheDocument();
    expect(screen.getByText("充值余额")).toBeInTheDocument();
    expect(screen.getByText("其他币种")).toBeInTheDocument();
    expect(screen.getByText("USD")).toBeInTheDocument();
    expect(screen.getByText("$2.00")).toBeInTheDocument();
    expect(screen.getByText("配置平台 Token")).toBeInTheDocument();
  });

  it("shows unavailable state when balance is not callable", () => {
    render(
      <DeepSeekUsagePanel
        usage={{
          ...baseUsage,
          balance: { ...baseUsage.balance!, is_available: false },
        }}
      />,
    );

    expect(screen.getByText("余额不足")).toBeInTheDocument();
  });

  it("renders model usage and trend chart when analytics are present", () => {
    render(
      <DeepSeekUsagePanel
        usage={{
          ...baseUsage,
          deepseek_analytics: {
            month_cost: 12.5,
            today_cost: 1.2,
            models: [
              {
                key: "flash",
                name: "V4 Flash",
                total_tokens: 1_000_000,
                request_count: 10,
                cache_hit_tokens: 700_000,
                cache_miss_tokens: 200_000,
                response_tokens: 100_000,
                cost: 8.5,
              },
              {
                key: "pro",
                name: "V4 Pro",
                total_tokens: 500_000,
                request_count: 5,
                cache_hit_tokens: 300_000,
                cache_miss_tokens: 100_000,
                response_tokens: 100_000,
                cost: 4.0,
              },
            ],
            daily: [
              {
                date: new Date().toISOString().slice(0, 10),
                flash_tokens: 1000,
                flash_cache_hit: 600,
                flash_cache_miss: 200,
                flash_response: 200,
                pro_tokens: 500,
                pro_cache_hit: 300,
                pro_cache_miss: 100,
                pro_response: 100,
                total_tokens: 1500,
                total_cost: 1.2,
              },
            ],
          },
        }}
      />,
    );

    expect(screen.getByText("当日消耗")).toBeInTheDocument();
    expect(screen.getByText("¥1.20")).toBeInTheDocument();
    expect(screen.getByText("本月消费")).toBeInTheDocument();
    expect(screen.getByText("¥12.50")).toBeInTheDocument();
    expect(screen.getByText("V4 Flash")).toBeInTheDocument();
    expect(screen.getByText("V4 Pro")).toBeInTheDocument();
    expect(screen.getByText("近 7 天 Token 趋势")).toBeInTheDocument();
    expect(screen.getByText("缓存命中 78%")).toBeInTheDocument();
  });
});
