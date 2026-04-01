import {
  useState,
  useEffect,
  useMemo,
  useCallback,
  useRef,
  lazy,
  Suspense,
} from "react";
import { motion, AnimatePresence } from "framer-motion";
import { useTranslation } from "react-i18next";
import { Toolbar } from "../components/layout/Toolbar";
import { SkillGrid } from "../components/skills/SkillGrid";
import { OfficialPublishers } from "../components/marketplace/OfficialPublishers";
import { EmptyState } from "../components/ui/EmptyState";
import { Button } from "../components/ui/button";
import { useMarketplace } from "../hooks/useMarketplace";
import { useSkills } from "../hooks/useSkills";
import { ArrowUp, Sparkles, X, Loader2 } from "lucide-react";
import { toast } from "../lib/toast";
import { LoadingLogo } from "../components/ui/LoadingLogo";
import type { OfficialPublisher, Skill, SortOption, ViewMode } from "../types";
import { cn } from "../lib/utils";

const DetailPanel = lazy(() =>
  import("../components/layout/DetailPanel").then((mod) => ({
    default: mod.DetailPanel,
  })),
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

export function Marketplace({
  onNavigateToPublisher,
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
  const { installSkill, updateSkill, uninstallSkill, pendingUpdateNames } =
    useSkills();
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
  /** Skills currently being installed (for per-card loading state) */
  const [installingNames, setInstallingNames] = useState<Set<string>>(
    new Set(),
  );

  // Tab change
  useEffect(() => {
    if (activeTab === "official") {
      fetchOfficialPublishers();
    } else {
      fetchLeaderboard(activeTab === "all" ? "all" : activeTab);
    }
  }, [activeTab, fetchOfficialPublishers, fetchLeaderboard]);

  // Search (debounced) — skip when AI search is active
  useEffect(() => {
    if (!searchQuery.trim()) return;
    if (aiSearching || aiKeywords) return; // Don't run normal search during/after AI search
    const timer = setTimeout(() => {
      search(searchQuery);
    }, 400);
    return () => clearTimeout(timer);
  }, [searchQuery, search, aiSearching, aiKeywords]);

  const displaySkills = useMemo(() => {
    let skills: Skill[] = [];
    const isAiMode = Boolean(aiKeywords && results);
    const isSearchMode = Boolean(searchQuery.trim() && results) || isAiMode;

    // Search results override
    if (isSearchMode && results) {
      skills = [...results.skills];
    } else if (activeTab !== "official") {
      skills = [...leaderboard];
    }

    // AI keyword toggle filter: only show skills matching active keywords
    if (
      isAiMode &&
      aiActiveKeywords.size > 0 &&
      Object.keys(aiKeywordSkillMap).length > 0
    ) {
      // Build set of skill names matching any active keyword
      const allowedNames = new Set<string>();
      for (const kw of aiActiveKeywords) {
        const names = aiKeywordSkillMap[kw];
        if (names) names.forEach((n) => allowedNames.add(n));
      }
      skills = skills.filter((s) => allowedNames.has(s.name));
    }

    // Sort
    if (sortBy === "name") {
      skills.sort((a, b) => a.name.localeCompare(b.name));
    } else if (sortBy === "updated") {
      skills.sort((a, b) => b.last_updated.localeCompare(a.last_updated));
    } else if (isSearchMode) {
      skills.sort((a, b) => b.stars - a.stars);
    }

    return skills.map((s, i) => {
      const rank =
        sortBy === "stars-desc"
          ? isSearchMode
            ? i + 1
            : (s.rank ?? i + 1)
          : s.rank;
      if (rank === s.rank) return s;
      return { ...s, rank };
    });
  }, [
    activeTab,
    results,
    leaderboard,
    sortBy,
    searchQuery,
    aiKeywords,
    aiActiveKeywords,
    aiKeywordSkillMap,
  ]);

  const handleInstall = useCallback(
    async (url: string, name: string) => {
      if (!url || !name) return;

      setInstallingNames((prev) => {
        const next = new Set(prev);
        next.add(name);
        return next;
      });

      try {
        const skill = await installSkill(url, name);
        patchSkill(name, (current) => ({
          ...current,
          installed: true,
          update_available: false,
          agent_links: skill.agent_links ?? current.agent_links,
        }));
        setSelectedSkill((prev) => {
          if (!prev) return prev;
          if (prev.name === name) {
            return {
              ...prev,
              installed: true,
              update_available: false,
              agent_links: skill.agent_links ?? prev.agent_links,
            };
          }
          return prev;
        });

        const agentCount = skill.agent_links?.length ?? 0;
        setInstallStatus(
          agentCount > 0
            ? t("marketplace.installedSynced", {
                count: agentCount,
                defaultValue: "✓ Installed & synced to {{count}} agents",
              })
            : t("marketplace.installedViaGithub"),
        );
        setTimeout(() => setInstallStatus(null), 4000);
      } catch (e) {
        const message = String(e).toLowerCase();
        if (message.includes("already installed")) {
          patchSkill(name, (current) => ({ ...current, installed: true }));
          setSelectedSkill((prev) =>
            prev?.name === name ? { ...prev, installed: true } : prev,
          );
          setInstallStatus(t("marketplace.installedViaGithub"));
          setTimeout(() => setInstallStatus(null), 4000);
          return;
        }
        console.error("[Marketplace] Install failed:", e);
        toast.error(
          String(e)
            ? `${t("mySkills.installFailed")}: ${String(e)}`
            : t("mySkills.installFailed"),
        );
      } finally {
        setInstallingNames((prev) => {
          const next = new Set(prev);
          next.delete(name);
          return next;
        });
      }
    },
    [installSkill, patchSkill, t],
  );

  const handleUpdate = useCallback(
    async (name: string) => {
      try {
        await updateSkill(name);
        patchSkill(name, (current) => ({
          ...current,
          update_available: false,
        }));
        setSelectedSkill((prev) =>
          prev?.name === name ? { ...prev, update_available: false } : prev,
        );
      } catch (e) {
        console.error("Update failed:", e);
        const reason = String(e);
        toast.error(
          reason
            ? `${t("marketplace.updateFailed")}: ${reason}`
            : t("marketplace.updateFailed"),
        );
      }
    },
    [patchSkill, t, updateSkill],
  );

  const handleUninstall = useCallback(
    async (name: string) => {
      try {
        await uninstallSkill(name);
        patchSkill(name, (current) => ({
          ...current,
          installed: false,
          update_available: false,
          agent_links: [],
        }));
        if (selectedSkill?.name === name) {
          setSelectedSkill((prev) =>
            prev
              ? {
                  ...prev,
                  installed: false,
                  update_available: false,
                  agent_links: [],
                }
              : null,
          );
        }
      } catch (e) {
        console.error("[Marketplace] Uninstall failed:", e);
        toast.error(t("marketplace.uninstallFailed"));
      }
    },
    [patchSkill, uninstallSkill, selectedSkill, t],
  );

  const handleReinstall = useCallback(
    async (url: string, name: string) => {
      try {
        await uninstallSkill(name);
        await handleInstall(url, name);
      } catch (e) {
        console.error("[Marketplace] Reinstall failed:", e);
        toast.error(t("marketplace.reinstallFailed"));
      }
    },
    [uninstallSkill, handleInstall, t],
  );

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

  const totalCount =
    searchQuery.trim() || aiKeywords
      ? (results?.total_count ?? 0)
      : leaderboard.length;
  const showOnlineSupplement =
    Boolean(searchQuery.trim()) &&
    !aiKeywords &&
    !loading &&
    !aiSearching &&
    displaySkills.length === 0 &&
    snapshotStatus === "miss";

  useEffect(() => {
    if (activeTab === "official") {
      setVisibleSkillCount(0);
      return;
    }
    setVisibleSkillCount(
      displaySkills.length <= EAGER_RENDER_THRESHOLD
        ? displaySkills.length
        : INITIAL_MARKETPLACE_VISIBLE_COUNT,
    );
  }, [activeTab, displaySkills.length]);

  return (
    <div className="flex-1 flex overflow-hidden relative">
      <div className="flex-1 flex flex-col overflow-hidden">
        <Toolbar
          titleNode={<h1>{t("sidebar.market")}</h1>}
          searchQuery={searchQuery}
          onSearchChange={(q) => {
            setSearchQuery(q);
            if (!q.trim()) {
              clearAiSearch();
            }
          }}
          sortBy={sortBy}
          onSortChange={setSortBy}
          viewMode={viewMode}
          onViewModeChange={setViewMode}
          onAiSearch={handleAiSearch}
          aiSearching={aiSearching}
        />

        {/* Category tabs */}
        <div
          className="flex items-center gap-1 px-6 py-2 border-b border-border bg-sidebar"
          role="tablist"
        >
          {tabIds.map((id, index) => (
            <button
              key={id}
              role="tab"
              aria-selected={activeTab === id}
              tabIndex={activeTab === id ? 0 : -1}
              id={`tab-${id}`}
              aria-controls={`tabpanel-${id}`}
              onClick={() => {
                setActiveTab(id);
                setSearchQuery("");
                clearAiSearch();
              }}
              onKeyDown={(e) => {
                let next = index;
                if (e.key === "ArrowRight") next = (index + 1) % tabIds.length;
                else if (e.key === "ArrowLeft")
                  next = (index - 1 + tabIds.length) % tabIds.length;
                else if (e.key === "Home") next = 0;
                else if (e.key === "End") next = tabIds.length - 1;
                else return;
                e.preventDefault();
                const nextId = tabIds[next];
                setActiveTab(nextId);
                setSearchQuery("");
                clearAiSearch();
                document.getElementById(`tab-${nextId}`)?.focus();
              }}
              className={cn(
                "px-3 py-1.5 rounded-md text-xs font-medium transition-colors cursor-pointer",
                activeTab === id
                  ? "bg-primary/20 text-primary shadow-sm"
                  : "text-muted-foreground hover:text-foreground hover:bg-sidebar-hover",
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
            {(refreshing ||
              snapshotStatus === "stale" ||
              snapshotStatus === "seeding") && (
              <span
                className="text-[11px] text-muted-foreground"
                title={snapshotUpdatedAt ?? undefined}
              >
                {refreshing
                  ? t("marketplace.refreshingSnapshot", {
                      defaultValue: "Refreshing snapshot...",
                    })
                  : snapshotStatus === "seeding"
                    ? t("marketplace.seedingSnapshot", {
                        defaultValue: "Seeding local snapshot...",
                      })
                    : t("marketplace.snapshotStale", {
                        defaultValue: "Snapshot is stale",
                      })}
              </span>
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

        {error && (
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
                        <span
                          className={cn(
                            "text-[10px] opacity-60",
                            !isActive && "no-underline",
                          )}
                        >
                          {count}
                        </span>
                      </button>
                    );
                  })}
                </div>
                <span className="text-xs text-muted-foreground ml-1">
                  {displaySkills.length}{" "}
                  {t("marketplace.aiResultsFound", { defaultValue: "results" })}
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
          className="flex-1 overflow-y-auto p-6"
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
                defaultValue:
                  "No local matches yet. You can run one remote search and seed the snapshot.",
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
              onVisibleCountChange={(visible) => setVisibleSkillCount(visible)}
              onSkillClick={(skill) =>
                setSelectedSkill((prev) =>
                  prev?.name === skill.name ? null : skill,
                )
              }
              onInstall={handleInstall}
              installingNames={installingNames}
              onUpdate={handleUpdate}
              pendingUpdateNames={pendingUpdateNames}
              emptyMessage={
                searchQuery.trim() || aiKeywords
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
              onClick={() =>
                scrollRef.current?.scrollTo({ top: 0, behavior: "smooth" })
              }
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
            <div className="absolute right-0 top-0 bottom-0 w-full max-w-sm h-full border-l border-border bg-card backdrop-blur-xl shadow-2xl overflow-y-auto z-50 rounded-tl-xl rounded-bl-xl flex items-center justify-center">
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
