import { render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { FILTER_ALL } from "../types";
import { UsagePanel } from "./UsagePanel";

vi.mock("react-i18next", () => ({
  useTranslation: () => ({
    t: (key: string) => key,
  }),
}));

vi.mock("../context/UsageDataContext", () => ({
  useUsageDataContext: () => ({
    loading: true,
    error: null,
    subscriptions: [],
    catalog: [],
    alerts: [],
    todayByCatalog: undefined,
    todayConsumption: null,
    refreshingAll: false,
    refreshBusy: false,
    refreshAllWithUi: vi.fn(),
    autoRefresh: {
      autoRefreshEnabled: false,
      intervalMs: 60_000,
      setAutoRefreshEnabled: vi.fn(),
      setIntervalMs: vi.fn(),
    },
    reorder: vi.fn(),
    dismissAlert: vi.fn(),
  }),
}));

describe("UsagePanel loading", () => {
  it("renders the card skeleton before usage data arrives", () => {
    render(
      <UsagePanel
        filter={FILTER_ALL}
        usageCreateRequest={null}
        clearUsageCreateRequest={() => undefined}
        onFocusModelsAgent={() => undefined}
        onFocusModelsCatalog={() => undefined}
      />,
    );

    const status = screen.getByRole("status", { name: "usage.loading" });
    expect(status.querySelectorAll(".grid > div")).toHaveLength(6);
  });
});
