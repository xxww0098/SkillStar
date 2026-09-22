import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { CatalogEntry } from "../types";
import { SubscriptionEditDialog } from "./SubscriptionEditDialog";

const startOAuthLogin = vi.fn();
const awaitOAuthCompletion = vi.fn();
const submitOAuthCallback = vi.fn();
const cancelOAuthLogin = vi.fn();

vi.mock("../api", () => ({
  usageApi: {
    createSubscription: vi.fn(),
    updateSubscription: vi.fn(),
    refreshSubscriptionUsage: vi.fn(),
    startOAuthLogin: (...args: unknown[]) => startOAuthLogin(...args),
    awaitOAuthCompletion: (...args: unknown[]) => awaitOAuthCompletion(...args),
    submitOAuthCallback: (...args: unknown[]) => submitOAuthCallback(...args),
    cancelOAuthLogin: (...args: unknown[]) => cancelOAuthLogin(...args),
    importSubscriptionFromLocal: vi.fn(),
  },
}));

vi.mock("sonner", () => ({
  toast: { success: vi.fn(), error: vi.fn(), warning: vi.fn() },
}));

// See SubscriptionEditDialog.cookie.test.tsx — `t` must be referentially
// stable or the form-reset effect re-runs every render.
const translation = { t: (key: string) => key };
vi.mock("react-i18next", () => ({
  useTranslation: () => translation,
}));

/** OAuth-only catalog, mirroring `catalog.rs`'s Codex entry. */
const codex: CatalogEntry = {
  id: "codex",
  display_name: "Codex",
  description: "OpenAI Codex CLI",
  tier: "o-auth",
  auth_modes: ["o-auth"],
  brand_color: "10A37F",
  default_currency: "USD",
  subscription_url: "https://chat.openai.com/codex",
  warning: null,
  regions: [],
};

function renderDialog() {
  return render(
    <SubscriptionEditDialog
      open
      catalog={[codex]}
      editing={null}
      preselectCatalogId="codex"
      onClose={vi.fn()}
      onCreated={vi.fn()}
      onUpdated={vi.fn()}
      onDeleted={vi.fn()}
    />,
  );
}

/** A never-settling completion — the login waits for the browser callback. */
function pendingCompletion() {
  return new Promise(() => {});
}

describe("SubscriptionEditDialog — OAuth steps", () => {
  beforeEach(() => {
    startOAuthLogin.mockReset();
    awaitOAuthCompletion.mockReset();
    submitOAuthCallback.mockReset();
    cancelOAuthLogin.mockReset();
    awaitOAuthCompletion.mockImplementation(pendingCompletion);
  });

  it("shows the guided steps with the browser step disabled before the link exists", () => {
    renderDialog();
    expect(screen.getByText("usage.oauthPanelTitle")).toBeTruthy();
    expect(screen.getByText("usage.oauthLinkPlaceholder")).toBeTruthy();
    expect(screen.getByText("usage.oauthStartLogin")).toBeTruthy();
    expect(screen.getByText("usage.oauthSubmitCallback").closest("button")?.disabled).toBe(true);
  });

  it("reveals the auth link and waiting status after starting", async () => {
    startOAuthLogin.mockResolvedValue({
      pending_id: "p-1",
      auth_url: "https://auth.example/authorize?state=x",
      expires_in_secs: 300,
    });
    renderDialog();

    fireEvent.click(screen.getByText("usage.oauthStartLogin"));

    await waitFor(() => expect(screen.getByText("https://auth.example/authorize?state=x")).toBeTruthy());
    expect(screen.getByText("usage.oauthOpenLink").closest("button")?.disabled).toBe(false);
    expect(screen.getByText("usage.btnWaitingLogin")).toBeTruthy();
    expect(screen.getByText("usage.cancelOAuth")).toBeTruthy();
    expect(awaitOAuthCompletion).toHaveBeenCalledWith("p-1");
  });

  it("submits a pasted callback for the pending login", async () => {
    startOAuthLogin.mockResolvedValue({ pending_id: "p-2", auth_url: "https://auth.example/authorize" });
    renderDialog();
    fireEvent.click(screen.getByText("usage.oauthStartLogin"));
    await waitFor(() => screen.getByText("usage.cancelOAuth"));

    const input = screen.getByPlaceholderText("usage.oauthCallbackPlaceholder");
    fireEvent.change(input, { target: { value: "http://localhost:1455/auth/callback?code=abc" } });
    fireEvent.click(screen.getByText("usage.oauthSubmitCallback"));

    await waitFor(() =>
      expect(submitOAuthCallback).toHaveBeenCalledWith("p-2", "http://localhost:1455/auth/callback?code=abc"),
    );
  });

  it("cancel returns the panel to the generate step", async () => {
    startOAuthLogin.mockResolvedValue({ pending_id: "p-3", auth_url: "https://auth.example/authorize" });
    cancelOAuthLogin.mockResolvedValue(undefined);
    renderDialog();
    fireEvent.click(screen.getByText("usage.oauthStartLogin"));
    await waitFor(() => screen.getByText("usage.cancelOAuth"));

    fireEvent.click(screen.getByText("usage.cancelOAuth"));

    await waitFor(() => expect(screen.getByText("usage.oauthStartLogin")).toBeTruthy());
    expect(cancelOAuthLogin).toHaveBeenCalledWith("p-3");
    expect(screen.queryByText("https://auth.example/authorize")).toBeNull();
  });
});
