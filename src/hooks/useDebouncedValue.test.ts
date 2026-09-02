import { act, renderHook } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { useDebouncedValue } from "./useDebouncedValue";

describe("useDebouncedValue", () => {
  beforeEach(() => {
    vi.useFakeTimers();
  });
  afterEach(() => {
    vi.useRealTimers();
  });

  it("returns the first value immediately, then lags subsequent ones", () => {
    const { result, rerender } = renderHook(({ value }) => useDebouncedValue(value, 200), {
      initialProps: { value: "a" },
    });
    expect(result.current).toBe("a");

    rerender({ value: "ab" });
    expect(result.current).toBe("a");
    act(() => {
      vi.advanceTimersByTime(199);
    });
    expect(result.current).toBe("a");
    act(() => {
      vi.advanceTimersByTime(1);
    });
    expect(result.current).toBe("ab");
  });

  it("collapses a burst of updates into the last value", () => {
    const { result, rerender } = renderHook(({ value }) => useDebouncedValue(value, 200), {
      initialProps: { value: "" },
    });
    rerender({ value: "f" });
    rerender({ value: "fi" });
    rerender({ value: "fil" });
    act(() => {
      vi.advanceTimersByTime(200);
    });
    expect(result.current).toBe("fil");
  });
});
