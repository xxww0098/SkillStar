import { createContext, useContext, useState, useCallback, useEffect, useRef, useMemo } from "react";
import type { NavPage, SubPage } from "../types";
import type { TabId as MarketplaceTabId } from "../pages/Marketplace";

// ── Page imports (for prefetching) ──────────────────────────────────
const importMySkillsPage = () => import("../pages/MySkills");
const importMarketplacePage = () => import("../pages/Marketplace");
const importPublisherDetailPage = () => import("../pages/PublisherDetail");
const importSkillCardsPage = () => import("../pages/SkillCards");
const importProjectsPage = () => import("../pages/Projects");
const importSecurityScanPage = () => import("../pages/SecurityScan");
const importSettingsPage = () => import("../pages/Settings");

const ALL_PAGES: NavPage[] = [
  "my-skills", "marketplace", "skill-cards", "projects", "security-scan", "settings",
];

const DEFAULT_NEXT_PAGES: Record<NavPage, NavPage[]> = {
  "my-skills": ["marketplace", "projects"],
  marketplace: ["my-skills", "skill-cards"],
  "skill-cards": ["projects", "my-skills"],
  projects: ["my-skills", "security-scan"],
  "security-scan": ["settings", "my-skills"],
  settings: ["my-skills", "projects"],
};

const PAGE_IMPORTERS: Record<NavPage, () => Promise<unknown>> = {
  "my-skills": importMySkillsPage,
  marketplace: () => { void importPublisherDetailPage(); return importMarketplacePage(); },
  "skill-cards": importSkillCardsPage,
  projects: importProjectsPage,
  settings: importSettingsPage,
  "security-scan": importSecurityScanPage,
};

// ── Hash ↔ NavPage mapping ──────────────────────────────────────────
const PAGE_TO_HASH: Record<NavPage, string> = {
  "my-skills": "skills",
  marketplace: "marketplace",
  "skill-cards": "cards",
  projects: "projects",
  "security-scan": "security",
  settings: "settings",
};

const HASH_TO_PAGE: Record<string, NavPage> = Object.fromEntries(
  Object.entries(PAGE_TO_HASH).map(([page, hash]) => [hash, page as NavPage])
);

function pageFromHash(): NavPage {
  const hash = window.location.hash.slice(1);
  return HASH_TO_PAGE[hash] ?? "my-skills";
}

// ── Context types ───────────────────────────────────────────────────
interface NavigationState {
  activePage: NavPage;
  subPage: SubPage;
  projectsPreSelectedSkills: string[] | null;
  skillCardsPreSelectedSkills: string[] | null;
  mySkillsFocusSkill: string | null;
  marketplaceTab: MarketplaceTabId;
  clipboardShareCode: string | null;
}

interface NavigationActions {
  navigate: (page: NavPage) => void;
  setSubPage: (subPage: SubPage) => void;
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
}

type NavigationContext = NavigationState & NavigationActions;

const NavContext = createContext<NavigationContext | null>(null);

export function useNavigation(): NavigationContext {
  const ctx = useContext(NavContext);
  if (!ctx) throw new Error("useNavigation must be used within NavigationProvider");
  return ctx;
}

// ── Provider ────────────────────────────────────────────────────────
export function NavigationProvider({ children }: { children: React.ReactNode }) {
  const [activePage, setActivePage] = useState<NavPage>(pageFromHash);
  const [subPage, setSubPage] = useState<SubPage>(null);
  const [projectsPreSelectedSkills, setProjectsPreSelectedSkills] = useState<string[] | null>(null);
  const [skillCardsPreSelectedSkills, setSkillCardsPreSelectedSkills] = useState<string[] | null>(null);
  const [mySkillsFocusSkill, setMySkillsFocusSkill] = useState<string | null>(null);
  const [marketplaceTab, setMarketplaceTab] = useState<MarketplaceTabId>("all");
  const [clipboardShareCode, setClipboardShareCode] = useState<string | null>(null);

  const prefetchedPages = useRef<Set<NavPage>>(new Set([activePage]));
  const previousPage = useRef<NavPage>(activePage);
  const transitionScores = useRef<Record<NavPage, Partial<Record<NavPage, number>>>>(
    Object.fromEntries(ALL_PAGES.map((p) => [p, {}])) as Record<NavPage, Partial<Record<NavPage, number>>>
  );

  // ── Navigate ────────────────────────────────────────────────────
  const navigate = useCallback((page: NavPage) => {
    setActivePage(page);
    setSubPage(null);
    window.location.hash = PAGE_TO_HASH[page];
  }, []);

  // ── Convenience navigators ──────────────────────────────────────
  const goToProjectsWithSkills = useCallback((skills: string[]) => {
    setProjectsPreSelectedSkills(skills);
    navigate("projects");
  }, [navigate]);

  const goToSkillCardsWithSkills = useCallback((skills: string[]) => {
    setSkillCardsPreSelectedSkills(skills);
    navigate("skill-cards");
  }, [navigate]);

  const goToMySkillsFocus = useCallback((skill: string) => {
    setMySkillsFocusSkill(skill);
    navigate("my-skills");
  }, [navigate]);

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
      const page = pageFromHash();
      setActivePage((prev) => (prev !== page ? page : prev));
    };
    window.addEventListener("hashchange", handleHashChange);
    return () => window.removeEventListener("hashchange", handleHashChange);
  }, []);

  const value: NavigationContext = useMemo(() => ({
    activePage, subPage,
    projectsPreSelectedSkills, skillCardsPreSelectedSkills,
    mySkillsFocusSkill, marketplaceTab, clipboardShareCode,
    navigate, setSubPage,
    setProjectsPreSelectedSkills, setSkillCardsPreSelectedSkills,
    setMySkillsFocusSkill, setMarketplaceTab, setClipboardShareCode,
    goToProjectsWithSkills, goToSkillCardsWithSkills, goToMySkillsFocus,
  }), [
    activePage, subPage,
    projectsPreSelectedSkills, skillCardsPreSelectedSkills,
    mySkillsFocusSkill, marketplaceTab, clipboardShareCode,
    navigate, setSubPage,
    setProjectsPreSelectedSkills, setSkillCardsPreSelectedSkills,
    setMySkillsFocusSkill, setMarketplaceTab, setClipboardShareCode,
    goToProjectsWithSkills, goToSkillCardsWithSkills, goToMySkillsFocus,
  ]);

  return <NavContext.Provider value={value}>{children}</NavContext.Provider>;
}
