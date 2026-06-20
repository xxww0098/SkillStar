import { AnimatePresence, motion, useReducedMotion } from "framer-motion";
import React, { lazy, Suspense, useCallback, useEffect, useMemo, useRef, useState } from "react";
import { CommandPalette } from "./components/layout/CommandPalette";
import { Sidebar } from "./components/layout/Sidebar";
import { LoadingLogo } from "./components/ui/LoadingLogo";
import { Toaster } from "./components/ui/sonner";
import { UsageDataProvider } from "./features/usage/context/UsageDataContext";
import { useSkills } from "./features/my-skills/hooks/useSkills";
import { useKeyboardShortcuts } from "./hooks/useKeyboardShortcuts";
import { useNavigation } from "./hooks/useNavigation";
import { useTauriSetup } from "./hooks/useTauriSetup";
import { useUpdater } from "./hooks/useUpdater";
import { looksLikeShareCode } from "./lib/shareCode";
import type { McpPublisherSummary, NavPage, OfficialPublisher } from "./types";

const MySkillsPage = lazy(() => import("./pages/MySkills").then((mod) => ({ default: mod.MySkills })));
const MarketplacePage = lazy(() => import("./pages/Marketplace").then((mod) => ({ default: mod.Marketplace })));
const PublisherDetailPage = lazy(() =>
  import("./pages/PublisherDetail").then((mod) => ({ default: mod.PublisherDetail })),
);
const McpPublisherDetailPage = lazy(() =>
  import("./pages/McpPublisherDetail").then((mod) => ({ default: mod.McpPublisherDetail })),
);
const SkillCardsPage = lazy(() => import("./pages/SkillCards").then((mod) => ({ default: mod.SkillCards })));
const ProjectsPage = lazy(() => import("./pages/Projects").then((mod) => ({ default: mod.Projects })));
const McpPage = lazy(() => import("./pages/Mcp").then((mod) => ({ default: mod.Mcp })));
const SettingsPage = lazy(() => import("./pages/Settings").then((mod) => ({ default: mod.Settings })));

// Models mode (single hub page that merges agent connections / providers / health / tool configs)
const ModelsPage = lazy(() => import("./pages/Models").then((mod) => ({ default: mod.Models })));

// Usage mode (single page: subscription tracker)
const UsagePage = lazy(() => import("./pages/Usage").then((mod) => ({ default: mod.Usage })));

function PageFallback() {
  return (
    <div className="flex-1 flex items-center justify-center">
      <LoadingLogo size="md" />
    </div>
  );
}

/** Match `Sidebar` fixed inset (`left-2` = 8px) + aside width (`w-[180px]` / `w-14`). */
const MAIN_CONTENT_PAD_EXPANDED_PX = 8 + 180;
/** `w-14` is 3.5rem; default 16px root → 56px. */
const MAIN_CONTENT_PAD_COLLAPSED_PX = 8 + 56;

const noop = () => {};

function UsageModeShell({ children }: { children: React.ReactNode }) {
  const { appMode } = useNavigation();
  if (appMode === "usage") {
    return <UsageDataProvider>{children}</UsageDataProvider>;
  }
  return children;
}

