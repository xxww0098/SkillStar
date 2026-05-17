import { createContext, useCallback, useContext, useEffect, useMemo, useRef, useState } from "react";
import type { TabId as MarketplaceTabId } from "../pages/Marketplace";
import { FILTER_ALL, type CatalogFilter } from "../features/usage/types";
import type { AppMode, ModelsNavPage, NavPage, SubPage } from "../types";

// ── Page imports (for prefetching) ──────────────────────────────────
const importMySkillsPage = () => import("../pages/MySkills");
const importMarketplacePage = () => import("../pages/Marketplace");
const importPublisherDetailPage = () => import("../pages/PublisherDetail");
const importSkillCardsPage = () => import("../pages/SkillCards");
const importProjectsPage = () => import("../pages/Projects");
const importSettingsPage = () => import("../pages/Settings");
const importUsagePage = () => import("../pages/Usage");

const ALL_PAGES: NavPage[] = ["my-skills", "marketplace", "skill-cards", "projects", "settings"];

const DEFAULT_NEXT_PAGES: Record<NavPage, NavPage[]> = {
  "my-skills": ["marketplace", "projects"],
  marketplace: ["my-skills", "skill-cards"],
  "skill-cards": ["projects", "my-skills"],
  projects: ["my-skills", "settings"],
  settings: ["my-skills", "projects"],
};

const PAGE_IMPORTERS: Record<NavPage, () => Promise<unknown>> = {
  "my-skills": importMySkillsPage,
  marketplace: () => {
    void importPublisherDetailPage();
    return importMarketplacePage();
  },
  "skill-cards": importSkillCardsPage,
  projects: importProjectsPage,
  settings: importSettingsPage,
};

// ── Persisted "last edited model provider" ─────────────────────────
// Clicking the Models toggle should re-open whichever provider the user
// last edited, even across reloads. We store just the id and let the
// providers list / detail panel handle "missing" gracefully.
const LAST_PROVIDER_STORAGE_KEY = "skillstar.lastEditedProviderId";

function loadLastProviderId(): string | null {
  try {
    const raw = localStorage.getItem(LAST_PROVIDER_STORAGE_KEY);
    return raw && raw.length > 0 ? raw : null;
  } catch {
    return null;
  }
}

function persistLastProviderId(id: string | null): void {
  try {
    if (id == null || id.length === 0) {
      localStorage.removeItem(LAST_PROVIDER_STORAGE_KEY);
    } else {
      localStorage.setItem(LAST_PROVIDER_STORAGE_KEY, id);
    }
  } catch {
    /* storage unavailable — fall back to in-memory only */
  }
}

// ── Hash ↔ NavPage mapping ──────────────────────────────────────────
const PAGE_TO_HASH: Record<NavPage, string> = {
  "my-skills": "skills",
  marketplace: "marketplace",
  "skill-cards": "cards",
  projects: "projects",
  settings: "settings",
};

const HASH_TO_PAGE: Record<string, NavPage> = Object.fromEntries(
  Object.entries(PAGE_TO_HASH).map(([page, hash]) => [hash, page as NavPage]),
);

// ── Models mode hash mapping ────────────────────────────────────────
const MODELS_PAGES: ModelsNavPage[] = ["providers", "health", "tool-configs", "models-settings"];
const MODELS_HASH_PREFIX = "models/";
const DEFAULT_MODELS_PAGE: ModelsNavPage = "providers";

const USAGE_HASH = "usage";

function isModelsHash(hash: string): boolean {
  return hash.startsWith(MODELS_HASH_PREFIX);
}

function isUsageHash(hash: string): boolean {
  return hash === USAGE_HASH;
}

function modelsPageFromHash(hash: string): ModelsNavPage {
  const page = hash.slice(MODELS_HASH_PREFIX.length);
  return (MODELS_PAGES as string[]).includes(page) ? (page as ModelsNavPage) : DEFAULT_MODELS_PAGE;
}

