import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { openExternalUrl } from "../../../../lib/externalOpen";
import { useBalanceQuery } from "../../api/balance";
import { useLatencyTest } from "../../api/diagnostics";
import { ConnectionStatusPanel } from "./ConnectionStatusPanel";

const mocks = vi.hoisted(() => ({
  testConnection: vi.fn(),
  refreshBalance: vi.fn(),
}));

vi.mock("../../api/diagnostics", () => ({
  useLatencyTest: vi.fn(),
  useEndpointSpeedTest: () => ({
    testEndpoints: vi.fn(),
    clearResults: vi.fn(),
    results: [],
    isLoading: false,
  }),
}));

vi.mock("../../api/balance", () => ({
  useBalanceQuery: vi.fn(),
}));

vi.mock("../../../../lib/externalOpen", () => ({
  openExternalUrl: vi.fn(),
}));

function renderPanel(presetId = "deepseek", apiKey = "sk-test") {
  return render(
    <ConnectionStatusPanel
      providerId="provider-1"
      presetId={presetId}
      apiKey={apiKey}
      baseUrl="https://api.example.com/v1"
    />,
  );
}

beforeEach(() => {
  vi.mocked(useLatencyTest).mockReturnValue({
    testConnection: mocks.testConnection,
    isLoading: false,
    result: null,
  });
  vi.mocked(useBalanceQuery).mockReturnValue({
    balance: {
      available: 12.34,
      currency: "CNY",
      updated_at: Date.now(),
    },
    isLoading: false,
    error: null,
    refresh: mocks.refreshBalance,
  });
});

afterEach(() => {
  cleanup();
  vi.clearAllMocks();
});

describe("ConnectionStatusPanel", () => {
  it("renders the balance card below the connection test card for supported presets", () => {
    renderPanel();

    const connectionHeading = screen.getByRole("heading", { name: "连接状态" });
    const balanceHeading = screen.getByRole("heading", { name: "余额" });

    expect(connectionHeading.compareDocumentPosition(balanceHeading)).toBe(Node.DOCUMENT_POSITION_FOLLOWING);
    expect(screen.getByText("¥12.34")).toBeInTheDocument();
    expect(screen.getByText("账户余额")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "刷新" })).toBeEnabled();
    expect(screen.getByRole("button", { name: "控制台" })).toBeEnabled();
  });

  it("does not render balance UI for presets without a balance endpoint", () => {
    renderPanel("anthropic");

    expect(vi.mocked(useBalanceQuery)).toHaveBeenCalledWith(null, "sk-test", "https://api.example.com/v1");
    expect(screen.queryByRole("heading", { name: "余额" })).not.toBeInTheDocument();
  });

  it("opens the provider console and refreshes balance from the balance card actions", () => {
    renderPanel("openrouter");

    fireEvent.click(screen.getByRole("button", { name: "刷新" }));
    fireEvent.click(screen.getByRole("button", { name: "控制台" }));

    expect(mocks.refreshBalance).toHaveBeenCalledOnce();
    expect(openExternalUrl).toHaveBeenCalledWith("https://openrouter.ai/settings/credits");
  });

  it("shows a disabled refresh action before an API key is configured", () => {
    vi.mocked(useBalanceQuery).mockReturnValue({
      balance: null,
      isLoading: false,
      error: null,
      refresh: mocks.refreshBalance,
    });

    renderPanel("deepseek", "");

    expect(screen.getByText("--")).toBeInTheDocument();
    expect(screen.getByText("未配置 API Key")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "刷新" })).toBeDisabled();
  });
});
