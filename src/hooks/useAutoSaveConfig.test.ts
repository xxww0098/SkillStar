import { act, renderHook } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { useAutoSaveConfig } from "./useAutoSaveConfig";

interface DemoConfig {
  enabled: boolean;
  value: string;
}

const FALLBACK: DemoConfig = { enabled: false, value: "" };
const LOADED: DemoConfig = { enabled: true, value: "loaded" };

function isSameDemoConfig(a: DemoConfig, b: DemoConfig): boolean {
  return a.enabled === b.enabled && a.value === b.value;
}

/** Flush the microtask queue under fake timers so a pending load()/save() promise settles. */
async function flush() {
  await act(async () => {
    await vi.advanceTimersByTimeAsync(0);
  });
}

describe("useAutoSaveConfig", () => {
  beforeEach(() => {
    vi.useFakeTimers();
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  function setup(overrides?: {
    load?: () => Promise<DemoConfig>;
    save?: (config: DemoConfig) => Promise<void>;
    debounceMs?: number;
    savedIndicatorMs?: number;
    onSaveError?: (error: unknown) => void;
    skip?: boolean;
  }) {
    const load = overrides?.load ?? vi.fn(async () => LOADED);
    const save = overrides?.save ?? vi.fn(async () => {});
    const onSaveError = overrides?.onSaveError ?? vi.fn();
    const hook = renderHook(() =>
      useAutoSaveConfig<DemoConfig>({
        load,
        fallback: FALLBACK,
        save,
        isEqual: isSameDemoConfig,
        debounceMs: overrides?.debounceMs,
        savedIndicatorMs: overrides?.savedIndicatorMs,
        onSaveError,
        skip: overrides?.skip,
      }),
    );
    return { load, save, onSaveError, hook };
  }

  /** setup() + flush the initial load() so the hook starts in the `loaded` state. */
  async function setupLoaded(overrides?: Parameters<typeof setup>[0]) {
    const result = setup(overrides);
    await flush();
    expect(result.hook.result.current.loaded).toBe(true);
    return result;
  }

  it("starts unloaded with the fallback config, then loads asynchronously", async () => {
    const { hook } = setup();
    expect(hook.result.current.loaded).toBe(false);
    expect(hook.result.current.config).toEqual(FALLBACK);

    await flush();

    expect(hook.result.current.loaded).toBe(true);
    expect(hook.result.current.config).toEqual(LOADED);
  });

  it("falls back to the fallback config when load() rejects", async () => {
    const load = vi.fn(async () => {
      throw new Error("load failed");
    });
    const { hook } = await setupLoaded({ load });

    expect(hook.result.current.config).toEqual(FALLBACK);
  });

  it("debounces before saving after a change, once loaded", async () => {
    const save = vi.fn(async () => {});
    const { hook } = await setupLoaded({ save });

    act(() => {
      hook.result.current.setConfig({ enabled: true, value: "edited" });
    });

    // Not saved yet before the debounce window elapses.
    await act(async () => {
      await vi.advanceTimersByTimeAsync(599);
    });
    expect(save).not.toHaveBeenCalled();
    expect(hook.result.current.saving).toBe(false);

    await act(async () => {
      await vi.advanceTimersByTimeAsync(1);
    });
    expect(save).toHaveBeenCalledTimes(1);
    expect(save).toHaveBeenCalledWith({ enabled: true, value: "edited" });

    expect(hook.result.current.saving).toBe(false);
    expect(hook.result.current.showSaved).toBe(true);
  });

  it("resets the debounce timer on every consecutive edit, saving only once", async () => {
    const save = vi.fn(async () => {});
    const { hook } = await setupLoaded({ save });

    act(() => {
      hook.result.current.setConfig({ enabled: true, value: "a" });
    });
    await act(async () => {
      await vi.advanceTimersByTimeAsync(400);
    });

    act(() => {
      hook.result.current.setConfig({ enabled: true, value: "ab" });
    });
    await act(async () => {
      await vi.advanceTimersByTimeAsync(400);
    });

    act(() => {
      hook.result.current.setConfig({ enabled: true, value: "abc" });
    });
    await act(async () => {
      await vi.advanceTimersByTimeAsync(600);
    });

    expect(save).toHaveBeenCalledTimes(1);
    expect(save).toHaveBeenCalledWith({ enabled: true, value: "abc" });
  });

  it("respects a custom debounce duration", async () => {
    const save = vi.fn(async () => {});
    const { hook } = await setupLoaded({ save, debounceMs: 1500 });

    act(() => {
      hook.result.current.setConfig({ enabled: true, value: "edited" });
    });

    await act(async () => {
      await vi.advanceTimersByTimeAsync(1499);
    });
    expect(save).not.toHaveBeenCalled();

    await act(async () => {
      await vi.advanceTimersByTimeAsync(1);
    });
    expect(save).toHaveBeenCalledTimes(1);
  });

  it("does not schedule a save when the edited config is equal to the saved config", async () => {
    const save = vi.fn(async () => {});
    const { hook } = await setupLoaded({ save });

    act(() => {
      // Same value as LOADED -> isEqual holds -> no save should be scheduled.
      hook.result.current.setConfig({ ...LOADED });
    });

    await act(async () => {
      await vi.advanceTimersByTimeAsync(5000);
    });
    expect(save).not.toHaveBeenCalled();
  });

  it("reverts to the previous saved config and reports the error when save() rejects", async () => {
    const error = new Error("save failed");
    const save = vi.fn(async () => {
      throw error;
    });
    const onSaveError = vi.fn();
    const { hook } = await setupLoaded({ save, onSaveError });

    act(() => {
      hook.result.current.setConfig({ enabled: true, value: "edited" });
    });

    await act(async () => {
      await vi.advanceTimersByTimeAsync(600);
    });

    expect(save).toHaveBeenCalledTimes(1);
    expect(onSaveError).toHaveBeenCalledWith(error);
    expect(hook.result.current.config).toEqual(LOADED);
    expect(hook.result.current.saving).toBe(false);
    expect(hook.result.current.showSaved).toBe(false);
  });

  it("clears the showSaved indicator 2 seconds after a successful save", async () => {
    const save = vi.fn(async () => {});
    const { hook } = await setupLoaded({ save });

    act(() => {
      hook.result.current.setConfig({ enabled: true, value: "edited" });
    });

    await act(async () => {
      await vi.advanceTimersByTimeAsync(600);
    });
    expect(hook.result.current.showSaved).toBe(true);

    await act(async () => {
      await vi.advanceTimersByTimeAsync(1999);
    });
    expect(hook.result.current.showSaved).toBe(true);

    await act(async () => {
      await vi.advanceTimersByTimeAsync(1);
    });
    expect(hook.result.current.showSaved).toBe(false);
  });

  it("respects a custom savedIndicatorMs duration", async () => {
    const save = vi.fn(async () => {});
    const { hook } = await setupLoaded({ save, savedIndicatorMs: 500 });

    act(() => {
      hook.result.current.setConfig({ enabled: true, value: "edited" });
    });

    await act(async () => {
      await vi.advanceTimersByTimeAsync(600);
    });
    expect(hook.result.current.showSaved).toBe(true);

    await act(async () => {
      await vi.advanceTimersByTimeAsync(499);
    });
    expect(hook.result.current.showSaved).toBe(true);

    await act(async () => {
      await vi.advanceTimersByTimeAsync(1);
    });
    expect(hook.result.current.showSaved).toBe(false);
  });

  it("does not save while a save is already in flight (saving guard)", async () => {
    let resolveSave: (() => void) | undefined;
    const save = vi.fn(
      () =>
        new Promise<void>((resolve) => {
          resolveSave = resolve;
        }),
    );
    const { hook } = await setupLoaded({ save });

    act(() => {
      hook.result.current.setConfig({ enabled: true, value: "first" });
    });
    await act(async () => {
      await vi.advanceTimersByTimeAsync(600);
    });
    expect(save).toHaveBeenCalledTimes(1);
    expect(hook.result.current.saving).toBe(true);

    // Edit again while the first save is still in flight; the debounce effect
    // should not fire a second concurrent save attempt.
    act(() => {
      hook.result.current.setConfig({ enabled: true, value: "second" });
    });
    await act(async () => {
      await vi.advanceTimersByTimeAsync(600);
    });
    expect(save).toHaveBeenCalledTimes(1);

    await act(async () => {
      resolveSave?.();
      await vi.advanceTimersByTimeAsync(0);
    });

    // Now that the first save finished, the pending edit gets its own debounce window.
    await act(async () => {
      await vi.advanceTimersByTimeAsync(600);
    });
    expect(save).toHaveBeenCalledTimes(2);
    expect(save).toHaveBeenLastCalledWith({ enabled: true, value: "second" });
  });

  it("skip suppresses auto-save scheduling (e.g. while a separate test-connection flow is running)", async () => {
    const save = vi.fn(async () => {});
    let skip = true;
    const utils = renderHook(() =>
      useAutoSaveConfig<DemoConfig>({
        load: async () => LOADED,
        fallback: FALLBACK,
        save,
        isEqual: isSameDemoConfig,
        skip,
      }),
    );
    await flush();
    expect(utils.result.current.loaded).toBe(true);

    act(() => {
      utils.result.current.setConfig({ enabled: true, value: "edited" });
    });
    await act(async () => {
      await vi.advanceTimersByTimeAsync(5000);
    });
    expect(save).not.toHaveBeenCalled();

    skip = false;
    utils.rerender();
    await act(async () => {
      await vi.advanceTimersByTimeAsync(600);
    });
    expect(save).toHaveBeenCalledTimes(1);
  });

  it("hydrate() adopts an externally-loaded config as both config and saved baseline", async () => {
    const save = vi.fn(async () => {});
    const { hook } = await setupLoaded({ save });

    const external: DemoConfig = { enabled: true, value: "from-external-source" };
    act(() => {
      hook.result.current.hydrate(external);
    });

    expect(hook.result.current.config).toEqual(external);
    expect(hook.result.current.loaded).toBe(true);

    // Since hydrate() also re-baselines savedConfig, no save should fire.
    await act(async () => {
      await vi.advanceTimersByTimeAsync(5000);
    });
    expect(save).not.toHaveBeenCalled();
  });

  it("markSaved() re-baselines savedConfig without touching the current config or loaded flag", async () => {
    const save = vi.fn(async () => {});
    const { hook } = await setupLoaded({ save });

    const external: DemoConfig = { enabled: true, value: "saved-out-of-band" };
    act(() => {
      hook.result.current.setConfig(external);
      hook.result.current.markSaved(external);
    });

    // markSaved re-baselines savedConfig to `external`, matching the current
    // config -> isEqual holds -> no auto-save should be scheduled.
    await act(async () => {
      await vi.advanceTimersByTimeAsync(5000);
    });
    expect(save).not.toHaveBeenCalled();
    expect(hook.result.current.config).toEqual(external);
  });

  it("revert() resets config back to the last saved baseline", async () => {
    const { hook } = await setupLoaded();

    act(() => {
      hook.result.current.setConfig({ enabled: true, value: "unsaved-edit" });
    });
    expect(hook.result.current.config).toEqual({ enabled: true, value: "unsaved-edit" });

    act(() => {
      hook.result.current.revert();
    });
    expect(hook.result.current.config).toEqual(LOADED);
  });
});
