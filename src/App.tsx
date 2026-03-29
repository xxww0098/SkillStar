import { useState, useCallback, useEffect, lazy, Suspense, useRef } from "react";
import { AnimatePresence, motion } from "framer-motion";
import { Sidebar } from "./components/layout/Sidebar";
import { useUpdater } from "./hooks/useUpdater";
import type { NavPage, SubPage } from "./types";
import type { TabId as MarketplaceTabId } from "./pages/Marketplace";

const importMySkillsPage = () => import("./pages/MySkills");
const importMarketplacePage = () => import("./pages/Marketplace");
const importPublisherDetailPage = () => import("./pages/PublisherDetail");
const importSkillCardsPage = () => import("./pages/SkillCards");
const importProjectsPage = () => import("./pages/Projects");
const importSettingsPage = () => import("./pages/Settings");

const MySkillsPage = lazy(() =>
  importMySkillsPage().then((mod) => ({ default: mod.MySkills }))
);
const MarketplacePage = lazy(() =>
  importMarketplacePage().then((mod) => ({ default: mod.Marketplace }))
);
const PublisherDetailPage = lazy(() =>
  importPublisherDetailPage().then((mod) => ({ default: mod.PublisherDetail }))
);
const SkillCardsPage = lazy(() =>
  importSkillCardsPage().then((mod) => ({ default: mod.SkillCards }))
);
const ProjectsPage = lazy(() =>
  importProjectsPage().then((mod) => ({ default: mod.Projects }))
);
const SettingsPage = lazy(() =>
  importSettingsPage().then((mod) => ({ default: mod.Settings }))
);

const DEFAULT_NEXT_PAGES: Record<NavPage, NavPage[]> = {
  "my-skills": ["marketplace", "projects"],
  marketplace: ["my-skills", "skill-cards"],
  "skill-cards": ["projects", "my-skills"],
  projects: ["my-skills", "settings"],
  settings: ["my-skills", "projects"],
};

const ALL_PAGES: NavPage[] = [
  "my-skills",
  "marketplace",
  "skill-cards",
  "projects",
  "settings",
];

function PageFallback() {
  return (
    <div className="flex-1 flex items-center justify-center text-sm text-muted-foreground">
      Loading...
    </div>
  );
}

