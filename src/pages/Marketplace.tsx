import { useState, useEffect, useMemo, useCallback, useRef, lazy, Suspense } from "react";
import { motion, AnimatePresence } from "framer-motion";
import { useTranslation } from "react-i18next";
import { Toolbar } from "../components/layout/Toolbar";
import { SkillGrid } from "../components/skills/SkillGrid";
import { OfficialPublishers } from "../components/marketplace/OfficialPublishers";
import { ImportModal } from "../components/skills/ImportModal";
import { useMarketplace } from "../hooks/useMarketplace";
import { useSkills } from "../hooks/useSkills";
import { ArrowUp } from "lucide-react";
import {
  MARKETPLACE_DESCRIPTION_BATCH_SIZE,
  applyMarketplaceDescriptionPatchToSkill,
  hydrateDescriptionsForSkills,
} from "../lib/marketplaceDescriptionHydration";
import { toast } from "../lib/toast";
import { LoadingLogo } from "../components/ui/LoadingLogo";
import type { Skill, SortOption, ViewMode, OfficialPublisher } from "../types";
import { cn } from "../lib/utils";

const DetailPanel = lazy(() =>
  import("../components/layout/DetailPanel").then((mod) => ({
    default: mod.DetailPanel,
  }))
);

export type TabId = "all" | "trending" | "hot" | "official";

const tabIds: TabId[] = ["all", "trending", "hot", "official"];

const tabLabelKeys: Record<TabId, string> = {
  all: "marketplace.allTime",
  trending: "marketplace.trending",
  hot: "marketplace.hot",
  official: "marketplace.official",
};

interface MarketplaceProps {
  onNavigateToPublisher?: (publisher: OfficialPublisher) => void;
  activeTab?: TabId;
  onTabChange?: (tab: TabId) => void;
}

const INITIAL_MARKETPLACE_VISIBLE_COUNT = 30;
const EAGER_RENDER_THRESHOLD = INITIAL_MARKETPLACE_VISIBLE_COUNT * 2;

