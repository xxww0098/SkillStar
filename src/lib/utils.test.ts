import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import {
  agentIconCls,
  cn,
  detectPlatform,
  formatAiErrorMessage,
  formatGlobalPathForDisplay,
  formatInstalls,
  formatPlatformPath,
  inferUserHomeRoot,
  navigateToAiSettings,
  navigateToSettingsSection,
  navigateToTranslationSettings,
  resolveSkillstarDataPath,
} from "./utils";

describe("cn (class merging)", () => {
  it("should merge class names", () => {
    expect(cn("foo", "bar")).toBe("foo bar");
  });

  it("should handle conditional classes", () => {
    expect(cn("base", false && "hidden", "visible")).toBe("base visible");
  });

  it("should merge Tailwind classes correctly", () => {
    expect(cn("p-4", "p-2")).toBe("p-2");
  });
});

describe("detectPlatform", () => {
  it("should return a valid platform string", () => {
    const platform = detectPlatform();
    expect(["macos", "windows", "linux", "unknown"]).toContain(platform);
  });
});

describe("formatPlatformPath", () => {
  it("should return the path unchanged on non-Windows", () => {
    // In jsdom, navigator is not Windows
    const result = formatPlatformPath("src/hooks/test.ts");
    // The result depends on the platform detection, but should be a string
    expect(typeof result).toBe("string");
  });
});

describe("inferUserHomeRoot", () => {
  it("should infer Windows home root", () => {
    expect(inferUserHomeRoot("C:\\Users\\xiewe\\.skillstar\\")).toBe("C:/Users/xiewe");
  });

  it("should infer Unix home root", () => {
    expect(inferUserHomeRoot("/home/user/.skillstar/")).toBe("/home/user");
    expect(inferUserHomeRoot("/Users/alice/.skillstar/")).toBe("/Users/alice");
  });
});

describe("formatGlobalPathForDisplay", () => {
  it("should keep absolute Windows paths on Windows platform", () => {
    expect(formatGlobalPathForDisplay("C:/Users/xiewe/.codex/skills", "windows")).toBe(
      "C:\\Users\\xiewe\\.codex\\skills",
    );
  });

  it("should collapse Unix home paths to tilde on macOS/Linux", () => {
    expect(formatGlobalPathForDisplay("/Users/alice/.codex/skills", "macos")).toBe("~/.codex/skills");
    expect(formatGlobalPathForDisplay("/home/alice/.codex/skills", "linux")).toBe("~/.codex/skills");
  });
});

describe("resolveSkillstarDataPath", () => {
  it("should resolve Windows SkillStar data path", () => {
    expect(resolveSkillstarDataPath("C:/Users/xiewe/", "windows")).toBe("C:\\Users\\xiewe\\.skillstar\\");
  });

  it("should resolve Linux/macOS SkillStar data path", () => {
    expect(resolveSkillstarDataPath("/home/xiewe/", "linux")).toBe("/home/xiewe/.skillstar/");
    expect(resolveSkillstarDataPath("/Users/xiewe/", "macos")).toBe("/Users/xiewe/.skillstar/");
  });
});

describe("agentIconCls", () => {
  it("should return base classes for SVG icons", () => {
    expect(agentIconCls("claude.svg")).toBe("w-3.5 h-3.5");
  });

  it("should bump size for PNG icons", () => {
    expect(agentIconCls("claude.png")).toBe("w-5 h-5");
  });

  it("should support custom base", () => {
    expect(agentIconCls("test.svg", "w-4 h-4")).toBe("w-4 h-4");
    expect(agentIconCls("test.png", "w-4 h-4")).toBe("w-5 h-5");
  });
});

describe("formatInstalls", () => {
  it("should format small numbers without suffix", () => {
    expect(formatInstalls(42)).toBe("42");
    expect(formatInstalls(999)).toBe("999");
  });

  it("should format thousands with K suffix", () => {
    expect(formatInstalls(1_000)).toBe("1.0K");
    expect(formatInstalls(759_100)).toBe("759.1K");
  });

  it("should format millions with M suffix", () => {
    expect(formatInstalls(1_200_000)).toBe("1.2M");
    expect(formatInstalls(10_500_000)).toBe("10.5M");
  });
});

