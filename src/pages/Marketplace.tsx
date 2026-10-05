import { AnimatePresence, motion } from "framer-motion";
import { ArrowUp, Sparkles } from "lucide-react";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { DetailPanel } from "../components/layout/DetailPanel";
import { Toolbar } from "../components/layout/Toolbar";
import { Button } from "../components/ui/button";
import { EmptyState } from "../components/ui/EmptyState";
import { LoadingLogo } from "../components/ui/LoadingLogo";
import { OfficialPublishers } from "../features/marketplace/components/OfficialPublishers";
import { SnapshotEmptyState, SnapshotErrorBanner } from "../features/marketplace/components/SnapshotState";
import { useMarketplace } from "../features/marketplace/hooks/useMarketplace";
import { useMarketplaceActions } from "../features/marketplace/hooks/useMarketplaceActions";
import { computeDisplaySkills } from "../features/marketplace/lib/skillDisplay";
import {
  isUnpopulatedSnapshot,
  type MarketplaceScope,
  snapshotStatusLabelKey,
} from "../features/marketplace/lib/snapshotState";
import { SkillGrid } from "../features/my-skills/components/SkillGrid";
import { useSkills } from "../features/my-skills/hooks/useSkills";
import { useAgentProfiles } from "../hooks/useAgentProfiles";
import { useViewMode } from "../hooks/useViewMode";
import { cn } from "../lib/utils";
import type { OfficialPublisher, Skill, SortOption } from "../types";

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

