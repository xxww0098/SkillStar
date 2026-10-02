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

/** Models hub drawer deep-link request (request-nonce pattern, like usageCreateRequest). */
export interface ModelsDrawerRequest {
  nonce: number;
  kind: "create" | "edit";
  providerId?: string;
  autoBindToolId?: string;
}

/**
 * Cross-view focus request into the Models hub (spec slice 13's triangle):
 * either "this agent's model routes" (from a Usage session chip) or "which
 * agents route to this catalog" (from a Usage quota card). Same
 * request-nonce pattern as `ModelsDrawerRequest`.
 */
export type ModelsFocusRequest =
  | { nonce: number; kind: "agent"; agentId: string }
  | { nonce: number; kind: "catalog"; catalogId: string };

const DEFAULT_NEXT_PAGES: Record<NavPage, NavPage[]> = {
  "my-skills": ["marketplace", "skill-cards"],
  marketplace: ["my-skills", "skill-cards"],
  "skill-cards": ["projects", "my-skills"],
  projects: ["my-skills", "settings"],
  settings: ["my-skills", "marketplace"],
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
//
// The Models hub merges what used to be four sub-pages into a single workbench.
// We keep `ModelsNavPage` (typed as `"hub"`) and the `navigateModels` API for
// back-compat with downstream callers, but every entry just lands on the hub.
const MODELS_HASH = "models";
const MODELS_LEGACY_HASH_PREFIX = "models/";
const DEFAULT_MODELS_PAGE: ModelsNavPage = "hub";

const USAGE_HASH = "usage";

function isModelsHash(hash: string): boolean {
  return hash === MODELS_HASH || hash.startsWith(MODELS_LEGACY_HASH_PREFIX);
}

function isUsageHash(hash: string): boolean {
  return hash === USAGE_HASH;
}

function modelsPageFromHash(_hash: string): ModelsNavPage {
  return DEFAULT_MODELS_PAGE;
}

function pageFromHash(): NavPage {
  const hash = window.location.hash.slice(1);
  if (isModelsHash(hash) || isUsageHash(hash)) return "my-skills";
  if (hash === "ssh" || hash === "learn") return "my-skills";
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
  projectsPreSelectedSkills: string[] | null;
  skillCardsPreSelectedSkills: string[] | null;
  mySkillsFocusSkill: string | null;
  marketplaceTab: MarketplaceTabId;
  clipboardShareCode: string | null;
  usageCatalogFilter: CatalogFilter;
  usageCreateRequest: { nonce: number; preselectCatalogId: string | null } | null;
  /** Request-nonce event asking the Models hub to open its drawer. */
  modelsDrawerRequest: ModelsDrawerRequest | null;
  /** Request-nonce event focusing the Models hub's route comparison. */
  modelsFocusRequest: ModelsFocusRequest | null;
}

interface NavigationActions {
  navigate: (page: NavPage) => void;
  setSubPage: (subPage: SubPage) => void;
  setAppMode: (mode: AppMode) => void;
  navigateModels: (page: ModelsNavPage) => void;
  setSelectedProviderId: (id: string | null) => void;
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
  openModelsDrawer: (req: Omit<ModelsDrawerRequest, "nonce">) => void;
  clearModelsDrawerRequest: () => void;
  /** Jump to the Models hub focused on one agent's model routes (Usage → Models triangle). */
  focusModelsAgent: (agentId: string) => void;
  /** Jump to the Models hub asking which agents route to this catalog (Usage → Models triangle). */
  focusModelsCatalog: (catalogId: string) => void;
  clearModelsFocusRequest: () => void;
  /** Warm a page's lazy chunk before the user commits to it (hover/focus). */
  prefetchPage: (page: NavPage) => void;
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
  const [modelsDrawerRequest, setModelsDrawerRequest] = useState<ModelsDrawerRequest | null>(null);
  const [modelsFocusRequest, setModelsFocusRequest] = useState<ModelsFocusRequest | null>(null);

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
        window.location.hash = MODELS_HASH;
      } else if (mode === "usage") {
        window.location.hash = USAGE_HASH;
        void importUsagePage();
      } else {
        window.location.hash = PAGE_TO_HASH[activePage];
      }
    },
    [activePage],
  );

  // ── Navigate within Models mode (kept for back-compat) ─────────
  // After the hub redesign every entry lands on the same hub page; we still
  // expose the action so existing call sites keep compiling.
  const navigateModels = useCallback((_page: ModelsNavPage) => {
    setModelsActivePage(DEFAULT_MODELS_PAGE);
    setAppModeState("models");
    window.location.hash = MODELS_HASH;
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

  const openModelsDrawer = useCallback((req: Omit<ModelsDrawerRequest, "nonce">) => {
    setAppModeState("models");
    window.location.hash = MODELS_HASH;
    setModelsDrawerRequest((prev) => ({ ...req, nonce: (prev?.nonce ?? 0) + 1 }));
  }, []);

  const clearModelsDrawerRequest = useCallback(() => {
    setModelsDrawerRequest(null);
  }, []);

  // ── Cross-view focus (Usage → Models triangle, slice 13) ────────
  const focusModelsAgent = useCallback((agentId: string) => {
    setAppModeState("models");
    window.location.hash = MODELS_HASH;
    setModelsFocusRequest((prev) => ({ kind: "agent" as const, agentId, nonce: (prev?.nonce ?? 0) + 1 }));
  }, []);

  const focusModelsCatalog = useCallback((catalogId: string) => {
    setAppModeState("models");
    window.location.hash = MODELS_HASH;
    setModelsFocusRequest((prev) => ({ kind: "catalog" as const, catalogId, nonce: (prev?.nonce ?? 0) + 1 }));
  }, []);

  const clearModelsFocusRequest = useCallback(() => {
    setModelsFocusRequest(null);
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
        const page = pageFromHash();
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
      projectsPreSelectedSkills,
      skillCardsPreSelectedSkills,
      mySkillsFocusSkill,
      marketplaceTab,
      clipboardShareCode,
      usageCatalogFilter,
      usageCreateRequest,
      modelsDrawerRequest,
      modelsFocusRequest,
      navigate,
      setSubPage,
      setAppMode,
      navigateModels,
      setSelectedProviderId,
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
      openModelsDrawer,
      clearModelsDrawerRequest,
      focusModelsAgent,
      focusModelsCatalog,
      clearModelsFocusRequest,
      prefetchPage,
    }),
    [
      activePage,
      subPage,
      appMode,
      modelsActivePage,
      selectedProviderId,
      projectsPreSelectedSkills,
      skillCardsPreSelectedSkills,
      mySkillsFocusSkill,
      marketplaceTab,
      clipboardShareCode,
      usageCatalogFilter,
      usageCreateRequest,
      modelsDrawerRequest,
      modelsFocusRequest,
      navigate,
      setAppMode,
      navigateModels,
      goToProjectsWithSkills,
      goToSkillCardsWithSkills,
      goToMySkillsFocus,
      openUsageCreate,
      clearUsageCreateRequest,
      openModelsDrawer,
      clearModelsDrawerRequest,
      focusModelsAgent,
      focusModelsCatalog,
      clearModelsFocusRequest,
      prefetchPage,
    ],
  );

  return <NavContext.Provider value={value}>{children}</NavContext.Provider>;
}