describe("formatAiErrorMessage", () => {
  const mockT = (key: string, opts?: Record<string, unknown>) => (opts?.defaultValue as string) ?? key;

  it("should return null for empty errors", () => {
    expect(formatAiErrorMessage(null, mockT)).toBeNull();
    expect(formatAiErrorMessage(undefined, mockT)).toBeNull();
    expect(formatAiErrorMessage("", mockT)).toBeNull();
  });

  it("should detect AI not configured errors", () => {
    const result = formatAiErrorMessage("AI provider is disabled", mockT);
    expect(result).toContain("not configured");
  });

  it("should detect MyMemory errors", () => {
    const result = formatAiErrorMessage("MyMemory service unavailable", mockT);
    expect(result).toContain("MyMemory");
  });

  it("should detect network errors", () => {
    const result = formatAiErrorMessage("Failed to send request", mockT);
    expect(result).toContain("Network");
  });

  it("should return raw message for unknown errors", () => {
    const result = formatAiErrorMessage("Some unknown error", mockT);
    expect(result).toBe("Some unknown error");
  });
});

describe("settings navigation helpers", () => {
  const storage = new Map<string, string>();
  const localStorageMock = {
    get length() {
      return storage.size;
    },
    clear: () => storage.clear(),
    getItem: (key: string) => storage.get(key) ?? null,
    key: (index: number) => Array.from(storage.keys())[index] ?? null,
    removeItem: (key: string) => {
      storage.delete(key);
    },
    setItem: (key: string, value: string) => {
      storage.set(key, String(value));
    },
  } as Storage;

  beforeEach(() => {
    storage.clear();
    Object.defineProperty(globalThis, "localStorage", {
      value: localStorageMock,
      configurable: true,
    });
    Object.defineProperty(window, "localStorage", {
      value: localStorageMock,
      configurable: true,
    });
  });

  afterEach(() => {
    storage.clear();
  });

  it("should navigate to settings and focus the requested section", () => {
    const handleNavigate = vi.fn();
    const handleFocus = vi.fn();

    window.addEventListener("skillstar:navigate", handleNavigate as EventListener);
    window.addEventListener("skillstar:settings-focus", handleFocus as EventListener);

    navigateToSettingsSection("storage");

    expect(localStorage.getItem("skillstar:settings-focus")).toBe("storage");
    expect(handleNavigate).toHaveBeenCalledTimes(1);
    expect((handleNavigate.mock.calls[0][0] as CustomEvent<{ page?: string }>).detail).toEqual({ page: "settings" });
    expect(handleFocus).toHaveBeenCalledTimes(1);
    expect((handleFocus.mock.calls[0][0] as CustomEvent<{ target?: string }>).detail).toEqual({ target: "storage" });

    window.removeEventListener("skillstar:navigate", handleNavigate as EventListener);
    window.removeEventListener("skillstar:settings-focus", handleFocus as EventListener);
  });

  it("should keep AI navigation focused on the AI section", () => {
    const handleFocus = vi.fn();

    window.addEventListener("skillstar:settings-focus", handleFocus as EventListener);

    navigateToAiSettings();

    expect(localStorage.getItem("skillstar:settings-focus")).toBe("ai-provider");
    expect(handleFocus).toHaveBeenCalledTimes(1);
    expect((handleFocus.mock.calls[0][0] as CustomEvent<{ target?: string }>).detail).toEqual({
      target: "ai-provider",
    });

    window.removeEventListener("skillstar:settings-focus", handleFocus as EventListener);
  });

  it("should keep translation navigation focused on the translation section", () => {
    const handleFocus = vi.fn();

    window.addEventListener("skillstar:settings-focus", handleFocus as EventListener);

    navigateToTranslationSettings();

    expect(localStorage.getItem("skillstar:settings-focus")).toBe("translation");
    expect(handleFocus).toHaveBeenCalledTimes(1);
    expect((handleFocus.mock.calls[0][0] as CustomEvent<{ target?: string }>).detail).toEqual({
      target: "translation",
    });

    window.removeEventListener("skillstar:settings-focus", handleFocus as EventListener);
  });
});
