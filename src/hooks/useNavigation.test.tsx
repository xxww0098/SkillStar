import { act, renderHook } from "@testing-library/react";
import fc from "fast-check";
import type { ReactNode } from "react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { NavPage } from "../types";
import { NavigationProvider, useAppMode, useNavigation } from "./useNavigation";

// jsdom's localStorage stub is incomplete in this test runner — provide
// a full mock so persistence-related tests can `.clear()` cleanly.
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

function wrapper({ children }: { children: ReactNode }) {
  return <NavigationProvider>{children}</NavigationProvider>;
}

describe("useNavigation - AppMode support", () => {
  beforeEach(() => {
    window.location.hash = "";
    localStorage.clear();
  });

  it("defaults to skills mode", () => {
    const { result } = renderHook(() => useNavigation(), { wrapper });
    expect(result.current.appMode).toBe("skills");
    expect(result.current.modelsActivePage).toBe("hub");
  });

  it("switches to models mode and updates hash to single hub URL", () => {
    const { result } = renderHook(() => useNavigation(), { wrapper });

    act(() => {
      result.current.setAppMode("models");
    });

    expect(result.current.appMode).toBe("models");
    expect(window.location.hash).toBe("#models");
  });

  it("switches back to skills mode and restores skills hash", () => {
    const { result } = renderHook(() => useNavigation(), { wrapper });

    act(() => {
      result.current.navigate("projects");
    });
    expect(window.location.hash).toBe("#projects");

    act(() => {
      result.current.setAppMode("models");
    });
    expect(window.location.hash).toBe("#models");

    act(() => {
      result.current.setAppMode("skills");
    });
    expect(result.current.appMode).toBe("skills");
    expect(window.location.hash).toBe("#projects");
  });

  it("navigateModels always lands on the hub (legacy API compat)", () => {
    const { result } = renderHook(() => useNavigation(), { wrapper });

    act(() => {
      result.current.navigateModels("hub");
    });

    expect(result.current.appMode).toBe("models");
    expect(result.current.modelsActivePage).toBe("hub");
    expect(window.location.hash).toBe("#models");
  });

  it("initializes from models hash in URL", () => {
    window.location.hash = "#models";

    const { result } = renderHook(() => useNavigation(), { wrapper });

    expect(result.current.appMode).toBe("models");
    expect(result.current.modelsActivePage).toBe("hub");
  });

  it("normalizes legacy `#models/<sub>` hash to the hub", () => {
    window.location.hash = "#models/providers";

    const { result } = renderHook(() => useNavigation(), { wrapper });

    expect(result.current.appMode).toBe("models");
    expect(result.current.modelsActivePage).toBe("hub");
  });

  it("handles hashchange into models mode", () => {
    const { result } = renderHook(() => useNavigation(), { wrapper });

    act(() => {
      window.location.hash = "#models";
      window.dispatchEvent(new HashChangeEvent("hashchange"));
    });

    expect(result.current.appMode).toBe("models");
    expect(result.current.modelsActivePage).toBe("hub");
  });

  it("handles hashchange from models to skills hash", () => {
    window.location.hash = "#models";
    const { result } = renderHook(() => useNavigation(), { wrapper });

    act(() => {
      window.location.hash = "#marketplace";
      window.dispatchEvent(new HashChangeEvent("hashchange"));
    });

    expect(result.current.appMode).toBe("skills");
    expect(result.current.activePage).toBe("marketplace");
  });

  it("navigate() sets mode back to skills", () => {
    const { result } = renderHook(() => useNavigation(), { wrapper });

    act(() => {
      result.current.setAppMode("models");
    });
    expect(result.current.appMode).toBe("models");

    act(() => {
      result.current.navigate("settings");
    });
    expect(result.current.appMode).toBe("skills");
    expect(result.current.activePage).toBe("settings");
  });
});

describe("useAppMode convenience hook", () => {
  beforeEach(() => {
    window.location.hash = "";
    localStorage.clear();
  });

  it("returns mode and derived booleans", () => {
    const { result } = renderHook(() => useAppMode(), { wrapper });

    expect(result.current.mode).toBe("skills");
    expect(result.current.isSkillsMode).toBe(true);
    expect(result.current.isModelsMode).toBe(false);
  });

  it("setMode switches to models", () => {
    const { result } = renderHook(() => useAppMode(), { wrapper });

    act(() => {
      result.current.setMode("models");
    });

    expect(result.current.mode).toBe("models");
    expect(result.current.isSkillsMode).toBe(false);
    expect(result.current.isModelsMode).toBe(true);
  });
});