function pageFromHash(): NavPage {
  const hash = window.location.hash.slice(1);
  if (isModelsHash(hash) || isUsageHash(hash)) return "my-skills";
  return HASH_TO_PAGE[hash] ?? "my-skills";
}

function appModeFromHash(): AppMode {
  const hash = window.location.hash.slice(1);
  if (isModelsHash(hash)) return "models";
  if (isUsageHash(hash)) return "usage";
  return "skills";
}

function modelsActivePageFromHash(): ModelsNavPage {
  const hash = window.location.hash.slice(1);
  if (isModelsHash(hash)) return modelsPageFromHash(hash);
  return DEFAULT_MODELS_PAGE;
}

// ── Context types ───────────────────────────────────────────────────
interface NavigationState {
  activePage: NavPage;
  subPage: SubPage;
  appMode: AppMode;
  modelsActivePage: ModelsNavPage;
  selectedProviderId: string | null;
  showPresetSelector: boolean;
  projectsPreSelectedSkills: string[] | null;
  skillCardsPreSelectedSkills: string[] | null;
  mySkillsFocusSkill: string | null;
  marketplaceTab: MarketplaceTabId;
  clipboardShareCode: string | null;
  usageCatalogFilter: CatalogFilter;
  usageCreateRequest: { nonce: number; preselectCatalogId: string | null } | null;
}

interface NavigationActions {
  navigate: (page: NavPage) => void;
  setSubPage: (subPage: SubPage) => void;
  setAppMode: (mode: AppMode) => void;
  navigateModels: (page: ModelsNavPage) => void;
  setSelectedProviderId: (id: string | null) => void;
  setShowPresetSelector: (show: boolean) => void;
  setProjectsPreSelectedSkills: (skills: string[] | null) => void;
  setSkillCardsPreSelectedSkills: (skills: string[] | null) => void;
  setMySkillsFocusSkill: (skill: string | null) => void;
  setMarketplaceTab: (tab: MarketplaceTabId) => void;
  setClipboardShareCode: (code: string | null) => void;
  /** Navigate to projects with pre-selected skills */
  goToProjectsWithSkills: (skills: string[]) => void;
  /** Navigate to skill-cards with pre-selected skills */
  goToSkillCardsWithSkills: (skills: string[]) => void;
  /** Navigate to my-skills and focus a skill */
  goToMySkillsFocus: (skill: string) => void;
  setUsageCatalogFilter: (filter: CatalogFilter) => void;
  openUsageCreate: (preselectCatalogId?: string | null) => void;
  clearUsageCreateRequest: () => void;
}

type NavigationContext = NavigationState & NavigationActions;

const NavContext = createContext<NavigationContext | null>(null);

export function useNavigation(): NavigationContext {
  const ctx = useContext(NavContext);
  if (!ctx) throw new Error("useNavigation must be used within NavigationProvider");
  return ctx;
}

/** Convenience hook for app mode state */
export function useAppMode() {
  const { appMode, setAppMode } = useNavigation();
  return {
    mode: appMode,
    setMode: setAppMode,
    isSkillsMode: appMode === "skills",
    isUsageMode: appMode === "usage",
    isModelsMode: appMode === "models",
  };
}

