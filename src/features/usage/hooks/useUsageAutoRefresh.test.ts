import { act, renderHook } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { USAGE_REFRESH_INTERVALS, useUsageAutoRefreshRunner, useUsageAutoRefreshSettings } from "./useUsageAutoRefresh";

const STORAGE_KEY = "skillstar:usage-auto-refresh";

const storage = new Map<string, string>();
Object.defineProperty(globalThis, "localStorage", {
  writable: true,
  value: {
    getItem: vi.fn((key: string) => storage.get(key) ?? null),
    setItem: vi.fn((key: string, value: string) => {
      storage.set(key, value);
    }),
    removeItem: vi.fn((key: string) => {
      storage.delete(key);
    }),
    clear: vi.fn(() => storage.clear()),
    key: vi.fn((index: number) => Array.from(storage.keys())[index] ?? null),
    get length() {
      return storage.size;
    },
  },
});

describe("useUsageAutoRefreshRunner", () => {
  beforeEach(() => {
    vi.useFakeTimers();
    storage.clear();
  });

  afterEach(() => {
    vi.useRealTimers();
    storage.clear();
  });

  it("runs immediately and on interval when enabled", async () => {
    const onRefresh = vi.fn().mockResolvedValue(undefined);
    const settings = { enabled: true, intervalMs: USAGE_REFRESH_INTERVALS[0].ms };

    renderHook(() => useUsageAutoRefreshRunner(settings, onRefresh, false));

    expect(onRefresh).toHaveBeenCalledTimes(1);

    await act(async () => {
      vi.advanceTimersByTime(USAGE_REFRESH_INTERVALS[0].ms);
    });

    expect(onRefresh).toHaveBeenCalledTimes(2);
  });

  it("does not schedule refresh when disabled", async () => {
    const onRefresh = vi.fn().mockResolvedValue(undefined);
    const settings = { enabled: false, intervalMs: USAGE_REFRESH_INTERVALS[0].ms };

    renderHook(() => useUsageAutoRefreshRunner(settings, onRefresh, false));

    await act(async () => {
      vi.advanceTimersByTime(60_000);
    });

    expect(onRefresh).not.toHaveBeenCalled();
  });

  it("skips ticks while a refresh is already running", async () => {
    let resolveRefresh: (() => void) | undefined;
    const onRefresh = vi.fn(
      () =>
        new Promise<void>((resolve) => {
          resolveRefresh = resolve;
        }),
    );
    const settings = { enabled: true, intervalMs: 1_000 };

    const { rerender } = renderHook(({ refreshing }) => useUsageAutoRefreshRunner(settings, onRefresh, refreshing), {
      initialProps: { refreshing: false },
    });

    expect(onRefresh).toHaveBeenCalledTimes(1);

    rerender({ refreshing: true });

    await act(async () => {
      vi.advanceTimersByTime(1_000);
    });

    expect(onRefresh).toHaveBeenCalledTimes(1);

    await act(async () => {
      resolveRefresh?.();
      rerender({ refreshing: false });
    });

    await act(async () => {
      vi.advanceTimersByTime(1_000);
    });

    expect(onRefresh).toHaveBeenCalledTimes(2);
  });
});

describe("useUsageAutoRefreshSettings", () => {
  beforeEach(() => {
    storage.clear();
  });

  afterEach(() => {
    storage.clear();
  });

  it("persists enabled state to localStorage", () => {
    const { result } = renderHook(() => useUsageAutoRefreshSettings());

    act(() => {
      result.current.setAutoRefreshEnabled(true);
    });

    expect(result.current.autoRefreshEnabled).toBe(true);
    expect(JSON.parse(storage.get(STORAGE_KEY) ?? "{}")).toEqual({
      enabled: true,
      intervalMs: USAGE_REFRESH_INTERVALS[1].ms,
    });
  });
});
