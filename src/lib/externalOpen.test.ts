import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { invoke, isTauri } from "@tauri-apps/api/core";
import { __test__, handleExternalAnchorClick, openExternalUrl } from "./externalOpen";

describe("externalOpen", () => {
  beforeEach(() => {
    __test__.resetDuplicateGuard();
    vi.mocked(isTauri).mockReturnValue(true);
    vi.mocked(invoke).mockReset();
    vi.mocked(invoke).mockResolvedValue(undefined);
  });

  afterEach(() => {
    __test__.resetDuplicateGuard();
  });

  it("isHttpUrl accepts only http(s)", () => {
    expect(__test__.isHttpUrl("https://cursor.com/settings")).toBe(true);
    expect(__test__.isHttpUrl("http://example.com")).toBe(true);
    expect(__test__.isHttpUrl("file:///tmp/x")).toBe(false);
    expect(__test__.isHttpUrl("javascript:alert(1)")).toBe(false);
  });

  it("openExternalUrl invokes open_external_url with the trimmed URL", async () => {
    const ok = await openExternalUrl("  https://platform.deepseek.com/usage  ");
    expect(ok).toBe(true);
    expect(invoke).toHaveBeenCalledWith("open_external_url", {
      url: "https://platform.deepseek.com/usage",
    });
  });

  it("openExternalUrl returns false on invoke failure and allows retry", async () => {
    vi.mocked(invoke).mockRejectedValueOnce(new Error("boom"));
    const first = await openExternalUrl("https://x.ai");
    expect(first).toBe(false);

    vi.mocked(invoke).mockResolvedValueOnce(undefined);
    const second = await openExternalUrl("https://x.ai");
    expect(second).toBe(true);
    expect(invoke).toHaveBeenCalledTimes(2);
  });

  it("openExternalUrl suppresses only successful duplicate opens within the window", async () => {
    await openExternalUrl("https://x.ai");
    await openExternalUrl("https://x.ai");
    // Second call is suppressed after success — single invoke.
    expect(invoke).toHaveBeenCalledTimes(1);
  });

  it("handleExternalAnchorClick preventDefaults and opens http(s) links", async () => {
    const preventDefault = vi.fn();
    const stopPropagation = vi.fn();
    const intercepted = handleExternalAnchorClick(
      { defaultPrevented: false, button: 0, preventDefault, stopPropagation },
      "https://cursor.com/settings",
    );
    expect(intercepted).toBe(true);
    expect(preventDefault).toHaveBeenCalled();
    expect(stopPropagation).toHaveBeenCalled();
    // open is async fire-and-forget; flush microtasks
    await Promise.resolve();
    expect(invoke).toHaveBeenCalledWith("open_external_url", {
      url: "https://cursor.com/settings",
    });
  });

  it("handleExternalAnchorClick ignores already-prevented events", () => {
    const preventDefault = vi.fn();
    const intercepted = handleExternalAnchorClick(
      { defaultPrevented: true, preventDefault },
      "https://cursor.com/settings",
    );
    expect(intercepted).toBe(false);
    expect(preventDefault).not.toHaveBeenCalled();
    expect(invoke).not.toHaveBeenCalled();
  });
});