export function Marketplace({ onNavigateToPublisher, activeTab: controlledTab, onTabChange }: MarketplaceProps) {
  const { t } = useTranslation();
  const {
    results,
    leaderboard,
    publishers,
    loading,
    search,
    fetchLeaderboard,
    fetchOfficialPublishers,
    applyDescriptionPatches,
  } = useMarketplace();
  const {
    skills: hubSkills,
    refresh,
    updateSkill,
    uninstallSkill,
    pendingUpdateNames,
  } = useSkills();
  const [searchQuery, setSearchQuery] = useState("");
  const [sortBy, setSortBy] = useState<SortOption>("stars-desc");
  const [viewMode, setViewMode] = useState<ViewMode>("grid");
  const [internalTab, setInternalTab] = useState<TabId>("all");
  const activeTab = controlledTab ?? internalTab;
  const setActiveTab = (tab: TabId) => {
    onTabChange?.(tab);
    setInternalTab(tab);
  };
  const [selectedSkill, setSelectedSkill] = useState<Skill | null>(null);
  const [installStatus, setInstallStatus] = useState<string | null>(null);
  const [showBackToTop, setShowBackToTop] = useState(false);
  const [visibleSkillCount, setVisibleSkillCount] = useState(0);
  const scrollRef = useRef<HTMLDivElement>(null);
  /** Skills already installed in this session (to mark cards) */
  const [installedNames, setInstalledNames] = useState<Set<string>>(new Set());
  /** GitHub Import modal state */
  const [importModalOpen, setImportModalOpen] = useState(false);
  const [importUrl, setImportUrl] = useState("");
  const [importSkillName, setImportSkillName] = useState<string | undefined>();
  const installedSkillNames = useMemo(
    () => new Set(hubSkills.map((skill) => skill.name)),
    [hubSkills]
  );

  // Tab change
  useEffect(() => {
    if (activeTab === "official") {
      fetchOfficialPublishers();
    } else {
      fetchLeaderboard(activeTab === "all" ? "all" : activeTab);
    }
  }, [activeTab, fetchOfficialPublishers, fetchLeaderboard]);

  // Search (debounced)
  useEffect(() => {
    if (!searchQuery.trim()) return;
    const timer = setTimeout(() => {
      search(searchQuery);
    }, 400);
    return () => clearTimeout(timer);
  }, [searchQuery, search]);

  const displaySkills = useMemo(() => {
    let skills: Skill[] = [];
    const isSearchMode = Boolean(searchQuery.trim() && results);

    // Search results override
    if (isSearchMode && results) {
      skills = [...results.skills];
    } else if (activeTab !== "official") {
      skills = [...leaderboard];
    }

    // Search results can be freely re-sorted, but leaderboard tabs should
    // preserve the upstream ranking unless the user explicitly picks a
    // different sort mode.
    if (sortBy === "name") {
      skills.sort((a, b) => a.name.localeCompare(b.name));
    } else if (sortBy === "updated") {
      skills.sort((a, b) => b.last_updated.localeCompare(a.last_updated));
    } else if (isSearchMode) {
      // "stars-desc" keeps search results sorted by install count.
      skills.sort((a, b) => b.stars - a.stars);
    }

    // Produce new objects instead of mutating in-place
    return skills.map((s, i) => {
      const rank =
        sortBy === "stars-desc"
          ? isSearchMode
            ? i + 1
            : s.rank ?? i + 1
          : s.rank;
      const installed =
        s.installed || installedNames.has(s.name) || installedSkillNames.has(s.name);
      if (rank === s.rank && installed === s.installed) return s;
      return { ...s, rank, installed };
    });
  }, [
    activeTab,
    results,
    leaderboard,
    sortBy,
    searchQuery,
    installedNames,
    installedSkillNames,
  ]);

  useEffect(() => {
    if (activeTab === "official" || displaySkills.length === 0) return;

    let cancelled = false;

    (async () => {
      const patches = await hydrateDescriptionsForSkills(
        displaySkills,
        MARKETPLACE_DESCRIPTION_BATCH_SIZE
      );

      if (cancelled || patches.length === 0) return;

      applyDescriptionPatches(patches);
      setSelectedSkill((prev) =>
        applyMarketplaceDescriptionPatchToSkill(prev, patches)
      );
    })();

    return () => {
      cancelled = true;
    };
  }, [activeTab, displaySkills, applyDescriptionPatches]);

  const handleInstall = useCallback(async (url: string, name?: string) => {
    // Route through ImportModal for full scan + select flow
    setImportUrl(url);
    setImportSkillName(name);
    setImportModalOpen(true);
  }, []);

  const handleUpdate = useCallback(async (name: string) => {
    try {
      await updateSkill(name);
    } catch (e) {
      console.error("Update failed:", e);
      const reason = String(e);
      toast.error(reason ? `${t("marketplace.updateFailed")}: ${reason}` : t("marketplace.updateFailed"));
    }
  }, [t, updateSkill]);

  const handleUninstall = useCallback(async (name: string) => {
    try {
      await uninstallSkill(name);
      setInstalledNames(prev => {
        const next = new Set(prev);
        next.delete(name);
        // We'd ideally need the url to remove it here, but removing it by name isn't straightforward without tracking
        // Instead, the next re-fetch will clear the 'installed' flag from the backend result if we implemented backend tracking
        return next;
      });
      if (selectedSkill?.name === name) {
        setSelectedSkill(prev => prev ? { ...prev, installed: false } : null);
      }
    } catch (e) {
      console.error("[Marketplace] Uninstall failed:", e);
      toast.error(t("marketplace.uninstallFailed"));
    }
  }, [uninstallSkill, selectedSkill]);

  const handleReinstall = useCallback(async (url: string, name: string) => {
    try {
      // First uninstall
      await uninstallSkill(name);
      // Then re-install
      await handleInstall(url, name);
    } catch (e) {
      console.error("[Marketplace] Reinstall failed:", e);
      toast.error(t("marketplace.reinstallFailed"));
    }
  }, [uninstallSkill, handleInstall]);

  const totalCount = searchQuery.trim()
    ? results?.total_count ?? 0
    : leaderboard.length;

  useEffect(() => {
    if (activeTab === "official") {
      setVisibleSkillCount(0);
      return;
    }
    setVisibleSkillCount(
      displaySkills.length <= EAGER_RENDER_THRESHOLD
        ? displaySkills.length
        : INITIAL_MARKETPLACE_VISIBLE_COUNT
    );
  }, [activeTab, displaySkills.length]);

  return (
    <div className="flex-1 flex overflow-hidden relative">
      <div className="flex-1 flex flex-col overflow-hidden">
        <Toolbar
          titleNode={<h1>{t("sidebar.market")}</h1>}
          searchQuery={searchQuery}
          onSearchChange={setSearchQuery}
          sortBy={sortBy}
          onSortChange={setSortBy}
          viewMode={viewMode}
          onViewModeChange={setViewMode}
        />

        {/* Category tabs */}
        <div className="flex items-center gap-1 px-6 py-2 border-b border-white/10 bg-card/30 backdrop-blur-sm" role="tablist">
          {tabIds.map((id) => (
            <button
              key={id}
              role="tab"
              aria-selected={activeTab === id}
              id={`tab-${id}`}
              aria-controls={`tabpanel-${id}`}
              onClick={() => {
                setActiveTab(id);
                setSearchQuery("");
              }}
              className={cn(
                "px-3 py-1.5 rounded-md text-xs font-medium transition-colors cursor-pointer",
                activeTab === id
                  ? "bg-primary/20 text-primary shadow-sm"
                  : "text-muted-foreground hover:text-foreground hover:bg-sidebar-hover"
              )}
            >
              {t(tabLabelKeys[id])}
            </button>
          ))}
          <div className="ml-auto flex items-center gap-2">
            {/* Install toast */}
            {installStatus && (
              <motion.span
                initial={{ opacity: 0, x: 10 }}
                animate={{ opacity: 1, x: 0 }}
                exit={{ opacity: 0 }}
                className="text-xs text-success font-medium"
              >
                {installStatus}
              </motion.span>
            )}
            {activeTab !== "official" && (
              <span className="text-caption">
                {totalCount > 0
                  ? t("marketplace.skillsCount", { count: visibleSkillCount })
                  : ""}
              </span>
            )}
          </div>
        </div>

        <motion.main
          ref={scrollRef}
          initial={{ opacity: 0 }}
          animate={{ opacity: 1 }}
          transition={{ duration: 0.2 }}
          className="flex-1 overflow-y-auto p-6 bg-gradient-to-br from-transparent via-card/10 to-transparent"
          onScroll={(e) => {
            const target = e.currentTarget;
            setShowBackToTop(target.scrollTop > 300);
          }}
        >
          {activeTab === "official" ? (
            <OfficialPublishers
              publishers={publishers}
              onPublisherClick={onNavigateToPublisher}
            />
          ) : loading ? (
            <div className="flex items-center justify-center py-20">
              <LoadingLogo size="lg" label={t("marketplace.loading")} />
            </div>
          ) : (
            <SkillGrid
              skills={displaySkills}
              viewMode={viewMode}
              onVisibleCountChange={(visible) => setVisibleSkillCount(visible)}
              onSkillClick={(skill) => setSelectedSkill(prev => prev?.name === skill.name ? null : skill)}
              onInstall={handleInstall}
              onUpdate={handleUpdate}
              pendingUpdateNames={pendingUpdateNames}
              emptyMessage={
                searchQuery.trim()
                  ? t("marketplace.noResultsSearch")
                  : t("marketplace.noResults")
              }
            />
          )}
        </motion.main>

        {/* Back to top button */}
        <AnimatePresence>
          {showBackToTop && (
            <motion.button
              initial={{ opacity: 0, scale: 0.8 }}
              animate={{ opacity: 1, scale: 1 }}
              exit={{ opacity: 0, scale: 0.8 }}
              transition={{ duration: 0.15 }}
              onClick={() => scrollRef.current?.scrollTo({ top: 0, behavior: "smooth" })}
              className="absolute bottom-8 right-8 z-40 w-10 h-10 rounded-full bg-background/80 hover:bg-background border border-border/50 text-foreground/80 hover:text-foreground shadow-sm hover:shadow-md backdrop-blur-md flex items-center justify-center transition duration-200 cursor-pointer group"
              title={t("marketplace.backToTop")}
            >
              <ArrowUp className="w-4 h-4 transition-transform duration-200 group-hover:-translate-y-0.5" />
            </motion.button>
          )}
        </AnimatePresence>
      </div>

      {selectedSkill && (
        <Suspense
          fallback={
            <div className="absolute right-0 top-0 bottom-0 w-[400px] h-full border-l border-white/10 bg-card/60 backdrop-blur-xl shadow-2xl overflow-y-auto z-50 rounded-tl-xl rounded-bl-xl flex items-center justify-center">
              <LoadingLogo size="sm" />
            </div>
          }
        >
          <DetailPanel
            skill={selectedSkill}
            onClose={() => setSelectedSkill(null)}
            onInstall={handleInstall}
            onUpdate={handleUpdate}
            onUninstall={handleUninstall}
            onReinstall={handleReinstall}
          />
        </Suspense>
      )}

      <ImportModal
        open={importModalOpen}
        onClose={() => {
          setImportModalOpen(false);
          setImportUrl("");
          setImportSkillName(undefined);
        }}
        onInstalled={(names) => {
          setInstalledNames((prev) => {
            const next = new Set(prev);
            names.forEach((name) => next.add(name));
            return next;
          });
          void refresh(true, true);
          setInstallStatus(t("marketplace.installedViaGithub"));
          setTimeout(() => setInstallStatus(null), 4000);
        }}
        initialUrl={importUrl}
        autoScan={!!importUrl}
        preSelectedSkill={importSkillName}
      />
    </div>
  );
}
