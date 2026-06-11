import { act, renderHook } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { SaveAttemptResult } from "../../types";
import { useAutosave } from "../useAutosave";

describe("useAutosave", () => {
  beforeEach(() => {
    vi.useFakeTimers();
  });
  afterEach(() => {
    vi.useRealTimers();
  });

  function setup(initialDirty = true, result: SaveAttemptResult = "saved") {
    const save = vi.fn(async () => result);
    const hook = renderHook(({ dirty, saveFn }) => useAutosave({ dirty, save: saveFn }), {
      initialProps: { dirty: initialDirty, saveFn: save },
    });
    return { save, hook };
  }

  it("debounces 600ms before saving and lands on 'saved'", async () => {
    const { save, hook } = setup();
    expect(hook.result.current.state).toBe("dirty");

    await act(async () => {
      vi.advanceTimersByTime(599);
    });
    expect(save).not.toHaveBeenCalled();

    await act(async () => {
      vi.advanceTimersByTime(1);
      await vi.runAllTimersAsync();
    });
    expect(save).toHaveBeenCalledTimes(1);
    expect(hook.result.current.state).toBe("saved");
  });

  it("does not save while clean", async () => {
    const { save, hook } = setup(false);
    await act(async () => {
      vi.advanceTimersByTime(2000);
    });
    expect(save).not.toHaveBeenCalled();
    expect(hook.result.current.state).toBe("idle");
  });

  it("a failed attempt lands on 'error' and does NOT re-arm until save identity changes", async () => {
    const save = vi.fn(async (): Promise<SaveAttemptResult> => "validation");
    const hook = renderHook(({ saveFn }) => useAutosave({ dirty: true, save: saveFn }), {
      initialProps: { saveFn: save },
    });

    await act(async () => {
      await vi.advanceTimersByTimeAsync(700);
    });
    expect(save).toHaveBeenCalledTimes(1);
    expect(hook.result.current.state).toBe("error");

    // Still dirty, but no identity change → no retry loop (no repeated toasts).
    await act(async () => {
      await vi.advanceTimersByTimeAsync(3000);
    });
    expect(save).toHaveBeenCalledTimes(1);

    // User edits again → new save identity → re-arms.
    const save2 = vi.fn(async (): Promise<SaveAttemptResult> => "saved");
    hook.rerender({ saveFn: save2 });
    await act(async () => {
      await vi.advanceTimersByTimeAsync(700);
    });
    expect(save2).toHaveBeenCalledTimes(1);
    expect(hook.result.current.state).toBe("saved");
  });

  it("flush() saves immediately when dirty and is a no-op when clean", async () => {
    const { save, hook } = setup();
    await act(async () => {
      await hook.result.current.flush();
    });
    expect(save).toHaveBeenCalledTimes(1);

    const clean = setup(false);
    await act(async () => {
      await clean.hook.result.current.flush();
    });
    expect(clean.save).not.toHaveBeenCalled();
  });
});