// ── Provider ────────────────────────────────────────────────────────
export function NavigationProvider({ children }: { children: React.ReactNode }) {
  const [activePage, setActivePage] = useState<NavPage>(pageFromHash);
  const [subPage, setSubPage] = useState<SubPage>(null);
  const [appMode, setAppModeState] = useState<AppMode>(appModeFromHash);
  const [modelsActivePage, setModelsActivePage] = useState<ModelsNavPage>(modelsActivePageFromHash);
  // Rehydrate the last edited provider so clicking the Models toggle
  // (or refreshing the app) reopens whatever the user was working on.
  const [selectedProviderId, setSelectedProviderIdState] = useState<string | null>(loadLastProviderId);
  const setSelectedProviderId = useCallback((id: string | null) => {
    setSelectedProviderIdState(id);
    persistLastProviderId(id);
  }, []);
  const [showPresetSelector, setShowPresetSelector] = useState(false);
  const [projectsPreSelectedSkills, setProjectsPreSelectedSkills] = useState<string[] | null>(null);
  const [skillCardsPreSelectedSkills, setSkillCardsPreSelectedSkills] = useState<string[] | null>(null);
  const [mySkillsFocusSkill, setMySkillsFocusSkill] = useState<string | null>(null);
  const [marketplaceTab, setMarketplaceTab] = useState<MarketplaceTabId>("all");
  const [clipboardShareCode, setClipboardShareCode] = useState<string | null>(null);
  const [usageCatalogFilter, setUsageCatalogFilter] = useState<CatalogFilter>(FILTER_ALL);
  const [usageCreateRequest, setUsageCreateRequest] = useState<{
    nonce: number;
    preselectCatalogId: string | null;
  } | null>(null);

  const prefetchedPages = useRef<Set<NavPage>>(new Set([activePage]));
  const previousPage = useRef<NavPage>(activePage);
  const transitionScores = useRef<Record<NavPage, Partial<Record<NavPage, number>>>>(
    Object.fromEntries(ALL_PAGES.map((p) => [p, {}])) as Record<NavPage, Partial<Record<NavPage, number>>>,
  );

  // ── Navigate ────────────────────────────────────────────────────
  const navigate = useCallback((page: NavPage) => {
    setActivePage(page);
    setSubPage(null);
    setAppModeState("skills");
    window.location.hash = PAGE_TO_HASH[page];
  }, []);

  // ── App Mode Switch ─────────────────────────────────────────────
  const setAppMode = useCallback(
    (mode: AppMode) => {
      setAppModeState(mode);
      if (mode === "models") {
        // Land on the provider detail (using the rehydrated
        // selectedProviderId), not the "新增供应商" preset wizard,
        // even if it was open before the mode switch.
        setShowPresetSelector(false);
        window.location.hash = `${MODELS_HASH_PREFIX}${modelsActivePage}`;
      } else if (mode === "usage") {
        setShowPresetSelector(false);
        window.location.hash = USAGE_HASH;
        void importUsagePage();
      } else {
        window.location.hash = PAGE_TO_HASH[activePage];
      }
    },
    [modelsActivePage, activePage],
  );

  // ── Navigate within Models mode ─────────────────────────────────
  const navigateModels = useCallback((page: ModelsNavPage) => {
    setModelsActivePage(page);
    setAppModeState("models");
    window.location.hash = `${MODELS_HASH_PREFIX}${page}`;
  }, []);

  // ── Convenience navigators ──────────────────────────────────────
  const goToProjectsWithSkills = useCallback(
    (skills: string[]) => {
      setProjectsPreSelectedSkills(skills);
      navigate("projects");
    },
    [navigate],
  );

  const goToSkillCardsWithSkills = useCallback(
    (skills: string[]) => {
      setSkillCardsPreSelectedSkills(skills);
      navigate("skill-cards");
    },
    [navigate],
  );

  const goToMySkillsFocus = useCallback(
    (skill: string) => {
      setMySkillsFocusSkill(skill);
      navigate("my-skills");
    },
    [navigate],
  );

  const openUsageCreate = useCallback((preselectCatalogId?: string | null) => {
    setUsageCreateRequest((prev) => ({
      nonce: (prev?.nonce ?? 0) + 1,
      preselectCatalogId: preselectCatalogId ?? null,
    }));
  }, []);

  const clearUsageCreateRequest = useCallback(() => {
    setUsageCreateRequest(null);
  }, []);

  // ── Prefetching ─────────────────────────────────────────────────
  const prefetchPage = useCallback((page: NavPage) => {
    if (prefetchedPages.current.has(page)) return;
    prefetchedPages.current.add(page);
    PAGE_IMPORTERS[page]?.();
  }, []);

  const getLikelyNextPages = useCallback((from: NavPage): NavPage[] => {
    const scored = transitionScores.current[from];
    const learned = Object.entries(scored)
      .sort((a, b) => (b[1] ?? 0) - (a[1] ?? 0))
      .map(([p]) => p as NavPage);
    const defaults = DEFAULT_NEXT_PAGES[from];
    const merged: NavPage[] = [];
    for (const p of [...learned, ...defaults, ...ALL_PAGES]) {
      if (p === from || merged.includes(p)) continue;
      merged.push(p);
      if (merged.length >= 2) break;
    }
    return merged;
  }, []);

  // Track transitions & prefetch
  useEffect(() => {
    const prev = previousPage.current;
    if (prev !== activePage) {
      const s = transitionScores.current[prev][activePage] ?? 0;
      transitionScores.current[prev][activePage] = s + 1;
      previousPage.current = activePage;
    }
    const timer = window.setTimeout(() => {
      for (const p of getLikelyNextPages(activePage)) prefetchPage(p);
    }, 250);
    return () => window.clearTimeout(timer);
  }, [activePage, getLikelyNextPages, prefetchPage]);

  // Sync hash on external navigation
  useEffect(() => {
    const handleExternalNavigate = (event: CustomEvent<{ page?: NavPage }>) => {
      const page = event.detail?.page;
      if (!page) return;
      setActivePage(page);
      setSubPage(null);
      window.location.hash = PAGE_TO_HASH[page];
    };
    window.addEventListener("skillstar:navigate", handleExternalNavigate as EventListener);
    return () => window.removeEventListener("skillstar:navigate", handleExternalNavigate as EventListener);
  }, []);

  // Listen for browser back/forward
  useEffect(() => {
    const handleHashChange = () => {
      const hash = window.location.hash.slice(1);
      if (isModelsHash(hash)) {
        const page = modelsPageFromHash(hash);
        setAppModeState("models");
        setModelsActivePage(page);
      } else if (isUsageHash(hash)) {
        setAppModeState("usage");
      } else {
        const page = HASH_TO_PAGE[hash] ?? "my-skills";
        setAppModeState("skills");
        setActivePage((prev) => (prev !== page ? page : prev));
      }
    };
    window.addEventListener("hashchange", handleHashChange);
    return () => window.removeEventListener("hashchange", handleHashChange);
  }, []);

  const value: NavigationContext = useMemo(
    () => ({
      activePage,
      subPage,
      appMode,
      modelsActivePage,
      selectedProviderId,
      showPresetSelector,
      projectsPreSelectedSkills,
      skillCardsPreSelectedSkills,
      mySkillsFocusSkill,
      marketplaceTab,
      clipboardShareCode,
      usageCatalogFilter,
      usageCreateRequest,
      navigate,
      setSubPage,
      setAppMode,
      navigateModels,
      setSelectedProviderId,
      setShowPresetSelector,
      setProjectsPreSelectedSkills,
      setSkillCardsPreSelectedSkills,
      setMySkillsFocusSkill,
      setMarketplaceTab,
      setClipboardShareCode,
      goToProjectsWithSkills,
      goToSkillCardsWithSkills,
      goToMySkillsFocus,
      setUsageCatalogFilter,
      openUsageCreate,
      clearUsageCreateRequest,
    }),
    [
      activePage,
      subPage,
      appMode,
      modelsActivePage,
      selectedProviderId,
      showPresetSelector,
      projectsPreSelectedSkills,
      skillCardsPreSelectedSkills,
      mySkillsFocusSkill,
      marketplaceTab,
      clipboardShareCode,
      usageCatalogFilter,
      usageCreateRequest,
      navigate,
      setAppMode,
      navigateModels,
      goToProjectsWithSkills,
      goToSkillCardsWithSkills,
      goToMySkillsFocus,
      openUsageCreate,
      clearUsageCreateRequest,
    ],
  );

  return <NavContext.Provider value={value}>{children}</NavContext.Provider>;
}
