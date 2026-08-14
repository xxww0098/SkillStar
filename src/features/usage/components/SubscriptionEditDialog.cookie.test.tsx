import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { CatalogEntry } from "../types";
import { SubscriptionEditDialog } from "./SubscriptionEditDialog";

const createSubscription = vi.fn();
const startOAuthLogin = vi.fn();

vi.mock("../api", () => ({
  usageApi: {
    createSubscription: (...args: unknown[]) => createSubscription(...args),
    updateSubscription: vi.fn(),
    refreshSubscriptionUsage: vi.fn(),
    startOAuthLogin: (...args: unknown[]) => startOAuthLogin(...args),
    awaitOAuthCompletion: vi.fn(),
    submitOAuthCallback: vi.fn(),
    cancelOAuthLogin: vi.fn(),
    importSubscriptionFromLocal: vi.fn(),
  },
}));

vi.mock("sonner", () => ({
  toast: { success: vi.fn(), error: vi.fn(), warning: vi.fn() },
}));

// The returned object must be referentially stable: the dialog's form-reset
// effect lists `t` in its dependency array, so a fresh `t` on every render
// would re-run the reset and wipe whatever the user just typed. Real i18next
// hands back a stable `t`.
const translation = { t: (key: string) => key };
vi.mock("react-i18next", () => ({
  useTranslation: () => translation,
}));

/** Cookie-only catalog, mirroring `catalog.rs`'s `COOKIE_MANUAL` entries. */
const stepfun: CatalogEntry = {
  id: "stepfun",
  display_name: "阶跃 Step",
  description: "账户余额 / 消费",
  tier: "cookie",
  auth_modes: ["cookie", "manual"],
  brand_color: "00B5A9",
  default_currency: "CNY",
  subscription_url: "https://platform.stepfun.com/account-overview",
  warning: "请使用 Cookie 模式，复制包含 Oasis-Token 的 Cookie。",
  regions: [],
};

function renderDialog() {
  return render(
    <SubscriptionEditDialog
      open
      catalog={[stepfun]}
      editing={null}
      preselectCatalogId="stepfun"
      onClose={vi.fn()}
      onCreated={vi.fn()}
      onUpdated={vi.fn()}
      onDeleted={vi.fn()}
    />,
  );
}

describe("SubscriptionEditDialog — Cookie mode", () => {
  beforeEach(() => {
    createSubscription.mockReset();
    startOAuthLogin.mockReset();
  });

  it("offers the cookie paste field for a cookie-only catalog", () => {
    renderDialog();
    expect(screen.getByLabelText("usage.fieldCookie")).toBeTruthy();
  });

  it("never pushes a cookie-only catalog down the OAuth flow", () => {
    renderDialog();
    // The regression this guards: `selectableAuthModes` used to strip cookie,
    // the `?? "o-auth"` fallback took over, and the dialog rendered an OAuth
    // login panel that called `start_oauth_login` on a Cookie-only provider.
    expect(screen.queryByText("usage.oauthLogin")).toBeNull();
    expect(startOAuthLogin).not.toHaveBeenCalled();
  });

  it("still surfaces the catalog's own paste instructions", () => {
    renderDialog();
    expect(screen.getByText(/Oasis-Token/)).toBeTruthy();
  });

  it("sends the pasted header as cookie_header with auth_mode cookie", async () => {
    createSubscription.mockResolvedValue({ id: "sub-1", display_name: "阶跃 Step" });
    renderDialog();

    fireEvent.change(screen.getByLabelText("usage.fieldCookie"), {
      target: { value: "  Oasis-Token=abc; other=1  " },
    });
    fireEvent.click(screen.getByText("common.add"));

    await waitFor(() => expect(createSubscription).toHaveBeenCalledTimes(1));
    const payload = createSubscription.mock.calls[0][0];
    expect(payload.catalog_id).toBe("stepfun");
    expect(payload.auth_mode).toBe("cookie");
    expect(payload.cookie_header).toBe("Oasis-Token=abc; other=1");
    // Cookie mode is not OAuth, so no region is invented for it.
    expect(payload.oauth_region).toBeUndefined();
  });

  it("omits cookie_header entirely when nothing was pasted", async () => {
    createSubscription.mockResolvedValue({ id: "sub-1", display_name: "阶跃 Step" });
    renderDialog();

    fireEvent.click(screen.getByText("common.add"));

    await waitFor(() => expect(createSubscription).toHaveBeenCalledTimes(1));
    // Absent, not `""` — the backend reads absent as "keep whatever is stored".
    expect(createSubscription.mock.calls[0][0].cookie_header).toBeUndefined();
  });
});