function App() {
  const [activePage, setActivePage] = useState<NavPage>("my-skills");
  const [subPage, setSubPage] = useState<SubPage>(null);
  const [sidebarCollapsed, setSidebarCollapsed] = useState(() => {
    try { return localStorage.getItem("sidebar-collapsed") === "true"; } catch { return false; }
  });
  const updater = useUpdater();
  const prefetchedPages = useRef<Set<NavPage>>(new Set(["my-skills"]));
  const previousPage = useRef<NavPage>("my-skills");
  const transitionScores = useRef<Record<NavPage, Partial<Record<NavPage, number>>>>({
    "my-skills": {},
    marketplace: {},
    "skill-cards": {},
    projects: {},
    settings: {},
  });

  // Cross-page navigation context
  const [projectsPreSelectedSkills, setProjectsPreSelectedSkills] = useState<string[] | null>(null);
  const [skillCardsPreSelectedSkills, setSkillCardsPreSelectedSkills] = useState<string[] | null>(null);
  const [mySkillsFocusSkill, setMySkillsFocusSkill] = useState<string | null>(null);
  const [marketplaceTab, setMarketplaceTab] = useState<MarketplaceTabId>("all");

  const handleNavigate = useCallback((page: NavPage) => {
    setActivePage(page);
    setSubPage(null);
  }, []);

  const prefetchPage = useCallback((page: NavPage) => {
    if (prefetchedPages.current.has(page)) {
      return;
    }
    prefetchedPages.current.add(page);
    switch (page) {
      case "my-skills":
        void importMySkillsPage();
        break;
      case "marketplace":
        void importMarketplacePage();
        // Prefetch publisher drill-down chunk with marketplace.
        void importPublisherDetailPage();
        break;
      case "skill-cards":
        void importSkillCardsPage();
        break;
      case "projects":
        void importProjectsPage();
        break;
      case "settings":
        void importSettingsPage();
        break;
    }
  }, []);

  const getLikelyNextPages = useCallback((from: NavPage): NavPage[] => {
    const scored = transitionScores.current[from];
    const learned = Object.entries(scored)
      .sort((a, b) => (b[1] ?? 0) - (a[1] ?? 0))
      .map(([page]) => page as NavPage);
    const defaults = DEFAULT_NEXT_PAGES[from];

    const merged: NavPage[] = [];
    for (const page of [...learned, ...defaults, ...ALL_PAGES]) {
      if (page === from || merged.includes(page)) {
        continue;
      }
      merged.push(page);
      if (merged.length >= 2) {
        break;
      }
    }
    return merged;
  }, []);

  useEffect(() => {
    const handleExternalNavigate = (event: Event) => {
      const customEvent = event as CustomEvent<{ page?: NavPage }>;
      const page = customEvent.detail?.page;
      if (!page) return;
      setActivePage(page);
      setSubPage(null);
    };

    window.addEventListener("skillstar:navigate", handleExternalNavigate as EventListener);
    return () => {
      window.removeEventListener("skillstar:navigate", handleExternalNavigate as EventListener);
    };
  }, []);

  useEffect(() => {
    const prev = previousPage.current;
    if (prev !== activePage) {
      const currentScore = transitionScores.current[prev][activePage] ?? 0;
      transitionScores.current[prev][activePage] = currentScore + 1;
      previousPage.current = activePage;
    }

    const timer = window.setTimeout(() => {
      for (const page of getLikelyNextPages(activePage)) {
        prefetchPage(page);
      }
    }, 250);

    return () => window.clearTimeout(timer);
  }, [activePage, getLikelyNextPages, prefetchPage]);

  const renderPage = () => {
    // Sub-page takes priority when active
    if (activePage === "marketplace" && subPage?.type === "publisher-detail") {
      return (
        <motion.div
          key="publisher-detail"
          initial={{ opacity: 0, x: 30 }}
          animate={{ opacity: 1, x: 0 }}
          exit={{ opacity: 0, x: -30 }}
          transition={{ duration: 0.2, ease: "easeOut" }}
          className="flex-1 flex overflow-hidden"
        >
          <PublisherDetailPage
            publisher={subPage.publisher}
            onBack={() => setSubPage(null)}
          />
        </motion.div>
      );
    }

    switch (activePage) {
      case "my-skills":
        return (
          <MySkillsPage
            initialFocusSkill={mySkillsFocusSkill}
            onClearFocus={() => setMySkillsFocusSkill(null)}
            onPackSkills={(skills) => {
              if (skills.length > 0) {
                setSkillCardsPreSelectedSkills(skills);
                handleNavigate("skill-cards");
              }
            }}
          />
        );
      case "marketplace":
        return (
          <MarketplacePage
            activeTab={marketplaceTab}
            onTabChange={setMarketplaceTab}
            onNavigateToPublisher={(pub_) =>
              setSubPage({ type: "publisher-detail", publisher: pub_ })
            }
          />
        );
      case "skill-cards":
        return (
          <SkillCardsPage
            preSelectedSkills={skillCardsPreSelectedSkills}
            onClearPreSelected={() => setSkillCardsPreSelectedSkills(null)}
            onNavigateToProjects={(skills) => {
              if (skills) setProjectsPreSelectedSkills(skills);
              handleNavigate("projects");
            }}
          />
        );
      case "projects":
        return (
          <ProjectsPage
            preSelectedSkills={projectsPreSelectedSkills}
            onClearPreSelected={() => setProjectsPreSelectedSkills(null)}
            onNavigateToSkill={(name) => {
              setMySkillsFocusSkill(name);
              handleNavigate("my-skills");
            }}
          />
        );
      case "settings":
        return <SettingsPage />;
      default:
        return <MySkillsPage />;
    }
  };

  return (
    <div className="flex h-screen w-screen overflow-hidden bg-background border border-border/50">
      <Sidebar
        activePage={activePage}
        onNavigate={handleNavigate}
        onPrefetch={prefetchPage}
        collapsed={sidebarCollapsed}
        onToggleCollapse={() => {
          setSidebarCollapsed((prev) => {
            const next = !prev;
            try { localStorage.setItem("sidebar-collapsed", String(next)); } catch {}
            return next;
          });
        }}
        updateStatus={updater.state.status}
        updateVersion={updater.state.version}
        updateProgress={updater.state.progress}
        updateError={updater.state.error}
        onUpdate={updater.download}
        onRestart={updater.apply}
        onSkip={updater.skip}
        onDismiss={updater.dismiss}
      />
      <AnimatePresence mode="wait">
        <motion.div
          key={activePage}
          initial={{ opacity: 0 }}
          animate={{ opacity: 1 }}
          exit={{ opacity: 0 }}
          transition={{ duration: 0.15 }}
          className="flex-1 flex overflow-hidden"
        >
          <Suspense fallback={<PageFallback />}>{renderPage()}</Suspense>
        </motion.div>
      </AnimatePresence>
    </div>
  );
}

export default App;
