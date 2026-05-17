import { act, renderHook } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { useViewMode } from "./useViewMode";

// jsdom's localStorage stub is incomplete — provide a full mock
const store = new Map<string, string>();
const mockLocalStorage = {
  getItem: vi.fn((key: string) => store.get(key) ?? null),
  setItem: vi.fn((key: string, value: string) => {
    store.set(key, value);
  }),
  removeItem: vi.fn((key: string) => {
    store.delete(key);
  }),
};
Object.defineProperty(globalThis, "localStorage", { value: mockLocalStorage, writable: true });

describe("useViewMode", () => {
  beforeEach(() => {
    store.clear();
    vi.clearAllMocks();
  });

  it("should default to 'grid' when no stored value exists", () => {
    const { result } = renderHook(() => useViewMode());
    expect(result.current[0]).toBe("grid");
  });

  it("should accept a custom default mode", () => {
    const { result } = renderHook(() => useViewMode("list"));
    expect(result.current[0]).toBe("list");
  });

  it("should read stored value from localStorage", () => {
    localStorage.setItem("skillstar:view-mode", "list");
    const { result } = renderHook(() => useViewMode());
    expect(result.current[0]).toBe("list");
  });

  it("should ignore invalid stored values and fall back to default", () => {
    localStorage.setItem("skillstar:view-mode", "invalid-mode");
    const { result } = renderHook(() => useViewMode());
    expect(result.current[0]).toBe("grid");
  });

  it("should update mode and persist to localStorage", () => {
    const { result } = renderHook(() => useViewMode());

    act(() => {
      result.current[1]("list");
    });

    expect(result.current[0]).toBe("list");
    expect(localStorage.getItem("skillstar:view-mode")).toBe("list");
  });

  it("should sync across hooks via custom event", () => {
    const { result: hook1 } = renderHook(() => useViewMode());
    const { result: hook2 } = renderHook(() => useViewMode());

    act(() => {
      hook1.current[1]("list");
    });

    // The second hook should receive the event and update
    expect(hook2.current[0]).toBe("list");
  });
});
