import { AnimatePresence, motion } from "framer-motion";
import { ArrowUp, Boxes, Loader2, Sparkles, X } from "lucide-react";
import { lazy, Suspense, useCallback, useEffect, useMemo, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { Toolbar } from "../components/layout/Toolbar";
import { Button } from "../components/ui/button";
import { EmptyState } from "../components/ui/EmptyState";
import { LoadingLogo } from "../components/ui/LoadingLogo";
import { McpPublishers } from "../features/mcp/components/McpPublishers";
import { useMcpPublishers } from "../features/mcp/hooks/useMcpPublishers";
import { OfficialPublishers } from "../features/marketplace/components/OfficialPublishers";
import { useMarketplace } from "../features/marketplace/hooks/useMarketplace";
import { useMarketplaceActions } from "../features/marketplace/hooks/useMarketplaceActions";
import { computeDisplaySkills } from "../features/marketplace/lib/skillDisplay";
import { SkillGrid } from "../features/my-skills/components/SkillGrid";
import { useSkills } from "../features/my-skills/hooks/useSkills";
import { useViewMode } from "../hooks/useViewMode";
import { toast } from "../lib/toast";
import { cn } from "../lib/utils";
import type { McpPublisherSummary, OfficialPublisher, Skill, SortOption } from "../types";

const DetailPanel = lazy(() =>
  import("../components/layout/DetailPanel").then((mod) => ({
    default: mod.DetailPanel,
  })),
);

export type TabId = "all" | "trending" | "hot" | "official" | "mcp-official";

const skillTabIds: TabId[] = ["all", "trending", "hot", "official"];
const mcpTabIds: TabId[] = ["mcp-official"];
const tabIds: TabId[] = [...skillTabIds, ...mcpTabIds];

const tabLabelKeys: Record<TabId, string> = {
  all: "marketplace.allTime",
  trending: "marketplace.trending",
  hot: "marketplace.hot",
  official: "marketplace.official",
  "mcp-official": "marketplace.mcpOfficial",
};

interface MarketplaceProps {
  onNavigateToPublisher?: (publisher: OfficialPublisher) => void;
  onNavigateToMcpPublisher?: (publisher: McpPublisherSummary) => void;
  activeTab?: TabId;
  onTabChange?: (tab: TabId) => void;
}

export function Marketplace({
  onNavigateToPublisher,
  onNavigateToMcpPublisher,
  activeTab: controlledTab,
  onTabChange,
}: MarketplaceProps) {
  const { t } = useTranslation();
  const {
    results,
    leaderboard,
    publishers,
    loading,
    refreshing,
    error,
    snapshotStatus,
    snapshotUpdatedAt,
    search,
    searchOnline,
    aiSearch,
    aiSearching,
    aiPhase,
    aiKeywords,
    aiKeywordSkillMap,
    aiActiveKeywords,
    toggleAiKeyword,
    clearAiSearch,
    fetchLeaderboard,
    fetchOfficialPublishers,
    patchSkill,
  } = useMarketplace();
  const { installSkill, updateSkill, uninstallSkill, pendingUpdateNames } = useSkills();
  const [searchQuery, setSearchQuery] = useState("");
  const [sortBy, setSortBy] = useState<SortOption>("stars-desc");
  const [viewMode, setViewMode] = useViewMode("grid");
  const [internalTab, setInternalTab] = useState<TabId>("all");
  const activeTab = controlledTab ?? internalTab;
  const isMcpTab = activeTab === "mcp-official";
  const mcpPublishers = useMcpPublishers(isMcpTab);
  const setActiveTab = (tab: TabId) => {
    onTabChange?.(tab);
    setInternalTab(tab);
  };
  const handleTabChange = useCallback(
    (tab: TabId) => {
      setActiveTab(tab);
      setSearchQuery("");
      setSelectedSkill(null);
      clearAiSearch();
    },
    [clearAiSearch, onTabChange],
  );
  const [selectedSkill, setSelectedSkill] = useState<Skill | null>(null);
  const [installStatus, setInstallStatus] = useState<string | null>(null);
  const [showBackToTop, setShowBackToTop] = useState(false);
  const scrollRef = useRef<HTMLDivElement>(null);
  /** Skills currently being installed (for per-card loading state) */
  const [installingNames, setInstallingNames] = useState<Set<string>>(new Set());

  // Tab change
  useEffect(() => {
    if (activeTab === "official") {
      fetchOfficialPublishers();
    } else if (isMcpTab) {
      return;
    } else {
      fetchLeaderboard(activeTab === "all" ? "all" : activeTab);
    }
  }, [activeTab, fetchOfficialPublishers, fetchLeaderboard, isMcpTab]);

  // Search (debounced) — skip when AI search is active
  useEffect(() => {
    if (!searchQuery.trim()) return;
    if (aiSearching || aiKeywords) return; // Don't run normal search during/after AI search
    const timer = setTimeout(() => {
      search(searchQuery);
    }, 400);
    return () => clearTimeout(timer);
  }, [searchQuery, search, aiSearching, aiKeywords]);

  const displaySkills = useMemo(
    () =>
      computeDisplaySkills({
        isMcpTab,
        results,
        leaderboard,
        sortBy,
        searchQuery,
        activeTab,
        aiKeywords,
        aiActiveKeywords,
        aiKeywordSkillMap,
      }),
    [activeTab, results, leaderboard, sortBy, searchQuery, aiKeywords, aiActiveKeywords, aiKeywordSkillMap, isMcpTab],
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

  const handleAiSearch = useCallback(() => {
    if (!searchQuery.trim()) {
      toast.error(
        t("marketplace.aiSearchEmptyQuery", {
          defaultValue: "Please enter a search query first",
        }),
      );
      return;
    }
    aiSearch(searchQuery);
  }, [searchQuery, aiSearch, t]);

  const handleClearAiSearch = useCallback(() => {
    clearAiSearch();
    setSearchQuery("");
  }, [clearAiSearch]);

  const toolbarSearchQuery = searchQuery;
  const handleToolbarSearchChange = useCallback(
    (value: string) => {
      setSearchQuery(value);
      if (!value.trim()) clearAiSearch();
    },
    [clearAiSearch],
  );

  const totalCount = isMcpTab ? mcpPublishers.publishers.length : displaySkills.length;
  const snapshotLabel = isMcpTab
    ? null
    : refreshing
      ? t("marketplace.refreshingSnapshot", {
          defaultValue: "Refreshing snapshot...",
        })
      : snapshotStatus === "seeding"
        ? t("marketplace.seedingSnapshot", {
            defaultValue: "Seeding local snapshot...",
          })
        : snapshotStatus === "stale"
          ? t("marketplace.snapshotStale", {
              defaultValue: "Snapshot is stale",
            })
          : null;
  const snapshotTitle = isMcpTab ? undefined : (snapshotUpdatedAt ?? undefined);
  const showOnlineSupplement =
    Boolean(searchQuery.trim()) &&
    !isMcpTab &&
    !aiKeywords &&
    !loading &&
    !aiSearching &&
    displaySkills.length === 0 &&
    snapshotStatus === "miss";

  const renderTabButton = (id: TabId) => {
    const index = tabIds.indexOf(id);
    const isActive = activeTab === id;
    const isMcp = mcpTabIds.includes(id);

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
          "inline-flex h-8 items-center gap-1.5 rounded-full px-3 text-xs font-medium transition-colors duration-150 cursor-pointer focus-ring",
          isActive
            ? "bg-primary/15 text-primary shadow-[0_1px_2px_rgba(15,23,42,0.08),0_1px_1px_rgba(15,23,42,0.04)] ring-1 ring-inset ring-primary/25 dark:bg-primary/18 dark:shadow-[0_1px_2px_rgba(0,0,0,0.45)]"
            : "text-muted-foreground hover:text-foreground hover:bg-sidebar-hover/60",
          isMcp && !isActive && "ring-1 ring-inset ring-border/50 hover:ring-primary/25",
        )}
      >
        {id === "mcp-official" ? (
          <>
            <Boxes className="h-3.5 w-3.5" />
            <span>{t(tabLabelKeys[id])}</span>
          </>
        ) : (
          <span>{t(tabLabelKeys[id])}</span>
        )}
      </button>
    );
  };

  return (
    <div className="flex-1 min-w-0 flex overflow-hidden relative">
      <div className="flex-1 min-w-0 flex flex-col overflow-hidden">
        <Toolbar
          titleNode={<h1>{t("sidebar.market")}</h1>}
          searchQuery={toolbarSearchQuery}
          onSearchChange={handleToolbarSearchChange}
          searchItems={spotlightItems}
          onSearchSelect={handleSpotlightSelect}
          sortBy={sortBy}
          onSortChange={setSortBy}
          viewMode={viewMode}
          onViewModeChange={setViewMode}
          onAiSearch={isMcpTab ? undefined : handleAiSearch}
          aiSearching={isMcpTab ? false : aiSearching}
          hideSortControls={isMcpTab}
        />

        {/* Category tabs */}
        <div className="border-b border-border bg-sidebar px-6 py-2">
          <div className="flex min-w-0 items-center justify-between gap-3">
            <div
              className="flex min-w-0 flex-1 items-center gap-3 overflow-x-auto [scrollbar-width:none] [&::-webkit-scrollbar]:hidden"
              role="tablist"
              aria-label={t("sidebar.market")}
            >
              <div
                className="flex min-w-max items-center gap-2 rounded-full border border-border/60 bg-background/20 p-1"
                role="presentation"
              >
                <span className="shrink-0 px-2 text-[11px] font-medium text-foreground/65">
                  {t("marketplace.skillGroup")}
                </span>
                <div className="flex items-center gap-1">{skillTabIds.map((id) => renderTabButton(id))}</div>
              </div>

              <div
                className="flex min-w-max items-center gap-2 rounded-full border border-border/60 bg-background/35 p-1"
                role="presentation"
              >
                <span className="px-2 text-[11px] font-medium text-foreground/65">
                  {t("marketplace.mcpSourceGithub")}
                </span>
                <div className="flex items-center gap-1">{mcpTabIds.map((id) => renderTabButton(id))}</div>
              </div>
            </div>

            <div className="ml-auto flex shrink-0 items-center gap-3 px-2 text-right" aria-live="polite">
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
              {snapshotLabel && (
                <span className="hidden text-[11px] text-muted-foreground sm:inline" title={snapshotTitle}>
                  {snapshotLabel}
                </span>
              )}
              {isMcpTab ? (
                <span className="text-caption">{t("marketplace.mcpPublishersCount", { count: totalCount })}</span>
              ) : activeTab !== "official" ? (
                <span className="text-caption">{t("marketplace.skillsCount", { count: totalCount })}</span>
              ) : null}
            </div>
          </div>
        </div>

        {!isMcpTab && error && (
          <div className="px-6 py-2 border-b border-destructive/20 bg-destructive/5 text-xs text-destructive">
            {error}
          </div>
        )}

        {/* AI Keywords toggle filter bar */}
        <AnimatePresence>
          {aiKeywords && aiKeywords.length > 0 && !aiSearching && (
            <motion.div
              initial={{ opacity: 0, height: 0 }}
              animate={{ opacity: 1, height: "auto" }}
              exit={{ opacity: 0, height: 0 }}
              transition={{ duration: 0.2 }}
              className="overflow-hidden"
            >
              <div className="flex items-center gap-2 px-6 py-2 border-b border-border bg-sidebar/50 backdrop-blur-sm">
                <Sparkles className="w-3.5 h-3.5 shrink-0 text-ai-text" />
                <div className="flex items-center gap-1.5 flex-wrap">
                  {aiKeywords.map((kw) => {
                    const isActive = aiActiveKeywords.has(kw);
                    const count = aiKeywordSkillMap[kw]?.length ?? 0;
                    return (
                      <button
                        key={kw}
                        onClick={() => toggleAiKeyword(kw)}
                        className={cn(
                          "inline-flex items-center h-[22px] px-2 rounded-full text-[11px] font-medium border transition-all duration-200 cursor-pointer gap-1",
                          isActive
                            ? "bg-ai-bg-hover/60 text-ai-text border-ai-border/40 shadow-[0_0_4px_var(--color-ai-shadow)]"
                            : "bg-transparent text-muted-foreground/50 border-border/30 line-through",
                        )}
                      >
                        {kw}
                        <span className={cn("text-[10px] opacity-60", !isActive && "no-underline")}>{count}</span>
                      </button>
                    );
                  })}
                </div>
                <span className="text-xs text-muted-foreground ml-1">
                  {displaySkills.length} {t("marketplace.aiResultsFound", { defaultValue: "results" })}
                </span>
                <button
                  onClick={handleClearAiSearch}
                  className="ml-auto w-5 h-5 rounded-md flex items-center justify-center text-muted-foreground hover:text-foreground hover:bg-sidebar-hover transition-colors cursor-pointer shrink-0"
                  title={t("common.clear")}
                >
                  <X className="w-3 h-3" />
                </button>
              </div>
            </motion.div>
          )}
        </AnimatePresence>

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
          {isMcpTab ? (
            <McpPublishers publishers={mcpPublishers.publishers} onPublisherClick={onNavigateToMcpPublisher} />
          ) : activeTab === "official" ? (
            <OfficialPublishers publishers={publishers} viewMode={viewMode} onPublisherClick={onNavigateToPublisher} />
          ) : loading || aiSearching ? (
            <div className="flex flex-col items-center justify-center py-20 gap-4">
              <LoadingLogo
                size="lg"
                label={
                  aiSearching
                    ? t("marketplace.aiSearching", {
                        defaultValue: "AI is analyzing your query...",
                      })
                    : t("marketplace.loading")
                }
              />
              {aiSearching && (
                <div className="flex flex-col items-center gap-3 w-full max-w-md">
                  {/* Stage 1: Extracting keywords */}
                  <motion.div
                    initial={{ opacity: 0, y: 8 }}
                    animate={{ opacity: 1, y: 0 }}
                    className="flex items-center gap-2 text-xs text-muted-foreground"
                  >
                    {aiPhase === "extracting" ? (
                      <Loader2 className="w-3 h-3 animate-spin text-ai-text" />
                    ) : (
                      <Sparkles className="w-3 h-3 text-ai-text" />
                    )}
                    {t("marketplace.aiPhaseExtract", {
                      defaultValue: "Extracting search keywords...",
                    })}
                  </motion.div>

                  {/* Keywords appear after extraction */}
                  <AnimatePresence>
                    {aiKeywords && aiKeywords.length > 0 && (
                      <motion.div
                        initial={{ opacity: 0, y: 8 }}
                        animate={{ opacity: 1, y: 0 }}
                        className="flex items-center gap-1.5 flex-wrap justify-center"
                      >
                        {aiKeywords.map((kw) => (
                          <span
                            key={kw}
                            className="inline-flex items-center h-[22px] px-2 rounded-full text-[11px] font-medium bg-ai-bg-hover/60 text-ai-text border border-ai-border/40 shadow-[0_0_4px_var(--color-ai-shadow)]"
                          >
                            {kw}
                          </span>
                        ))}
                      </motion.div>
                    )}
                  </AnimatePresence>

                  {/* Stage 2: Concurrent searching */}
                  <AnimatePresence>
                    {aiPhase === "searching" && (
                      <motion.div
                        initial={{ opacity: 0, y: 8 }}
                        animate={{ opacity: 1, y: 0 }}
                        className="flex items-center gap-2 text-xs text-muted-foreground"
                      >
                        <Loader2 className="w-3 h-3 animate-spin text-ai-text" />
                        {t("marketplace.aiPhaseSearch", {
                          defaultValue: "Searching concurrently...",
                        })}
                      </motion.div>
                    )}
                  </AnimatePresence>
                </div>
              )}
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
              emptyMessage={
                searchQuery.trim() || aiKeywords ? t("marketplace.noResultsSearch") : t("marketplace.noResults")
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
            <div className="absolute right-0 top-0 bottom-0 w-full max-w-md h-full border-l border-border bg-card backdrop-blur-xl shadow-2xl overflow-y-auto z-50 rounded-tl-xl rounded-bl-xl flex items-center justify-center">
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
    </div>
  );
}
