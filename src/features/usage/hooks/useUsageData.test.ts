import { act, renderHook, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { Subscription } from "../types";
import { applyActiveSubscriptionMap, mergeActiveSubscriptionUpdate, useUsageData } from "./useUsageData";

const api = vi.hoisted(() => ({
  listCatalog: vi.fn(),
  listSubscriptions: vi.fn(),
  getUsageSummary: vi.fn(),
  getSubscriptionAlerts: vi.fn(),
  deleteSubscription: vi.fn(),
  reorderSubscriptions: vi.fn(),
  dismissSubscriptionAlert: vi.fn(),
  refreshAllSubscriptions: vi.fn(),
  setActiveSubscription: vi.fn(),
  getActiveSubscriptions: vi.fn(),
  reconcileCliAccounts: vi.fn(),
}));
vi.mock("../api", () => ({ usageApi: api }));

const toastMock = vi.hoisted(() => ({
  error: vi.fn(),
  success: vi.fn(),
  info: vi.fn(),
  warning: vi.fn(),
  loading: vi.fn(),
}));
vi.mock("sonner", () => ({ toast: toastMock }));

function subscription(id: string, active: boolean, sortIndex = 0): Subscription {
  return { id, catalog_id: "xai", display_name: id, is_active: active, sort_index: sortIndex } as Subscription;
}

describe("mergeActiveSubscriptionUpdate", () => {
  it("keeps the previous active sibling when backend rolled the target back", () => {
    const bob = subscription("bob", true);
    const alice = subscription("alice", false);

    const merged = mergeActiveSubscriptionUpdate([bob, subscription("alice", false)], alice);

    expect(merged.find((row) => row.id === "bob")?.is_active).toBe(true);
    expect(merged.find((row) => row.id === "alice")?.is_active).toBe(false);
  });

  it("demotes the previous sibling only after backend confirms the target active", () => {
    const bob = subscription("bob", true);
    const alice = subscription("alice", true);

    const merged = mergeActiveSubscriptionUpdate([bob, subscription("alice", false)], alice);

    expect(merged.find((row) => row.id === "bob")?.is_active).toBe(false);
    expect(merged.find((row) => row.id === "alice")?.is_active).toBe(true);
  });
});

describe("applyActiveSubscriptionMap", () => {
  it("moves the badge to whichever row the backend map names", () => {
    const rows = [subscription("bob", true), subscription("alice", false)];

    const next = applyActiveSubscriptionMap(rows, { xai: "alice" });

    expect(next.find((row) => row.id === "bob")?.is_active).toBe(false);
    expect(next.find((row) => row.id === "alice")?.is_active).toBe(true);
  });

  it("clears the badge for a catalog the map no longer lists", () => {
    const next = applyActiveSubscriptionMap([subscription("bob", true)], {});
    expect(next[0].is_active).toBe(false);
  });

  it("returns the same array when nothing moved", () => {
    const rows = [subscription("bob", true), subscription("alice", false)];
    expect(applyActiveSubscriptionMap(rows, { xai: "bob" })).toBe(rows);
  });
});

describe("useUsageData mutations", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    api.listCatalog.mockResolvedValue([]);
    api.listSubscriptions.mockResolvedValue([]);
    api.getUsageSummary.mockResolvedValue(null);
    api.getSubscriptionAlerts.mockResolvedValue([]);
    api.reconcileCliAccounts.mockResolvedValue({});
  });

  async function mounted(rows: Subscription[]) {
    api.listSubscriptions.mockResolvedValue(rows);
    const hook = renderHook(() => useUsageData());
    await waitFor(() => expect(hook.result.current.loading).toBe(false));
    return hook;
  }

  it("keeps the row and reports the reason when deleting fails", async () => {
    api.deleteSubscription.mockRejectedValue(new Error("row is locked"));
    const { result } = await mounted([subscription("a", false)]);

    await act(async () => {
      await result.current.remove("a").catch(() => undefined);
    });

    expect(result.current.subscriptions.map((row) => row.id)).toEqual(["a"]);
    expect(toastMock.error).toHaveBeenCalledWith(expect.stringContaining("row is locked"));
  });

  it("rolls the order back and reports when persisting the reorder fails", async () => {
    api.reorderSubscriptions.mockRejectedValue(new Error("storage is read-only"));
    const { result } = await mounted([subscription("a", false, 0), subscription("b", false, 1)]);

    await act(async () => {
      await result.current.reorder(["b", "a"]).catch(() => undefined);
    });

    expect(result.current.subscriptions.map((row) => row.id)).toEqual(["a", "b"]);
    expect(result.current.subscriptions.map((row) => row.sort_index)).toEqual([0, 1]);
    expect(toastMock.error).toHaveBeenCalledWith(expect.stringContaining("storage is read-only"));
  });

  it("restores a dismissed alert and reports when the backend refuses", async () => {
    api.getSubscriptionAlerts.mockResolvedValue([{ id: "alert-1", subscription_id: "a" }]);
    api.dismissSubscriptionAlert.mockRejectedValue(new Error("alert already gone"));
    const { result } = await mounted([subscription("a", false)]);

    await act(async () => {
      await result.current.dismissAlert("alert-1").catch(() => undefined);
    });

    expect(result.current.alerts.map((alert) => alert.id)).toEqual(["alert-1"]);
    expect(toastMock.error).toHaveBeenCalledWith(expect.stringContaining("alert already gone"));
  });

  it("reads which account each CLI is actually serving on load", async () => {
    api.reconcileCliAccounts.mockResolvedValue({ xai: { kind: "linkedTo", subscriptionId: "b" } });

    const { result } = await mounted([subscription("a", true), subscription("b", false)]);

    // The pin says "a" and the file says "b"; the hook exposes both, and the
    // card decides from the file.
    expect(result.current.subscriptions.find((row) => row.id === "a")?.is_active).toBe(true);
    expect(result.current.cliAccounts).toEqual({ xai: { kind: "linkedTo", subscriptionId: "b" } });
  });

  it("re-reads the CLI after a refused switch, so the old badge survives", async () => {
    api.setActiveSubscription.mockResolvedValue({
      ...subscription("a", false),
      switch_result: { toolId: "grok", configPath: "/g/auth.json", success: false, error: "locked" },
    });
    api.reconcileCliAccounts.mockResolvedValue({ xai: { kind: "linkedTo", subscriptionId: "b" } });

    const { result } = await mounted([subscription("a", false), subscription("b", true)]);
    await act(async () => {
      await result.current.setActive("a");
    });

    expect(result.current.subscriptions.find((row) => row.id === "b")?.is_active).toBe(true);
    expect(result.current.cliAccounts).toEqual({ xai: { kind: "linkedTo", subscriptionId: "b" } });
  });

  it("does not let an in-flight refreshAll overwrite a newer setActive", async () => {
    const stale = [subscription("a", false), subscription("b", true)];
    let releaseRefresh: (rows: Subscription[]) => void = () => undefined;
    api.refreshAllSubscriptions.mockReturnValue(
      new Promise<Subscription[]>((resolve) => {
        releaseRefresh = resolve;
      }),
    );
    api.setActiveSubscription.mockResolvedValue(subscription("a", true));
    // The CLI read is inside the same queue, so the refresh's older answer can
    // never land after the switch's newer one.
    api.reconcileCliAccounts
      .mockResolvedValueOnce({})
      .mockResolvedValueOnce({ xai: { kind: "linkedTo", subscriptionId: "b" } })
      .mockResolvedValueOnce({ xai: { kind: "linkedTo", subscriptionId: "a" } });

    const { result } = await mounted(stale);

    let refreshDone!: Promise<void>;
    let activeDone!: Promise<unknown>;
    act(() => {
      refreshDone = result.current.refreshAll();
      // Queued behind the refresh that is already on the wire.
      activeDone = result.current.setActive("a");
    });

    await act(async () => {
      releaseRefresh(stale.map((row) => ({ ...row })));
      await refreshDone;
      await activeDone;
    });

    expect(result.current.subscriptions.find((row) => row.id === "a")?.is_active).toBe(true);
    expect(result.current.subscriptions.find((row) => row.id === "b")?.is_active).toBe(false);
    expect(result.current.cliAccounts).toEqual({ xai: { kind: "linkedTo", subscriptionId: "a" } });
  });

  it("re-projects the active badge from the backend map (active-changed path)", async () => {
    api.getActiveSubscriptions.mockResolvedValue({ xai: "b" });
    const { result } = await mounted([subscription("a", true), subscription("b", false)]);

    await act(async () => {
      await result.current.syncActiveAccounts();
    });

    expect(result.current.subscriptions.find((row) => row.id === "a")?.is_active).toBe(false);
    expect(result.current.subscriptions.find((row) => row.id === "b")?.is_active).toBe(true);
  });
});
