import { fireEvent, render, screen } from "@testing-library/react";
import type React from "react";
import { describe, expect, it, vi } from "vitest";
import type { CatalogEntry, Subscription } from "../types";
import { UsageGrid } from "./UsageGrid";

vi.mock("framer-motion", () => ({
  Reorder: {
    Group: ({ children, className }: { children: React.ReactNode; className?: string }) => (
      <div className={className}>{children}</div>
    ),
    Item: ({ children }: { children: React.ReactNode }) => <div>{children}</div>,
  },
  useDragControls: () => ({ start: vi.fn() }),
}));

vi.mock("react-i18next", () => ({
  useTranslation: () => ({
    t: (key: string, opts?: Record<string, unknown>) => {
      if (key === "usage.providerSubscriptionCount") return `已有 ${opts?.count} 张卡片`;
      if (key === "usage.addProviderSubscription") return `新增 ${opts?.provider} 订阅`;
      return key;
    },
  }),
}));

vi.mock("./SubscriptionCard", () => ({
  SubscriptionCard: ({ subscription }: { subscription: Subscription }) => (
    <article aria-label={subscription.display_name}>{subscription.display_name}</article>
  ),
}));

const catalogEntry: CatalogEntry = {
  id: "xai",
  display_name: "Grok",
  description: "xAI Grok CLI",
  tier: "o-auth",
  auth_modes: ["o-auth"],
  brand_color: "2563EB",
  default_currency: "USD",
  subscription_url: "https://grok.com",
  warning: null,
  regions: [],
};

const subscription: Subscription = {
  id: "sub-1",
  catalog_id: "xai",
  display_name: "Grok · xiewei63",
  auth_mode: "o-auth",
  plan_tier: "GROK",
  monthly_price: null,
  currency: "USD",
  billing_cycle: "monthly",
  start_date: 0,
  renew_date: 0,
  auto_renew: false,
  has_credential: true,
  requires_reauth: false,
  manual_quota: null,
  note: null,
  sort_index: 0,
  created_at: 0,
  updated_at: 0,
  usage: null,
};

describe("UsageGrid", () => {
  it("shows a current-provider add button when that provider already has cards", () => {
    const onAddNew = vi.fn();

    render(
      <UsageGrid
        subscriptions={[subscription]}
        allSubscriptions={[subscription]}
        catalog={[catalogEntry]}
        filter="xai"
        onRefresh={vi.fn()}
        onEdit={vi.fn()}
        onDelete={vi.fn()}
        onReorder={vi.fn()}
        onAddNew={onAddNew}
      />,
    );

    expect(screen.getByText("已有 1 张卡片")).toBeInTheDocument();

    fireEvent.click(screen.getByRole("button", { name: "新增 Grok 订阅" }));

    expect(onAddNew).toHaveBeenCalledWith("xai");
  });
});