function AppContent() {
  const nav = useNavigation();
  const prefersReducedMotion = useReducedMotion();
  const updater = useUpdater();
  const { ghostSkills, skills } = useSkills();
  const pendingUpdatesCount = useMemo(() => skills.filter((s) => s.update_available).length, [skills]);
  const lastClipboardValue = useRef("");
  const [commandPaletteOpen, setCommandPaletteOpen] = useState(false);

  const toggleCommandPalette = useCallback(() => {
    setCommandPaletteOpen((prev) => !prev);
  }, []);

  // ── Keyboard shortcuts ────────────────────────────────────────────
  useKeyboardShortcuts({
    onNavigate: nav.navigate,
    onSetAppMode: nav.setAppMode,
    onToggleCommandPalette: toggleCommandPalette,
  });

  // ── Sidebar collapsed ──────────────────────────────────────────
  const [sidebarCollapsed, setSidebarCollapsed] = React.useState(() => {
    try {
      return localStorage.getItem("sidebar-collapsed") === "true";
    } catch {
      return false;
    }
  });

  const handleToggleCollapse = useCallback(() => {
    setSidebarCollapsed((prev) => {
      const next = !prev;
      try {
        localStorage.setItem("sidebar-collapsed", String(next));
      } catch (e) {
        if (import.meta.env.DEV) console.warn("[App] Failed to persist sidebar collapsed state:", e);
      }
      return next;
    });
  }, []);

  const handleCloseCommandPalette = useCallback(() => {
    setCommandPaletteOpen(false);
  }, []);

  const handleCommandPaletteNavigate = useCallback(
    (page: NavPage) => {
      nav.navigate(page);
      setCommandPaletteOpen(false);
    },
    [nav],
  );

  const handleEnterModelsMode = useCallback(() => {
    nav.setAppMode("models");
    setCommandPaletteOpen(false);
  }, [nav]);

  const handleEnterUsageMode = useCallback(() => {
    nav.setAppMode("usage");
    setCommandPaletteOpen(false);
  }, [nav]);

  // ── Tauri lifecycle (patrol, tray, window-hidden) ──────────────
  useTauriSetup();

  // ── Clipboard share-code detection ─────────────────────────────
  useEffect(() => {
    let dismissed = false;
    const handleFocus = async () => {
      try {
        if (localStorage.getItem("skillstar-clipboard-consent") !== "true") return;
        const text = await navigator.clipboard.readText();
        if (!text || text === lastClipboardValue.current || dismissed) return;
        const codeType = looksLikeShareCode(text.trim());
        if (codeType) {
          lastClipboardValue.current = text;
          dismissed = true;
          const { toast } = await import("sonner");
          toast.info("Share code detected in clipboard", {
            description: "Click to import it into My Skills",
            duration: 8000,
            action: {
              label: "Import",
              onClick: () => {
                nav.setClipboardShareCode(text.trim());
                nav.navigate("my-skills");
                dismissed = false;
              },
            },
            onDismiss: () => {
              dismissed = false;
            },
          });
        }
      } catch {
        /* Clipboard read permission denied */
      }
    };
    window.addEventListener("focus", handleFocus);
    return () => window.removeEventListener("focus", handleFocus);
  }, [nav]);

  const handlePublisherBack = useCallback(() => nav.setSubPage(null), [nav]);
  const handleClearFocus = useCallback(() => nav.setMySkillsFocusSkill(null), [nav]);
  const handlePackSkills = useCallback(
    (skills: string[]) => {
      if (skills.length > 0) nav.goToSkillCardsWithSkills(skills);
    },
    [nav],
  );
  const handleClearShareCode = useCallback(() => nav.setClipboardShareCode(null), [nav]);
  const handleNavigateToPublisher = useCallback(
    (pub_: OfficialPublisher) => nav.setSubPage({ type: "publisher-detail", publisher: pub_ }),
    [nav],
  );
  const handleNavigateToMcpPublisher = useCallback(
    (pub_: McpPublisherSummary) => nav.setSubPage({ type: "mcp-publisher-detail", publisher: pub_ }),
    [nav],
  );
  const handleClearPreSelectedCards = useCallback(() => nav.setSkillCardsPreSelectedSkills(null), [nav]);
  const handleNavigateToProjects = useCallback(
    (skills?: string[]) => {
      if (skills) nav.goToProjectsWithSkills(skills);
    },
    [nav],
  );
  const handleClearPreSelectedProjects = useCallback(() => nav.setProjectsPreSelectedSkills(null), [nav]);

  const renderPage = () => {
    if (nav.appMode === "usage") {
      return (
        <UsagePage
          filter={nav.usageCatalogFilter}
          usageCreateRequest={nav.usageCreateRequest}
          clearUsageCreateRequest={nav.clearUsageCreateRequest}
        />
      );
    }

    // Models mode (single hub page)
    if (nav.appMode === "models") {
      return <ModelsPage />;
    }

    // Skills mode pages
    if (nav.activePage === "marketplace" && nav.subPage?.type === "publisher-detail") {
      return (
        <motion.div
          key="publisher-detail"
          initial={{ opacity: 0, x: 30 }}
          animate={{ opacity: 1, x: 0 }}
          exit={{ opacity: 0, x: -30 }}
          transition={{ duration: 0.2, ease: "easeOut" }}
          className="flex-1 min-w-0 flex overflow-hidden"
        >
          <PublisherDetailPage publisher={nav.subPage.publisher} onBack={handlePublisherBack} />
        </motion.div>
      );
    }
    if (nav.activePage === "marketplace" && nav.subPage?.type === "mcp-publisher-detail") {
      return (
        <motion.div
          key="mcp-publisher-detail"
          initial={{ opacity: 0, x: 30 }}
          animate={{ opacity: 1, x: 0 }}
          exit={{ opacity: 0, x: -30 }}
          transition={{ duration: 0.2, ease: "easeOut" }}
          className="flex-1 min-w-0 flex overflow-hidden"
        >
          <McpPublisherDetailPage publisher={nav.subPage.publisher} onBack={handlePublisherBack} />
        </motion.div>
      );
    }

    switch (nav.activePage) {
      case "my-skills":
        return (
          <MySkillsPage
            initialFocusSkill={nav.mySkillsFocusSkill}
            onClearFocus={handleClearFocus}
            onPackSkills={handlePackSkills}
            initialShareCode={nav.clipboardShareCode ?? undefined}
            onClearShareCode={handleClearShareCode}
          />
        );
      case "marketplace":
        return (
          <MarketplacePage
            activeTab={nav.marketplaceTab}
            onTabChange={nav.setMarketplaceTab}
            onNavigateToPublisher={handleNavigateToPublisher}
            onNavigateToMcpPublisher={handleNavigateToMcpPublisher}
          />
        );
      case "skill-cards":
        return (
          <SkillCardsPage
            preSelectedSkills={nav.skillCardsPreSelectedSkills}
            onClearPreSelected={handleClearPreSelectedCards}
            onNavigateToProjects={handleNavigateToProjects}
          />
        );
      case "projects":
        return (
          <ProjectsPage
            preSelectedSkills={nav.projectsPreSelectedSkills}
            onClearPreSelected={handleClearPreSelectedProjects}
          />
        );
      case "mcp":
        return (
          <McpPage
            onOpenMarket={() => {
              nav.setMarketplaceTab("mcp-official");
              nav.navigate("marketplace");
            }}
          />
        );
      case "settings":
        return <SettingsPage onCheckUpdate={updater.check} isCheckingUpdate={updater.state.status === "checking"} />;
      default:
        return <MySkillsPage />;
    }
  };

  const mainContentStyle = useMemo(
    () => ({ paddingLeft: sidebarCollapsed ? MAIN_CONTENT_PAD_COLLAPSED_PX : MAIN_CONTENT_PAD_EXPANDED_PX }),
    [sidebarCollapsed],
  );

  return (
    <UsageModeShell>
      <div className="relative h-screen w-screen overflow-hidden bg-background border border-border/50">
        <a
          href="#main-content"
          className="sr-only focus:not-sr-only focus:absolute focus:top-2 focus:left-2 focus:z-[200] focus:px-4 focus:py-2 focus:bg-primary focus:text-primary-foreground focus:rounded-lg focus:text-sm"
        >
          Skip to content
        </a>
        <Sidebar
          activePage={nav.activePage}
          onNavigate={nav.navigate}
          onPrefetch={noop}
          collapsed={sidebarCollapsed}
          onToggleCollapse={handleToggleCollapse}
          updateStatus={updater.state.status}
          updateVersion={updater.state.version}
          updateProgress={updater.state.progress}
          updateError={updater.state.error}
          onUpdate={updater.download}
          onRestart={updater.apply}
          onSkip={updater.skip}
          onDismiss={updater.dismiss}
          onRetry={updater.retry}
          ghostSkillCount={ghostSkills.length}
          pendingUpdatesCount={pendingUpdatesCount}
        />
        <div
          id="main-content"
          className="h-full w-full flex flex-col overflow-hidden pt-0 transition-[padding-left] duration-200 ease-out will-change-[padding-left]"
          style={mainContentStyle}
        >
          <div className="ss-main-chrome">
            <AnimatePresence mode="wait">
              <motion.div
                key={nav.appMode === "models" ? "models" : nav.appMode === "usage" ? "usage" : nav.activePage}
                initial={{ opacity: 0, y: 6 }}
                animate={{ opacity: 1, y: 0 }}
                exit={{ opacity: 0, y: -4 }}
                transition={{ duration: prefersReducedMotion ? 0.01 : 0.2, ease: [0.22, 1, 0.36, 1] }}
                className="flex min-h-0 min-w-0 flex-1 flex-col overflow-hidden"
              >
                <Suspense fallback={<PageFallback />}>{renderPage()}</Suspense>
              </motion.div>
            </AnimatePresence>
          </div>
        </div>
        <CommandPalette
          open={commandPaletteOpen}
          onClose={handleCloseCommandPalette}
          onNavigate={handleCommandPaletteNavigate}
          onEnterModelsMode={handleEnterModelsMode}
          onEnterUsageMode={handleEnterUsageMode}
        />
        <Toaster />
      </div>
    </UsageModeShell>
  );
}

export default function App() {
  return <AppContent />;
}