export function Marketplace({ onNavigateToPublisher, activeTab: controlledTab, onTabChange }: MarketplaceProps) {
  const { t } = useTranslation();
  const {
    results,
    leaderboard,
    publishers,
    loading,
    refreshing,
    snapshots,
    retrySnapshot,
    search,
    searchOnline,
    clearSearch,
    notePendingSearchQuery,
    fetchLeaderboard,
    fetchOfficialPublishers,
    patchSkill,
  } = useMarketplace();
  const {
    skills: installedSkills,
    installSkill,
    updateSkill,
    uninstallSkill,
    pendingUpdateNames,
    toggleSkillForAgent,
    pendingAgentToggleKeys,
  } = useSkills();
  const { profiles } = useAgentProfiles();
  const [searchQuery, setSearchQuery] = useState("");
  const [sortBy, setSortBy] = useState<SortOption>("stars-desc");
  const [viewMode, setViewMode] = useViewMode("grid");
  const [internalTab, setInternalTab] = useState<TabId>("all");
  const activeTab = controlledTab ?? internalTab;
  const setActiveTab = (tab: TabId) => {
    onTabChange?.(tab);
    setInternalTab(tab);
  };
  const handleTabChange = useCallback(
    (tab: TabId) => {
      setActiveTab(tab);
      setSearchQuery("");
      setSelectedSkill(null);
      clearSearch();
    },
    [clearSearch, onTabChange],
  );
  const [selectedSkill, setSelectedSkill] = useState<Skill | null>(null);
  const [installStatus, setInstallStatus] = useState<string | null>(null);
  const [showBackToTop, setShowBackToTop] = useState(false);
  const scrollRef = useRef<HTMLDivElement>(null);
  /** Skills currently being installed (for per-card loading state) */
  const [installingNames, setInstallingNames] = useState<Set<string>>(new Set());

  useEffect(() => {
    setSelectedSkill((current) => {
      if (!current) return null;
      const installed = installedSkills.find((skill) => skill.name === current.name);
      return installed ? { ...current, ...installed } : current;
    });
  }, [installedSkills]);

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

  const displaySkills = useMemo(
    () =>
      computeDisplaySkills({
        results,
        leaderboard,
        sortBy,
        searchQuery,
        activeTab,
      }),
    [activeTab, results, leaderboard, sortBy, searchQuery],
  );

  const spotlightItems = useMemo(
    () =>
      displaySkills.map((skill) => ({
        id: skill.name,
        title: skill.name,
        subtitle: skill.localized_description || skill.description || undefined,
        meta: skill.source,
      })),
    [displaySkills],
  );

  const handleSpotlightSelect = useCallback(
    (id: string) => {
      const skill = displaySkills.find((s) => s.name === id) ?? results?.skills.find((s) => s.name === id);
      if (skill) setSelectedSkill(skill);
    },
    [displaySkills, results],
  );

  // Stable identity so SkillGrid/SkillCard memoization holds across
  // unrelated re-renders (e.g. every search-input keystroke).
  const handleSkillClick = useCallback(
    (skill: Skill) => setSelectedSkill((prev) => (prev?.name === skill.name ? null : skill)),
    [],
  );

  const { handleInstall, handleUpdate, handleUninstall, handleReinstall } = useMarketplaceActions({
    installSkill,
    updateSkill,
    uninstallSkill,
    patchSkill,
    selectedSkill,
    setSelectedSkill,
    setInstallingNames,
    setInstallStatus,
    t,
  });

  const toolbarSearchQuery = searchQuery;
  const handleToolbarSearchChange = useCallback(
    (value: string) => {
      setSearchQuery(value);
      if (!value.trim()) {
        clearSearch();
        return;
      }
      // Typing past a finished query must not leave that query's freshness
      // label / error banner attached to the one being typed.
      notePendingSearchQuery(value);
    },
    [clearSearch, notePendingSearchQuery],
  );

  const totalCount = displaySkills.length;

  // Which local-first dataset the current view actually renders. Snapshot
  // status/error are per-scope, so publishers can no longer describe (or fail
  // on behalf of) the skills tab.
  const snapshotScope: MarketplaceScope =
    activeTab === "official" ? "publishers" : searchQuery.trim() ? "search" : "leaderboard";
  const snapshot = snapshots[snapshotScope];

  // `search` recovers by re-running the online search against the query on
  // screen. Reaching this scope always means there is one, so the retry
  // action is never inert and needs no disabled variant.
  const handleSnapshotRetry = useCallback(() => {
    if (snapshotScope === "search") {
      void searchOnline(searchQuery);
      return;
    }
    void retrySnapshot(snapshotScope);
  }, [retrySnapshot, searchOnline, searchQuery, snapshotScope]);

  const snapshotStatusKey = snapshotStatusLabelKey(snapshot.status);
  const snapshotLabel = refreshing
    ? t("marketplace.refreshingSnapshot", {
        defaultValue: "Refreshing snapshot...",
      })
    : snapshotStatusKey
      ? t(snapshotStatusKey)
      : null;
  const snapshotTitle = snapshot.updatedAt ?? undefined;
  const showOnlineSupplement =
    Boolean(searchQuery.trim()) && !loading && displaySkills.length === 0 && snapshot.status === "miss";
  // A seeding snapshot is a loading state, not an
  // empty market. Only take over the viewport while there is nothing to show.
  const showSeedingLoader = snapshot.status === "seeding" && displaySkills.length === 0;

  const renderTabButton = (id: TabId) => {
    const index = tabIds.indexOf(id);
    const isActive = activeTab === id;

    return (
      <button
        key={id}
        type="button"
        role="tab"
        aria-selected={isActive}
        tabIndex={isActive ? 0 : -1}
        id={`tab-${id}`}
        aria-controls={`tabpanel-${id}`}
        onClick={() => handleTabChange(id)}
        onKeyDown={(e) => {
          let next = index;
          if (e.key === "ArrowRight") next = (index + 1) % tabIds.length;
          else if (e.key === "ArrowLeft") next = (index - 1 + tabIds.length) % tabIds.length;
          else if (e.key === "Home") next = 0;
          else if (e.key === "End") next = tabIds.length - 1;
          else return;
          e.preventDefault();
          const nextId = tabIds[next];
          handleTabChange(nextId);
          document.getElementById(`tab-${nextId}`)?.focus();
        }}
        className={cn(
          "inline-flex h-7 items-center rounded-full px-2.5 text-xs transition-all duration-150 cursor-pointer focus-ring select-none",
          isActive
            ? "bg-primary text-primary-foreground font-semibold shadow-xs ring-1 ring-inset ring-primary/40 dark:bg-primary dark:text-primary-foreground"
            : "text-muted-foreground font-medium hover:text-foreground hover:bg-sidebar-hover/80",
        )}
      >
        {t(tabLabelKeys[id])}
      </button>
    );
  };

  return (
    <div className="flex-1 min-w-0 flex overflow-hidden relative">
      <div className="flex-1 min-w-0 flex flex-col overflow-hidden">
        <Toolbar
          titleNode={<h1 className="text-sm font-bold tracking-tight text-foreground">{t("sidebar.market")}</h1>}
          searchQuery={toolbarSearchQuery}
          onSearchChange={handleToolbarSearchChange}
          searchItems={spotlightItems}
          onSearchSelect={handleSpotlightSelect}
          sortBy={sortBy}
          onSortChange={setSortBy}
          viewMode={viewMode}
          onViewModeChange={setViewMode}
          filtersLead={
            <div className="flex min-w-0 items-center gap-2 shrink-0" role="tablist" aria-label={t("sidebar.market")}>
              <div
                className="flex min-w-max items-center gap-0.5 rounded-full border border-border/80 bg-background/50 p-0.5 h-8 shadow-2xs"
                role="presentation"
              >
                {tabIds.map((id) => renderTabButton(id))}
              </div>
            </div>
          }
          countText={
            activeTab !== "official" ? <span>{t("marketplace.skillsCount", { count: totalCount })}</span> : null
          }
          actionsLead={
            (installStatus || snapshotLabel) && (
              <div className="flex items-center gap-2.5 shrink-0 px-1" aria-live="polite">
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
                {snapshotLabel && (
                  <span className="hidden text-[11px] text-muted-foreground sm:inline" title={snapshotTitle}>
                    {snapshotLabel}
                  </span>
                )}
              </div>
            )
          }
        />

        {snapshot.error && (
          <SnapshotErrorBanner error={snapshot.error} refreshing={refreshing} onRetry={handleSnapshotRetry} />
        )}

        <motion.main
          ref={scrollRef}
          role="tabpanel"
          id={`tabpanel-${activeTab}`}
          aria-labelledby={`tab-${activeTab}`}
          initial={{ opacity: 0 }}
          animate={{ opacity: 1 }}
          transition={{ duration: 0.2 }}
          className="ss-page-scroll"
          onScroll={(e) => {
            const target = e.currentTarget;
            setShowBackToTop(target.scrollTop > 300);
          }}
        >
          {activeTab === "official" ? (
            publishers.length === 0 && isUnpopulatedSnapshot(snapshot.status) ? (
              <SnapshotEmptyState status={snapshot.status} refreshing={refreshing} onRetry={handleSnapshotRetry} />
            ) : (
              <OfficialPublishers
                publishers={publishers}
                viewMode={viewMode}
                onPublisherClick={onNavigateToPublisher}
              />
            )
          ) : loading || showSeedingLoader ? (
            <div className="flex flex-col items-center justify-center py-20 gap-4">
              <LoadingLogo
                size="lg"
                label={showSeedingLoader && !loading ? t("marketplace.seedingSnapshot") : t("marketplace.loading")}
              />
            </div>
          ) : showOnlineSupplement ? (
            <EmptyState
              icon={<Sparkles className="w-6 h-6 text-muted-foreground" />}
              title={t("marketplace.noResultsSearch")}
              description={t("marketplace.searchRemoteHint", {
                defaultValue: "No local matches yet. You can run one remote search and seed the snapshot.",
              })}
              action={
                <Button
                  variant="outline"
                  size="sm"
                  onClick={() => void searchOnline(searchQuery)}
                  disabled={refreshing}
                >
                  {refreshing
                    ? t("marketplace.refreshingSnapshot", {
                        defaultValue: "Refreshing snapshot...",
                      })
                    : t("marketplace.searchOnlineSupplement", {
                        defaultValue: "Search online and save locally",
                      })}
                </Button>
              }
              size="lg"
            />
          ) : displaySkills.length === 0 && isUnpopulatedSnapshot(snapshot.status) ? (
            // `miss` / `remote_error` with nothing to show means the snapshot never
            // landed — not "the marketplace is empty". Say so, and offer the action.
            <SnapshotEmptyState status={snapshot.status} refreshing={refreshing} onRetry={handleSnapshotRetry} />
          ) : (
            <SkillGrid
              skills={displaySkills}
              viewMode={viewMode}
              columnStrategy="auto-fill"
              minColumnWidth={320}
              scrollParentRef={scrollRef}
              onSkillClick={handleSkillClick}
              onInstall={handleInstall}
              installingNames={installingNames}
              onUpdate={handleUpdate}
              pendingUpdateNames={pendingUpdateNames}
              profiles={profiles}
              onToggleAgent={toggleSkillForAgent}
              pendingAgentToggleKeys={pendingAgentToggleKeys}
              selectedSkills={selectedSkill ? new Set([selectedSkill.name]) : undefined}
              emptyMessage={searchQuery.trim() ? t("marketplace.noResultsSearch") : t("marketplace.noResults")}
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
        <DetailPanel
          skill={selectedSkill}
          onClose={() => setSelectedSkill(null)}
          onInstall={handleInstall}
          onUpdate={handleUpdate}
          onUninstall={handleUninstall}
          onReinstall={handleReinstall}
        />
      )}
    </div>
  );
}
