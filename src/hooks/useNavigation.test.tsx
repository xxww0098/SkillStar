import { act, renderHook } from "@testing-library/react";
import fc from "fast-check";
import type { ReactNode } from "react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { ModelsNavPage, NavPage } from "../types";
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
    expect(result.current.modelsActivePage).toBe("providers");
  });

  it("switches to models mode and updates hash", () => {
    const { result } = renderHook(() => useNavigation(), { wrapper });

    act(() => {
      result.current.setAppMode("models");
    });

    expect(result.current.appMode).toBe("models");
    expect(window.location.hash).toBe("#models/providers");
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
    expect(window.location.hash).toBe("#models/providers");

    act(() => {
      result.current.setAppMode("skills");
    });
    expect(result.current.appMode).toBe("skills");
    expect(window.location.hash).toBe("#projects");
  });

  it("preserves models page when switching modes (round-trip)", () => {
    const { result } = renderHook(() => useNavigation(), { wrapper });

    // Navigate to health in models mode
    act(() => {
      result.current.navigateModels("health");
    });
    expect(result.current.modelsActivePage).toBe("health");
    expect(window.location.hash).toBe("#models/health");

    // Switch to skills
    act(() => {
      result.current.setAppMode("skills");
    });
    expect(result.current.appMode).toBe("skills");

    // Switch back to models — should restore "health"
    act(() => {
      result.current.setAppMode("models");
    });
    expect(result.current.appMode).toBe("models");
    expect(result.current.modelsActivePage).toBe("health");
    expect(window.location.hash).toBe("#models/health");
  });

  it("navigateModels updates modelsActivePage and hash", () => {
    const { result } = renderHook(() => useNavigation(), { wrapper });

    act(() => {
      result.current.navigateModels("tool-configs");
    });

    expect(result.current.appMode).toBe("models");
    expect(result.current.modelsActivePage).toBe("tool-configs");
    expect(window.location.hash).toBe("#models/tool-configs");
  });

  it("initializes from models hash in URL", () => {
    window.location.hash = "#models/health";

    const { result } = renderHook(() => useNavigation(), { wrapper });

    expect(result.current.appMode).toBe("models");
    expect(result.current.modelsActivePage).toBe("health");
  });

  it("handles hashchange to models hash", () => {
    const { result } = renderHook(() => useNavigation(), { wrapper });

    act(() => {
      window.location.hash = "#models/tool-configs";
      window.dispatchEvent(new HashChangeEvent("hashchange"));
    });

    expect(result.current.appMode).toBe("models");
    expect(result.current.modelsActivePage).toBe("tool-configs");
  });

  it("handles hashchange from models to skills hash", () => {
    window.location.hash = "#models/providers";
    const { result } = renderHook(() => useNavigation(), { wrapper });

    act(() => {
      window.location.hash = "#marketplace";
      window.dispatchEvent(new HashChangeEvent("hashchange"));
    });

    expect(result.current.appMode).toBe("skills");
    expect(result.current.activePage).toBe("marketplace");
  });

  it("falls back to default models page for unknown models hash", () => {
    window.location.hash = "#models/unknown-page";

    const { result } = renderHook(() => useNavigation(), { wrapper });

    expect(result.current.appMode).toBe("models");
    expect(result.current.modelsActivePage).toBe("providers");
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

  it("closes the preset selector when entering Models mode", () => {
    const { result } = renderHook(() => useNavigation(), { wrapper });

    act(() => {
      result.current.setShowPresetSelector(true);
    });
    expect(result.current.showPresetSelector).toBe(true);

    act(() => {
      result.current.setAppMode("models");
    });

    expect(result.current.appMode).toBe("models");
    expect(result.current.showPresetSelector).toBe(false);
  });

  it("does not touch the preset selector when leaving Models mode", () => {
    const { result } = renderHook(() => useNavigation(), { wrapper });

    act(() => {
      result.current.setAppMode("models");
      result.current.setShowPresetSelector(true);
    });
    expect(result.current.showPresetSelector).toBe(true);

    act(() => {
      result.current.setAppMode("skills");
    });

    expect(result.current.appMode).toBe("skills");
    // Going back to skills shouldn't silently flip the preset selector.
    expect(result.current.showPresetSelector).toBe(true);
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

  const modelsPages = fc.constantFrom<ModelsNavPage>("providers", "health", "tool-configs", "models-settings");

  beforeEach(() => {
    window.location.hash = "";
    localStorage.clear();
  });

  it("skills page is preserved after switching to models and back", () => {
    fc.assert(
      fc.property(skillsPages, modelsPages, (skillsPage, modelsPage) => {
        window.location.hash = "";
        const { result } = renderHook(() => useNavigation(), { wrapper });

        // Navigate to a skills page
        act(() => {
          result.current.navigate(skillsPage);
        });
        expect(result.current.activePage).toBe(skillsPage);

        // Switch to models mode and navigate to a models page
        act(() => {
          result.current.navigateModels(modelsPage);
        });
        expect(result.current.appMode).toBe("models");
        expect(result.current.modelsActivePage).toBe(modelsPage);

        // Switch back to skills mode — skills page should be preserved
        act(() => {
          result.current.setAppMode("skills");
        });
        expect(result.current.appMode).toBe("skills");
        expect(result.current.activePage).toBe(skillsPage);
      }),
      { numRuns: 100 },
    );
  });

  it("models page is preserved after switching to skills and back", () => {
    fc.assert(
      fc.property(skillsPages, modelsPages, (skillsPage, modelsPage) => {
        window.location.hash = "";
        const { result } = renderHook(() => useNavigation(), { wrapper });

        // Navigate to a models page
        act(() => {
          result.current.navigateModels(modelsPage);
        });
        expect(result.current.modelsActivePage).toBe(modelsPage);

        // Switch to skills mode and navigate to a skills page
        act(() => {
          result.current.navigate(skillsPage);
        });
        expect(result.current.appMode).toBe("skills");
        expect(result.current.activePage).toBe(skillsPage);

        // Switch back to models mode — models page should be preserved
        act(() => {
          result.current.setAppMode("models");
        });
        expect(result.current.appMode).toBe("models");
        expect(result.current.modelsActivePage).toBe(modelsPage);
      }),
      { numRuns: 100 },
    );
  });

  it("both pages are preserved in a full round-trip", () => {
    fc.assert(
      fc.property(skillsPages, modelsPages, (skillsPage, modelsPage) => {
        window.location.hash = "";
        const { result } = renderHook(() => useNavigation(), { wrapper });

        // Step 1: Navigate to skills page
        act(() => {
          result.current.navigate(skillsPage);
        });

        // Step 2: Switch to models and navigate to models page
        act(() => {
          result.current.navigateModels(modelsPage);
        });

        // Step 3: Switch back to skills — skills page preserved
        act(() => {
          result.current.setAppMode("skills");
        });
        expect(result.current.activePage).toBe(skillsPage);

        // Step 4: Switch back to models — models page preserved
        act(() => {
          result.current.setAppMode("models");
        });
        expect(result.current.modelsActivePage).toBe(modelsPage);
      }),
      { numRuns: 100 },
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
    settings: "settings",
  };

  const skillsPages = fc.constantFrom<NavPage>("my-skills", "marketplace", "skill-cards", "projects", "settings");

  const modelsPages = fc.constantFrom<ModelsNavPage>("providers", "health", "tool-configs", "models-settings");

  beforeEach(() => {
    window.location.hash = "";
    localStorage.clear();
  });

  it("navigating to any models page produces correct hash #models/{page}", () => {
    fc.assert(
      fc.property(modelsPages, (modelsPage) => {
        window.location.hash = "";
        const { result } = renderHook(() => useNavigation(), { wrapper });

        act(() => {
          result.current.navigateModels(modelsPage);
        });

        expect(window.location.hash).toBe(`#models/${modelsPage}`);
      }),
      { numRuns: 100 },
    );
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

  it("switching to models mode produces hash encoding the remembered models page", () => {
    fc.assert(
      fc.property(skillsPages, modelsPages, (skillsPage, modelsPage) => {
        window.location.hash = "";
        const { result } = renderHook(() => useNavigation(), { wrapper });

        // Set up a models page in memory
        act(() => {
          result.current.navigateModels(modelsPage);
        });

        // Switch to skills
        act(() => {
          result.current.navigate(skillsPage);
        });
        expect(window.location.hash).toBe(`#${PAGE_TO_HASH[skillsPage]}`);

        // Switch back to models — hash should reflect the remembered models page
        act(() => {
          result.current.setAppMode("models");
        });
        expect(window.location.hash).toBe(`#models/${modelsPage}`);
      }),
      { numRuns: 100 },
    );
  });

  it("switching to skills mode produces hash encoding the remembered skills page", () => {
    fc.assert(
      fc.property(skillsPages, modelsPages, (skillsPage, modelsPage) => {
        window.location.hash = "";
        const { result } = renderHook(() => useNavigation(), { wrapper });

        // Navigate to a skills page first
        act(() => {
          result.current.navigate(skillsPage);
        });

        // Switch to models
        act(() => {
          result.current.navigateModels(modelsPage);
        });
        expect(window.location.hash).toBe(`#models/${modelsPage}`);

        // Switch back to skills — hash should reflect the remembered skills page
        act(() => {
          result.current.setAppMode("skills");
        });
        expect(window.location.hash).toBe(`#${PAGE_TO_HASH[skillsPage]}`);
      }),
      { numRuns: 100 },
    );
  });
});
