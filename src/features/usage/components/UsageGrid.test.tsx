import { fireEvent, render, screen, within } from "@testing-library/react";
import type React from "react";
import { describe, expect, it, vi } from "vitest";
import { FILTER_ALL, type CatalogEntry, type Subscription } from "../types";
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
      if (key === "usage.collapseAllProviderGroups") return "全部折叠";
      if (key === "usage.expandAllProviderGroups") return "全部展开";
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

const codexCatalogEntry: CatalogEntry = {
  id: "codex",
  display_name: "Codex",
  description: "OpenAI Codex",
  tier: "o-auth",
  auth_modes: ["o-auth"],
  brand_color: "3B82F6",
  default_currency: "USD",
  subscription_url: "https://chat.openai.com/codex",
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
  it("groups all subscriptions by provider row", () => {
    const secondGrok = {
      ...subscription,
      id: "sub-2",
      display_name: "Grok · work",
      sort_index: 2,
    };
    const codexSub = {
      ...subscription,
      id: "sub-3",
      catalog_id: "codex",
      display_name: "Codex · personal",
      sort_index: 1,
    };

    render(
      <UsageGrid
        subscriptions={[subscription, secondGrok, codexSub]}
        allSubscriptions={[subscription, secondGrok, codexSub]}
        catalog={[catalogEntry, codexCatalogEntry]}
        filter={FILTER_ALL}
        onRefresh={vi.fn()}
        onEdit={vi.fn()}
        onDelete={vi.fn()}
        onReorder={vi.fn()}
        onAddNew={vi.fn()}
      />,
    );

    const grokRow = screen.getByRole("region", { name: "Grok 已有 2 张卡片" });
    const codexRow = screen.getByRole("region", { name: "Codex 已有 1 张卡片" });
    const grokToggle = within(grokRow).getByRole("button", { name: /Grok/ });

    expect(grokToggle).toHaveAttribute("aria-expanded", "true");
    expect(within(grokRow).getByText("Grok · xiewei63")).toBeInTheDocument();
    expect(within(grokRow).getByText("Grok · work")).toBeInTheDocument();
    expect(within(grokRow).queryByText("Codex · personal")).not.toBeInTheDocument();
    expect(within(codexRow).getByText("Codex · personal")).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "新增 Grok 订阅" })).not.toBeInTheDocument();

    fireEvent.click(grokToggle);

    expect(grokToggle).toHaveAttribute("aria-expanded", "false");
    expect(within(grokRow).queryByText("Grok · xiewei63")).not.toBeInTheDocument();
    expect(within(grokRow).queryByText("Grok · work")).not.toBeInTheDocument();
    expect(within(codexRow).getByText("Codex · personal")).toBeInTheDocument();
  });

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

  it("collapses and expands all provider groups from the batch control", () => {
    const codexSub = {
      ...subscription,
      id: "sub-3",
      catalog_id: "codex",
      display_name: "Codex · personal",
      sort_index: 1,
    };

    render(
      <UsageGrid
        subscriptions={[subscription, codexSub]}
        allSubscriptions={[subscription, codexSub]}
        catalog={[catalogEntry, codexCatalogEntry]}
        filter={FILTER_ALL}
        onRefresh={vi.fn()}
        onEdit={vi.fn()}
        onDelete={vi.fn()}
        onReorder={vi.fn()}
        onAddNew={vi.fn()}
      />,
    );

    fireEvent.click(screen.getByRole("button", { name: "全部折叠" }));

    expect(screen.getByRole("button", { name: "全部展开" })).toBeInTheDocument();
    expect(screen.queryByText("Grok · xiewei63")).not.toBeInTheDocument();
    expect(screen.queryByText("Codex · personal")).not.toBeInTheDocument();

    fireEvent.click(screen.getByRole("button", { name: "全部展开" }));

    expect(screen.getByRole("button", { name: "全部折叠" })).toBeInTheDocument();
    expect(screen.getByText("Grok · xiewei63")).toBeInTheDocument();
    expect(screen.getByText("Codex · personal")).toBeInTheDocument();
  });
});
