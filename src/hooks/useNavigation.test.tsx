import { act, renderHook } from "@testing-library/react";
import fc from "fast-check";
import type { ReactNode } from "react";
import { beforeEach, describe, expect, it } from "vitest";
import type { NavPage } from "../types";
import { NavigationProvider, useAppMode, useNavigation } from "./useNavigation";

function wrapper({ children }: { children: ReactNode }) {
  return <NavigationProvider>{children}</NavigationProvider>;
}

describe("useNavigation - AppMode support", () => {
  beforeEach(() => {
    window.location.hash = "";
  });

  it("defaults to skills mode", () => {
    const { result } = renderHook(() => useNavigation(), { wrapper });
    expect(result.current.appMode).toBe("skills");
    expect(result.current.activePage).toBe("my-skills");
  });

  it("switches to usage mode and updates hash to #usage", () => {
    const { result } = renderHook(() => useNavigation(), { wrapper });

    act(() => {
      result.current.setAppMode("usage");
    });

    expect(result.current.appMode).toBe("usage");
    expect(window.location.hash).toBe("#usage");
  });

  it("switches back to skills mode and restores skills hash", () => {
    const { result } = renderHook(() => useNavigation(), { wrapper });

    act(() => {
      result.current.navigate("projects");
    });
    expect(window.location.hash).toBe("#projects");

    act(() => {
      result.current.setAppMode("usage");
    });
    expect(window.location.hash).toBe("#usage");

    act(() => {
      result.current.setAppMode("skills");
    });
    expect(result.current.appMode).toBe("skills");
    expect(window.location.hash).toBe("#projects");
  });

  it("handles hashchange into usage mode", () => {
    const { result } = renderHook(() => useNavigation(), { wrapper });

    act(() => {
      window.location.hash = "#usage";
      window.dispatchEvent(new HashChangeEvent("hashchange"));
    });

    expect(result.current.appMode).toBe("usage");
  });

  it("handles hashchange from usage to skills hash", () => {
    window.location.hash = "#usage";
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
      result.current.setAppMode("usage");
    });
    expect(result.current.appMode).toBe("usage");

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
  });

  it("returns mode and derived booleans", () => {
    const { result } = renderHook(() => useAppMode(), { wrapper });

    expect(result.current.mode).toBe("skills");
    expect(result.current.isSkillsMode).toBe(true);
    expect(result.current.isUsageMode).toBe(false);
  });

  it("setMode switches to usage", () => {
    const { result } = renderHook(() => useAppMode(), { wrapper });

    act(() => {
      result.current.setMode("usage");
    });

    expect(result.current.mode).toBe("usage");
    expect(result.current.isSkillsMode).toBe(false);
    expect(result.current.isUsageMode).toBe(true);
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
  });

  it("skills page is preserved after switching to usage and back", () => {
    fc.assert(
      fc.property(skillsPages, (skillsPage) => {
        window.location.hash = "";
        const { result } = renderHook(() => useNavigation(), { wrapper });

        act(() => {
          result.current.navigate(skillsPage);
        });
        expect(result.current.activePage).toBe(skillsPage);

        act(() => {
          result.current.setAppMode("usage");
        });
        expect(result.current.appMode).toBe("usage");

        act(() => {
          result.current.setAppMode("skills");
        });
        expect(result.current.appMode).toBe("skills");
        expect(result.current.activePage).toBe(skillsPage);
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
    settings: "settings",
  };

  const skillsPages = fc.constantFrom<NavPage>("my-skills", "marketplace", "skill-cards", "projects", "settings");

  beforeEach(() => {
    window.location.hash = "";
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

  it("switching back to usage mode reproduces the usage hash", () => {
    fc.assert(
      fc.property(skillsPages, (skillsPage) => {
        window.location.hash = "";
        const { result } = renderHook(() => useNavigation(), { wrapper });

        act(() => {
          result.current.setAppMode("usage");
        });
        expect(window.location.hash).toBe("#usage");

        act(() => {
          result.current.navigate(skillsPage);
        });
        expect(window.location.hash).toBe(`#${PAGE_TO_HASH[skillsPage]}`);

        act(() => {
          result.current.setAppMode("usage");
        });
        expect(window.location.hash).toBe("#usage");
      }),
      { numRuns: 50 },
    );
  });
});
