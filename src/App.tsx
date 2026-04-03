import React, { lazy, Suspense, useState, useCallback, useEffect, useRef } from "react";
import { LoadingLogo } from "./components/ui/LoadingLogo";
import { AnimatePresence, motion, useReducedMotion } from "framer-motion";
import { Sidebar } from "./components/layout/Sidebar";
import { CommandPalette } from "./components/layout/CommandPalette";
import { useUpdater } from "./hooks/useUpdater";
import { useNavigation } from "./hooks/useNavigation";
import { useKeyboardShortcuts } from "./hooks/useKeyboardShortcuts";
import { useTauriSetup } from "./hooks/useTauriSetup";
import { looksLikeShareCode } from "./lib/shareCode";
import { Toaster } from "./components/ui/sonner";

const MySkillsPage = lazy(() => import("./pages/MySkills").then((mod) => ({ default: mod.MySkills })));
const MarketplacePage = lazy(() => import("./pages/Marketplace").then((mod) => ({ default: mod.Marketplace })));
const PublisherDetailPage = lazy(() => import("./pages/PublisherDetail").then((mod) => ({ default: mod.PublisherDetail })));
const SkillCardsPage = lazy(() => import("./pages/SkillCards").then((mod) => ({ default: mod.SkillCards })));
const ProjectsPage = lazy(() => import("./pages/Projects").then((mod) => ({ default: mod.Projects })));
const SecurityScanPage = lazy(() => import("./pages/SecurityScan").then((mod) => ({ default: mod.SecurityScan })));
const SettingsPage = lazy(() => import("./pages/Settings").then((mod) => ({ default: mod.Settings })));

function PageFallback() {
  return (
    <div className="flex-1 flex items-center justify-center">
      <LoadingLogo size="md" />
    </div>
  );
}

function AppContent() {
  const nav = useNavigation();
  const prefersReducedMotion = useReducedMotion();
  const updater = useUpdater();
  const lastClipboardValue = useRef("");
  const [commandPaletteOpen, setCommandPaletteOpen] = useState(false);

  const toggleCommandPalette = useCallback(() => {
    setCommandPaletteOpen((prev) => !prev);
  }, []);

  // ── Keyboard shortcuts ────────────────────────────────────────────
  useKeyboardShortcuts({
    onNavigate: nav.navigate,
    onToggleCommandPalette: toggleCommandPalette,
  });

  // ── Sidebar collapsed ──────────────────────────────────────────
  const [sidebarCollapsed, setSidebarCollapsed] = React.useState(() => {
    try { return localStorage.getItem("sidebar-collapsed") === "true"; } catch { return false; }
  });

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
            onDismiss: () => { dismissed = false; },
          });
        }
      } catch { /* Clipboard read permission denied */ }
    };
    window.addEventListener("focus", handleFocus);
    return () => window.removeEventListener("focus", handleFocus);
  }, [nav]);

  const renderPage = () => {
    if (nav.activePage === "marketplace" && nav.subPage?.type === "publisher-detail") {
      return (
        <motion.div key="publisher-detail" initial={{ opacity: 0, x: 30 }} animate={{ opacity: 1, x: 0 }} exit={{ opacity: 0, x: -30 }} transition={{ duration: 0.2, ease: "easeOut" }} className="flex-1 min-w-0 flex overflow-hidden">
          <PublisherDetailPage publisher={nav.subPage.publisher} onBack={() => nav.setSubPage(null)} />
        </motion.div>
      );
    }

    switch (nav.activePage) {
      case "my-skills":
        return (
          <MySkillsPage
            initialFocusSkill={nav.mySkillsFocusSkill}
            onClearFocus={() => nav.setMySkillsFocusSkill(null)}
            onPackSkills={(skills) => { if (skills.length > 0) nav.goToSkillCardsWithSkills(skills); }}
            initialShareCode={nav.clipboardShareCode ?? undefined}
            onClearShareCode={() => nav.setClipboardShareCode(null)}
          />
        );
      case "marketplace":
        return (
          <MarketplacePage
            activeTab={nav.marketplaceTab}
            onTabChange={nav.setMarketplaceTab}
            onNavigateToPublisher={(pub_) => nav.setSubPage({ type: "publisher-detail", publisher: pub_ })}
          />
        );
      case "skill-cards":
        return (
          <SkillCardsPage
            preSelectedSkills={nav.skillCardsPreSelectedSkills}
            onClearPreSelected={() => nav.setSkillCardsPreSelectedSkills(null)}
            onNavigateToProjects={(skills) => { if (skills) nav.goToProjectsWithSkills(skills); }}
          />
        );
      case "projects":
        return (
          <ProjectsPage
            preSelectedSkills={nav.projectsPreSelectedSkills}
            onClearPreSelected={() => nav.setProjectsPreSelectedSkills(null)}
          />
        );
      case "settings":
        return <SettingsPage onCheckUpdate={updater.check} isCheckingUpdate={updater.state.status === "checking"} />;
      case "security-scan":
        return <SecurityScanPage />;
      default:
        return <MySkillsPage />;
    }
  };

  return (
    <div className="flex h-screen w-screen overflow-hidden bg-background border border-border/50">
      <a href="#main-content" className="sr-only focus:not-sr-only focus:absolute focus:top-2 focus:left-2 focus:z-[200] focus:px-4 focus:py-2 focus:bg-primary focus:text-primary-foreground focus:rounded-lg focus:text-sm">
        Skip to content
      </a>
      <Sidebar
        activePage={nav.activePage}
        onNavigate={nav.navigate}
        onPrefetch={() => {}}
        collapsed={sidebarCollapsed}
        onToggleCollapse={() => {
          setSidebarCollapsed((prev) => {
            const next = !prev;
            try { localStorage.setItem("sidebar-collapsed", String(next)); } catch (e) {
              console.warn("[App] Failed to persist sidebar collapsed state:", e);
            }
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
        onRetry={updater.retry}
      />
      <AnimatePresence mode="wait">
        <motion.div
          id="main-content"
          key={nav.activePage}
          initial={{ opacity: 0, y: 6 }}
          animate={{ opacity: 1, y: 0 }}
          exit={{ opacity: 0, y: -4 }}
          transition={{ duration: prefersReducedMotion ? 0.01 : 0.2, ease: [0.22, 1, 0.36, 1] }}
          className="flex-1 min-h-0 min-w-0 flex overflow-hidden"
        >
          <Suspense fallback={<PageFallback />}>{renderPage()}</Suspense>
        </motion.div>
      </AnimatePresence>
      <CommandPalette
        open={commandPaletteOpen}
        onClose={() => setCommandPaletteOpen(false)}
        onNavigate={(page) => {
          nav.navigate(page);
          setCommandPaletteOpen(false);
        }}
      />
      <Toaster />
    </div>
  );
}

export default function App() {
  return <AppContent />;
}
