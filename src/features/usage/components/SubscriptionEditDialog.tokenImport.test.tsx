import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { CatalogEntry } from "../types";
import { SubscriptionEditDialog } from "./SubscriptionEditDialog";

const createSubscription = vi.fn();
const importSubscriptionToken = vi.fn();

vi.mock("../api", () => ({
  usageApi: {
    createSubscription: (...args: unknown[]) => createSubscription(...args),
    updateSubscription: vi.fn(),
    refreshSubscriptionUsage: vi.fn(),
    startOAuthLogin: vi.fn(),
    awaitOAuthCompletion: vi.fn(),
    submitOAuthCallback: vi.fn(),
    cancelOAuthLogin: vi.fn(),
    importSubscriptionFromLocal: vi.fn(),
    importSubscriptionToken: (...args: unknown[]) => importSubscriptionToken(...args),
  },
}));

vi.mock("sonner", () => ({
  toast: { success: vi.fn(), error: vi.fn(), warning: vi.fn() },
}));

const translation = { t: (key: string) => key };
vi.mock("react-i18next", () => ({
  useTranslation: () => translation,
}));

const pasted: CatalogEntry = {
  id: "example-paste",
  display_name: "Example Paste",
  description: "Pasted credentials",
  tier: "o-auth",
  auth_modes: ["token-import"],
  brand_color: "112233",
  default_currency: "USD",
  subscription_url: "https://example.test/account",
  warning: "Paste the session token from Example.",
  regions: [],
};

function renderDialog() {
  return render(
    <SubscriptionEditDialog
      open
      catalog={[pasted]}
      editing={null}
      preselectCatalogId="example-paste"
      onClose={vi.fn()}
      onCreated={vi.fn()}
      onUpdated={vi.fn()}
      onDeleted={vi.fn()}
    />,
  );
}

describe("SubscriptionEditDialog — token import", () => {
  beforeEach(() => {
    createSubscription.mockReset();
    importSubscriptionToken.mockReset();
  });

  it("submits the paste as the token payload and not as an api key", async () => {
    importSubscriptionToken.mockResolvedValue({ id: "sub-1", display_name: "Example Paste" });
    renderDialog();

    expect(screen.getByText("usage.tokenImportHint")).toBeTruthy();
    fireEvent.change(screen.getByLabelText("usage.fieldTokenImport"), {
      target: { value: "  raw-paste-secret  " },
    });
    fireEvent.click(screen.getByText("common.add"));

    await waitFor(() => expect(importSubscriptionToken).toHaveBeenCalledTimes(1));
    expect(importSubscriptionToken).toHaveBeenCalledWith("example-paste", "raw-paste-secret", undefined);
    expect(createSubscription).not.toHaveBeenCalled();
  });
});
