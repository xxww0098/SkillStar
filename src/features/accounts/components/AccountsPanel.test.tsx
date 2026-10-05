import { render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { FILTER_ALL, type CatalogFilter } from "@/features/usage";
import { AccountsPanel } from "./AccountsPanel";

const reload = vi.fn();

vi.mock("@/features/usage", async () => {
  const actual = await vi.importActual<typeof import("@/features/usage")>("@/features/usage");
  return {
    ...actual,
    useUsageDataContext: () => ({
      loading: false,
      error: null,
      subscriptions: [],
      catalog: [],
      cliAccounts: {},
      alerts: [],
      refreshingAll: false,
      refreshBusy: false,
      autoRefresh: {
        autoRefreshEnabled: false,
        intervalMs: 300000,
        setAutoRefreshEnabled: vi.fn(),
        setIntervalMs: vi.fn(),
      },
      refreshAllWithUi: vi.fn(),
      refreshOneWithUi: vi.fn(),
      resetQuotaWithUi: vi.fn(),
      reorder: vi.fn(),
      remove: vi.fn(),
      dismissAlert: vi.fn(),
      setActive: vi.fn(),
      switchActiveToCli: vi.fn(),
      reload,
    }),
  };
});

function renderPanel(filter: CatalogFilter = FILTER_ALL) {
  return render(
    <AccountsPanel filter={filter} accountsCreateRequest={null} clearAccountsCreateRequest={() => undefined} />,
  );
}

describe("AccountsPanel", () => {
  it("renders the accounts header and empty grid", () => {
    renderPanel();
    expect(screen.getByRole("heading", { level: 1, name: /账号|accounts/i })).toBeInTheDocument();
  });

  it("opens the create dialog when a create request arrives", async () => {
    const { rerender } = renderPanel();
    rerender(
      <AccountsPanel
        filter="cursor"
        accountsCreateRequest={{ nonce: 1, preselectCatalogId: "cursor" }}
        clearAccountsCreateRequest={() => undefined}
      />,
    );
    // The dialog opens for the preselected provider; the request is consumed.
    expect(await screen.findByRole("dialog")).toBeInTheDocument();
  });
});