describe("useNavigation - last edited provider persistence", () => {
  const STORAGE_KEY = "skillstar.lastEditedProviderId";

  beforeEach(() => {
    window.location.hash = "";
    localStorage.clear();
  });

  it("starts with no selected provider when storage is empty", () => {
    const { result } = renderHook(() => useNavigation(), { wrapper });
    expect(result.current.selectedProviderId).toBeNull();
  });

  it("rehydrates the last edited provider from localStorage on mount", () => {
    localStorage.setItem(STORAGE_KEY, "provider-xyz");

    const { result } = renderHook(() => useNavigation(), { wrapper });
    expect(result.current.selectedProviderId).toBe("provider-xyz");
  });

  it("persists the selected provider id to localStorage on change", () => {
    const { result } = renderHook(() => useNavigation(), { wrapper });

    act(() => {
      result.current.setSelectedProviderId("provider-abc");
    });

    expect(result.current.selectedProviderId).toBe("provider-abc");
    expect(localStorage.getItem(STORAGE_KEY)).toBe("provider-abc");
  });

  it("removes the persisted id when cleared with null", () => {
    localStorage.setItem(STORAGE_KEY, "provider-old");
    const { result } = renderHook(() => useNavigation(), { wrapper });
    expect(result.current.selectedProviderId).toBe("provider-old");

    act(() => {
      result.current.setSelectedProviderId(null);
    });

    expect(result.current.selectedProviderId).toBeNull();
    expect(localStorage.getItem(STORAGE_KEY)).toBeNull();
  });

  it("clicking Models with a persisted provider reopens it (mode switch keeps selection)", () => {
    localStorage.setItem(STORAGE_KEY, "provider-deepseek");

    const { result } = renderHook(() => useNavigation(), { wrapper });
    expect(result.current.appMode).toBe("skills");
    expect(result.current.selectedProviderId).toBe("provider-deepseek");

    act(() => {
      result.current.setAppMode("models");
    });

    expect(result.current.appMode).toBe("models");
    // Selection survives the mode switch — providers page will auto-open it.
    expect(result.current.selectedProviderId).toBe("provider-deepseek");
  });
});

describe("Property: Mode Switch Page Preservation (Round-Trip)", () => {
  /**
   * **Validates: Requirements 1.5**
   *
   * Property 1: For any sequence of mode switches where the user navigates to
   * page P in mode A, switches to mode B, then switches back to mode A,
   * the active page in mode A SHALL be P.
   */

  const skillsPages = fc.constantFrom<NavPage>("my-skills", "marketplace", "skill-cards", "projects", "settings");

  beforeEach(() => {
    window.location.hash = "";
    localStorage.clear();
  });

  it("skills page is preserved after switching to models and back", () => {
    fc.assert(
      fc.property(skillsPages, (skillsPage) => {
        window.location.hash = "";
        const { result } = renderHook(() => useNavigation(), { wrapper });

        act(() => {
          result.current.navigate(skillsPage);
        });
        expect(result.current.activePage).toBe(skillsPage);

        act(() => {
          result.current.setAppMode("models");
        });
        expect(result.current.appMode).toBe("models");
        expect(result.current.modelsActivePage).toBe("hub");

        act(() => {
          result.current.setAppMode("skills");
        });
        expect(result.current.appMode).toBe("skills");
        expect(result.current.activePage).toBe(skillsPage);
      }),
      { numRuns: 50 },
    );
  });

  it("models mode collapses to the hub page across round trips", () => {
    fc.assert(
      fc.property(skillsPages, (skillsPage) => {
        window.location.hash = "";
        const { result } = renderHook(() => useNavigation(), { wrapper });

        act(() => {
          result.current.setAppMode("models");
        });
        expect(result.current.modelsActivePage).toBe("hub");

        act(() => {
          result.current.navigate(skillsPage);
        });
        expect(result.current.appMode).toBe("skills");

        act(() => {
          result.current.setAppMode("models");
        });
        expect(result.current.appMode).toBe("models");
        expect(result.current.modelsActivePage).toBe("hub");
      }),
      { numRuns: 50 },
    );
  });
});

describe("Property: Mode Switch URL Hash Consistency", () => {
  /**
   * **Validates: Requirements 1.6**
   *
   * Property 2: For any mode/page combination, after a mode switch the URL hash
   * SHALL correctly encode the current mode and the active page within that mode.
   */

  const PAGE_TO_HASH: Record<NavPage, string> = {
    "my-skills": "skills",
    marketplace: "marketplace",
    "skill-cards": "cards",
    projects: "projects",
    mcp: "mcp",
    settings: "settings",
  };

  const skillsPages = fc.constantFrom<NavPage>(
    "my-skills",
    "marketplace",
    "skill-cards",
    "projects",
    "mcp",
    "settings",
  );

  beforeEach(() => {
    window.location.hash = "";
    localStorage.clear();
  });

  it("navigating to models always produces the single #models hash", () => {
    const { result } = renderHook(() => useNavigation(), { wrapper });

    act(() => {
      result.current.navigateModels("hub");
    });

    expect(window.location.hash).toBe("#models");
  });

  it("navigating to any skills page produces correct hash matching PAGE_TO_HASH", () => {
    fc.assert(
      fc.property(skillsPages, (skillsPage) => {
        window.location.hash = "";
        const { result } = renderHook(() => useNavigation(), { wrapper });

        act(() => {
          result.current.navigate(skillsPage);
        });

        expect(window.location.hash).toBe(`#${PAGE_TO_HASH[skillsPage]}`);
      }),
      { numRuns: 100 },
    );
  });

  it("switching back to models mode reproduces the hub hash", () => {
    fc.assert(
      fc.property(skillsPages, (skillsPage) => {
        window.location.hash = "";
        const { result } = renderHook(() => useNavigation(), { wrapper });

        act(() => {
          result.current.setAppMode("models");
        });
        expect(window.location.hash).toBe("#models");

        act(() => {
          result.current.navigate(skillsPage);
        });
        expect(window.location.hash).toBe(`#${PAGE_TO_HASH[skillsPage]}`);

        act(() => {
          result.current.setAppMode("models");
        });
        expect(window.location.hash).toBe("#models");
      }),
      { numRuns: 50 },
    );
  });
});
