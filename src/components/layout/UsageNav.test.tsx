import { render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { FILTER_ALL, type CatalogEntry, type Subscription } from "@/features/usage/types";

vi.mock("react-i18next", () => ({
  useTranslation: () => ({
    t: (key: string, opts?: Record<string, unknown>) => {
      if (key === "usage.allSubscriptions") return "全部订阅";
      if (key === "usage.searchCatalog") return "搜索订阅商...";
      if (key === "usage.sidebarNav") return "供应商";
      if (key === "usage.addSubscription") return "新增订阅";
      return opts ? `${key}:${JSON.stringify(opts)}` : key;
    },
  }),
}));

vi.mock("@/features/usage/context/UsageDataContext", () => ({
  useUsageDataContext: () => ({
    catalog: [
      {
        id: "xai",
        display_name: "Grok",
        description: "xAI Grok CLI",
        brand_color: "2563EB",
      } as CatalogEntry,
      {
        id: "codex",
        display_name: "Codex",
        description: "OpenAI Codex",
        brand_color: "3B82F6",
      } as CatalogEntry,
    ],
    subscriptions: [{ catalog_id: "xai" }, { catalog_id: "codex" }] as Subscription[],
  }),
}));

vi.mock("@/features/usage/components/ProviderLogo", () => ({
  ProviderLogo: () => <div data-testid="provider-logo" />,
}));

import { UsageNav } from "./UsageNav";

describe("UsageNav", () => {
  it("hides the add button when all subscriptions is selected", () => {
    render(<UsageNav selected={FILTER_ALL} onSelect={vi.fn()} collapsed={false} />);

    expect(screen.getByRole("button", { name: /全部订阅/ })).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "新增订阅" })).not.toBeInTheDocument();
  });

  it("keeps the add button out of navigation when a provider is selected", () => {
    render(<UsageNav selected="xai" onSelect={vi.fn()} collapsed={false} />);

    expect(screen.getByRole("button", { name: /Grok/ })).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "新增订阅" })).not.toBeInTheDocument();
  });
});
