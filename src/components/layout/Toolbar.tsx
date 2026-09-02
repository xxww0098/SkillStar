import {
  ArrowUpCircle,
  Boxes,
  Check,
  ChevronDown,
  Download,
  GitFork,
  ListFilter,
  Loader2,
  RefreshCw,
  Search,
  Trash2,
  X,
} from "lucide-react";

import { Popover } from "radix-ui";
import { useCallback, useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { toast } from "../../lib/toast";
import { cn } from "../../lib/utils";
import type { AgentProfile, SortOption, ViewMode } from "../../types";
import { AgentFilterPill } from "../ui/AgentFilterPill";
import { ViewToggle } from "../ui/ViewToggle";
import { PageToolbar } from "./PageToolbar";
import { type SpotlightSearchItem, SpotlightSearch } from "./SpotlightSearch";

interface ToolbarProps {
  searchQuery: string;
  onSearchChange: (q: string) => void;
  sortBy: SortOption;
  onSortChange: (s: SortOption) => void;
  viewMode: ViewMode;
  onViewModeChange: (v: ViewMode) => void;
  /** Agent profiles for the agent filter */
  agentProfiles?: AgentProfile[];
  /** Currently selected agent filter ID, null = show all */
  agentFilter?: string | null;
  /** Callback when agent filter changes */
  onAgentFilterChange?: (agentId: string | null) => void;
  /** Optional callback for Import from GitHub button */
  onImport?: () => void;
  /** Optional callback to refresh the data */
  onRefresh?: () => void;
  /** Whether the data is currently refreshing */
  isRefreshing?: boolean;
  /** Optional count label shown in toolbar (e.g. "12 cards") */
  countText?: React.ReactNode;
  /** Number of cards with available updates */
  pendingUpdateCount?: number;
  /** Hide the "Stars" sort option */
  hideStarsSort?: boolean;
  /** Callback to update all pending skills */
  onUpdateAll?: () => void;
  /** Whether batch update-all is in progress */
  isUpdatingAll?: boolean;
  /** Optional callback for AI marketplace search */
  onAiSearch?: () => void;
  /** Whether AI search is in progress */
  aiSearching?: boolean;
  /** Optional title node to render at the start of the toolbar */
  titleNode?: React.ReactNode;
  /** Prepended inside the filters row (e.g. remote host picker) */
  filtersLead?: React.ReactNode;
  /** Prepended inside the actions row (e.g. push / console) */
  actionsLead?: React.ReactNode;
  /** Optional search placeholder override */
  searchPlaceholder?: string;
  /** Candidates for Spotlight result list (filtered client-side by query). */
  searchItems?: SpotlightSearchItem[];
  /** Open detail / focus a result selected from Spotlight. */
  onSearchSelect?: (id: string) => void;
  /** Hide sort segmented controls */
  hideSortControls?: boolean;
  /** Hide grid/list view toggle */
  hideViewToggle?: boolean;
  /** Source type filter: "all" | "hub" | "local" */
  sourceFilter?: "all" | "hub" | "local";
  /** Callback when source filter changes */
  onSourceFilterChange?: (filter: "all" | "hub" | "local") => void;
  /** Number of local skills */
  localCount?: number;
  /** Unique repo source strings for repo filter (e.g. ["pbakaus/impeccable", "xxww0098/skills-hub"]) */
  repoSources?: string[];
  /** Currently selected repo source filter, null = show all */
  repoFilter?: string | null;
  /** Callback when repo filter changes */
  onRepoFilterChange?: (source: string | null) => void;
  /** Remove all skills from a repo source (shown as a trash button on each source row) */
  onRemoveRepoSource?: (source: string) => void;
  /** Reinstall every skill discovered from a repo source. */
  onReinstallRepoSource?: (source: string) => void;
  /** Source currently being reinstalled, if any. */
  reinstallingRepoSource?: string | null;
  /** Updatable skills inside the current search/source filters — the number on
   *  the filter chip, so it can never promise more than the filter will show.
   *  Falls back to `pendingUpdateCount` when the caller has no filters. */
  filteredUpdateCount?: number;
  /** Whether "only updates" filter is active */
  onlyUpdatesFilter?: boolean;
  /** Callback when "only updates" filter changes */
  onOnlyUpdatesFilterChange?: (enabled: boolean) => void;
}

export function Toolbar({
  searchQuery,
  onSearchChange,
  sortBy,
  onSortChange,
  viewMode,
  onViewModeChange,
  agentProfiles,
  agentFilter,
  onAgentFilterChange,
  onImport,
  onRefresh,
  isRefreshing,
  countText,
  pendingUpdateCount,
  hideStarsSort,
  onAiSearch,
  aiSearching,
  titleNode,
  filtersLead,
  actionsLead,
  searchPlaceholder,
  searchItems,
  onSearchSelect,
  hideSortControls,
  hideViewToggle,
  sourceFilter,
  onSourceFilterChange,
  localCount,
  onUpdateAll,
  isUpdatingAll,
  repoSources,
  repoFilter,
  onRepoFilterChange,
  onRemoveRepoSource,
  onReinstallRepoSource,
  reinstallingRepoSource,
  filteredUpdateCount,
  onlyUpdatesFilter,
  onOnlyUpdatesFilterChange,
}: ToolbarProps) {
  const { t } = useTranslation();
  const [spotlightOpen, setSpotlightOpen] = useState(false);
  const hasActiveSearch = Boolean(searchQuery.trim());

  const enabledProfiles = agentProfiles?.filter((p) => p.enabled) ?? [];
  const hasPendingUpdates = (pendingUpdateCount ?? 0) > 0;
  const chipUpdateCount = filteredUpdateCount ?? pendingUpdateCount ?? 0;

  const [cooldown, setCooldown] = useState(false);
  const [refreshState, setRefreshState] = useState<"idle" | "requested" | "refreshing">("idle");

  const handleRefresh = useCallback(() => {
    if (cooldown || isRefreshing) return;
    setCooldown(true);
    setRefreshState("requested");
    onRefresh?.();
    setTimeout(() => {
      setCooldown(false);
    }, 5000);
  }, [cooldown, isRefreshing, onRefresh]);

  useEffect(() => {
    if (refreshState === "requested") {
      if (isRefreshing) {
        setRefreshState("refreshing");
      } else {
        const timer = setTimeout(() => {
          if (refreshState === "requested") {
            toast.success(t("common.refreshed", { defaultValue: "Refreshed successfully" }));
            setRefreshState("idle");
          }
        }, 500);
        return () => clearTimeout(timer);
      }
    } else if (refreshState === "refreshing") {
      if (!isRefreshing) {
        toast.success(t("common.refreshed", { defaultValue: "Refreshed successfully" }));
        setRefreshState("idle");
      }
    }
  }, [isRefreshing, refreshState, t]);

  const sortOptions: { value: SortOption; label: string }[] = [
    ...(hideSortControls || hideStarsSort ? [] : [{ value: "stars-desc" as SortOption, label: t("toolbar.stars") }]),
    ...(hideSortControls || hideStarsSort ? [] : [{ value: "updated" as SortOption, label: t("toolbar.updated") }]),
  ];

  const [originOpen, setOriginOpen] = useState(false);
  const hasSourceFilter = Boolean(onSourceFilterChange) && (localCount ?? 0) > 0;
  const showRepoFilter = (repoSources?.length ?? 0) >= 1 && Boolean(onRepoFilterChange);
  const showOriginFilter = hasSourceFilter || showRepoFilter;
  const effectiveSource = sourceFilter ?? "all";
  const originActive = effectiveSource !== "all" || Boolean(repoFilter);
  const originLabel = repoFilter
    ? repoFilter
    : effectiveSource === "hub"
      ? t("toolbar.hub")
      : effectiveSource === "local"
        ? t("toolbar.local")
        : t("toolbar.source");
  const clearOrigin = () => {
    onSourceFilterChange?.("all");
    onRepoFilterChange?.(null);
    setOriginOpen(false);
  };

  // ⌘F / Ctrl+F opens Spotlight; bare `/` when not typing in an input.
  useEffect(() => {
    const onKeyDown = (e: KeyboardEvent) => {
      const meta = e.metaKey || e.ctrlKey;
      const target = e.target as HTMLElement | null;
      const isInput = target?.tagName === "INPUT" || target?.tagName === "TEXTAREA" || target?.isContentEditable;

      if (meta && e.key.toLowerCase() === "f") {
        e.preventDefault();
        setSpotlightOpen(true);
        return;
      }

      if (!isInput && !meta && !e.altKey && e.key === "/") {
        e.preventDefault();
        setSpotlightOpen(true);
      }
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, []);

  // ── Search slot: compact trigger + Spotlight overlay ──
  const searchSlot = (
    <>
      <div className="flex items-center gap-1 shrink-0">
        <button
          type="button"
          onClick={() => setSpotlightOpen(true)}
          className={cn(
            "flex items-center h-8 gap-1.5 rounded-lg border text-xs font-medium cursor-pointer whitespace-nowrap transition-all duration-150 focus-ring select-none",
            hasActiveSearch ? "pl-2.5 pr-1.5 max-w-[11rem]" : "px-2.5",
            hasActiveSearch
              ? "border-primary bg-primary/20 text-primary shadow-xs ring-1 ring-primary/30"
              : "border-border/80 bg-background/60 text-foreground/80 hover:text-foreground hover:bg-muted/70 hover:border-primary/40 shadow-2xs",
          )}
          title={`${searchPlaceholder ?? t("toolbar.searchPlaceholder")} (⌘F)`}
          aria-label={searchPlaceholder ?? t("toolbar.searchPlaceholder")}
          aria-expanded={spotlightOpen}
          aria-haspopup="dialog"
        >
          <Search className="w-3.5 h-3.5 shrink-0" />
          {hasActiveSearch ? (
            <>
              <span className="min-w-0 truncate max-w-[5.5rem] font-semibold">{searchQuery}</span>
              <span
                role="button"
                tabIndex={0}
                className="rounded-md p-0.5 transition-colors hover:bg-primary/25 shrink-0"
                onClick={(e) => {
                  e.stopPropagation();
                  onSearchChange("");
                }}
                onKeyDown={(e) => {
                  if (e.key === "Enter" || e.key === " ") {
                    e.stopPropagation();
                    e.preventDefault();
                    onSearchChange("");
                  }
                }}
                aria-label={t("common.clear", { defaultValue: "Clear" })}
              >
                <X className="w-3 h-3" />
              </span>
            </>
          ) : (
            <kbd className="hidden sm:inline-flex items-center rounded-md border border-border/80 bg-muted/80 px-1.5 py-0.5 font-mono text-[10px] font-bold text-foreground">
              ⌘F
            </kbd>
          )}
        </button>
      </div>

      <SpotlightSearch
        open={spotlightOpen}
        onClose={() => setSpotlightOpen(false)}
        query={searchQuery}
        onQueryChange={onSearchChange}
        items={searchItems ?? []}
        onSelect={(id) => {
          onSearchSelect?.(id);
        }}
        placeholder={searchPlaceholder}
        onAiSearch={onAiSearch}
        aiSearching={aiSearching}
      />
    </>
  );

  // ── Filters slot ──
  const filtersSlot = (
    <>
      {filtersLead}
      {/* Agent filter */}
      {enabledProfiles.length > 0 && onAgentFilterChange && (
        <AgentFilterPill items={enabledProfiles} value={agentFilter ?? null} onChange={onAgentFilterChange} />
      )}

      {/* Unified origin filter: source type (Hub / Local) + repo source, with count inlined */}
      {showOriginFilter && (
        <Popover.Root open={originOpen} onOpenChange={setOriginOpen}>
          <Popover.Trigger asChild>
            <button
              className={cn(
                "flex items-center h-8 gap-1.5 rounded-lg border text-xs font-medium cursor-pointer whitespace-nowrap shrink-0 transition-all duration-150 focus-ring pl-2.5",
                originActive ? "pr-1.5" : "pr-2",
                originActive
                  ? "border-primary/60 bg-primary/15 text-primary font-semibold hover:bg-primary/20 max-w-[240px] shadow-2xs ring-1 ring-primary/25"
                  : "border-border/80 bg-background/60 shadow-2xs backdrop-blur-md text-foreground/80 hover:text-foreground hover:bg-accent/15 hover:border-primary/40",
              )}
              title={t("toolbar.source")}
              aria-label={t("toolbar.source")}
            >
              <Boxes className="w-3.5 h-3.5 shrink-0" />
              <span className="max-w-[7rem] truncate">{originLabel}</span>
              {countText ? (
                <>
                  <span className="w-px h-3.5 shrink-0 bg-border/80" aria-hidden />
                  <span
                    className={cn(
                      "flex items-center gap-1 tabular-nums shrink-0 font-bold",
                      originActive ? "text-primary" : "text-foreground",
                    )}
                  >
                    {countText}
                  </span>
                </>
              ) : null}
              {originActive ? (
                <span
                  role="button"
                  tabIndex={0}
                  className="ml-0.5 rounded-sm hover:bg-primary/20 p-0.5 transition-colors shrink-0"
                  onClick={(e) => {
                    e.stopPropagation();
                    clearOrigin();
                  }}
                  onKeyDown={(e) => {
                    if (e.key === "Enter" || e.key === " ") {
                      e.stopPropagation();
                      clearOrigin();
                    }
                  }}
                >
                  <X className="w-3 h-3" />
                </span>
              ) : (
                <ChevronDown className="w-3 h-3 shrink-0 opacity-60" />
              )}
            </button>
          </Popover.Trigger>
          <Popover.Portal>
            <Popover.Content
              sideOffset={6}
              align="start"
              className="z-50 min-w-[200px] max-w-[280px] max-h-[360px] overflow-y-auto rounded-xl border border-border bg-card/95 backdrop-blur-xl shadow-xl p-1.5 animate-in fade-in-0 zoom-in-95 data-[state=closed]:animate-out data-[state=closed]:fade-out-0 data-[state=closed]:zoom-out-95"
            >
              {/* Source type section — segmented tabs */}
              {hasSourceFilter && (
                <div className="px-1.5 pt-1 pb-1.5">
                  <div
                    role="tablist"
                    aria-label={t("toolbar.sourceType")}
                    className="flex items-center gap-0.5 rounded-lg border border-border/80 bg-sidebar/30 p-0.5 h-8"
                  >
                    {(["all", "hub", "local"] as const).map((f) => {
                      const isActive = effectiveSource === f;
                      const label =
                        f === "all" ? t("toolbar.all") : f === "hub" ? t("toolbar.hub") : t("toolbar.local");
                      return (
                        <button
                          key={f}
                          type="button"
                          role="tab"
                          aria-selected={isActive}
                          onClick={() => {
                            onSourceFilterChange?.(f);
                            if (f === "local") onRepoFilterChange?.(null);
                          }}
                          className={cn(
                            "relative flex-1 h-full px-2 rounded-md text-xs font-medium cursor-pointer z-10 transition-colors focus-ring",
                            isActive
                              ? "text-accent-foreground"
                              : "text-muted-foreground hover:text-foreground hover:bg-sidebar-hover",
                          )}
                        >
                          <div
                            className={cn(
                              "absolute inset-0 bg-accent rounded-md -z-10 [backface-visibility:hidden]",
                              isActive ? "opacity-100" : "opacity-0",
                            )}
                          />
                          {label}
                        </button>
                      );
                    })}
                  </div>
                </div>
              )}

              {/* Repo source section (hidden when filtering to local-only skills) */}
              {showRepoFilter && effectiveSource !== "local" && (
                <>
                  {hasSourceFilter && <div className="h-px bg-border/50 my-1" />}
                  <div className="px-2.5 pt-1 pb-1.5 text-[10px] font-semibold uppercase tracking-wider text-muted-foreground">
                    {t("toolbar.repo")}
                  </div>
                  <button
                    onClick={() => {
                      onRepoFilterChange?.(null);
                      setOriginOpen(false);
                    }}
                    className={cn(
                      "w-full flex items-center gap-2 px-3 py-2 rounded-lg text-xs font-medium cursor-pointer transition-colors",
                      !repoFilter ? "bg-accent text-accent-foreground" : "text-foreground/80 hover:bg-accent/50",
                    )}
                  >
                    <GitFork className="w-3.5 h-3.5 shrink-0 text-muted-foreground" />
                    <span>{t("toolbar.allRepos")}</span>
                    {!repoFilter && <Check className="w-3.5 h-3.5 ml-auto text-primary" />}
                  </button>
                  {repoSources?.map((source) => {
                    const isActive = repoFilter === source;
                    return (
                      <div
                        key={source}
                        className={cn(
                          "group flex items-center gap-0.5 rounded-lg text-xs transition-colors",
                          isActive ? "bg-primary/10 text-primary font-medium" : "text-foreground/80 hover:bg-accent/50",
                        )}
                      >
                        <button
                          type="button"
                          onClick={() => {
                            onRepoFilterChange?.(isActive ? null : source);
                            setOriginOpen(false);
                          }}
                          className="min-w-0 flex-1 flex items-center gap-2 pl-3 pr-1 py-2 rounded-lg cursor-pointer text-left"
                        >
                          <span className="truncate">{source}</span>
                          {isActive && <Check className="w-3.5 h-3.5 ml-auto shrink-0 text-primary" />}
                        </button>
                        {onReinstallRepoSource ? (
                          <button
                            type="button"
                            disabled={Boolean(reinstallingRepoSource)}
                            className={cn(
                              "mr-0.5 p-1 rounded-md shrink-0 cursor-pointer transition-colors",
                              "text-muted-foreground/50 hover:text-primary hover:bg-primary/10",
                              "focus-ring disabled:cursor-not-allowed disabled:opacity-50",
                            )}
                            title={t("toolbar.reinstallSource", { source })}
                            aria-label={t("toolbar.reinstallSource", { source })}
                            onClick={(e) => {
                              e.stopPropagation();
                              onReinstallRepoSource(source);
                            }}
                          >
                            {reinstallingRepoSource === source ? (
                              <Loader2 className="w-3.5 h-3.5 animate-spin" />
                            ) : (
                              <RefreshCw className="w-3.5 h-3.5" />
                            )}
                          </button>
                        ) : null}
                        {onRemoveRepoSource ? (
                          <button
                            type="button"
                            className={cn(
                              "mr-1.5 p-1 rounded-md shrink-0 cursor-pointer transition-colors",
                              "text-muted-foreground/40 hover:text-destructive hover:bg-destructive/10",
                              "opacity-70 group-hover:opacity-100 focus-visible:opacity-100",
                              "focus-ring",
                            )}
                            title={t("toolbar.removeSource", { source })}
                            aria-label={t("toolbar.removeSource", { source })}
                            onClick={(e) => {
                              e.stopPropagation();
                              setOriginOpen(false);
                              onRemoveRepoSource(source);
                            }}
                          >
                            <Trash2 className="w-3.5 h-3.5" />
                          </button>
                        ) : null}
                      </div>
                    );
                  })}
                </>
              )}
            </Popover.Content>
          </Popover.Portal>
        </Popover.Root>
      )}

      {/* Count badge — only when origin filter is absent (otherwise count is inlined) */}
      {countText && !showOriginFilter ? (
        <div className="h-8 px-3 flex items-center justify-center rounded-lg border border-border/70 bg-background/50 shadow-sm text-xs font-medium text-foreground/80 tabular-nums whitespace-nowrap shrink-0">
          {countText}
        </div>
      ) : null}
    </>
  );

  // ── Actions slot ──
  const actionsSlot = (
    <>
      {actionsLead}
      {/* Import */}
      {onImport && (
        <button
          onClick={onImport}
          className="flex items-center h-8 px-3 gap-1.5 rounded-lg border border-border/80 bg-background/50 shadow-sm backdrop-blur-md text-xs font-medium text-foreground/80 hover:text-foreground hover:bg-accent/10 hover:border-accent/50 transition duration-200 cursor-pointer group whitespace-nowrap shrink-0 focus-ring"
        >
          <Download className="w-3.5 h-3.5 text-muted-foreground group-hover:text-accent-foreground transition-colors" />
          {t("toolbar.import", { defaultValue: "Import" })}
        </button>
      )}

      {/* Refresh */}
      {onRefresh && (
        <button
          onClick={handleRefresh}
          disabled={isRefreshing || cooldown}
          className={cn(
            "flex items-center h-8 w-8 justify-center rounded-lg border border-border/80 bg-background/50 shadow-sm backdrop-blur-md text-foreground/80 hover:text-foreground hover:bg-accent/10 hover:border-accent/50 transition duration-200 cursor-pointer group shrink-0 focus-ring",
            (isRefreshing || cooldown) && "opacity-50 cursor-not-allowed",
          )}
          title={t("common.refresh", { defaultValue: "Refresh" })}
        >
          {isRefreshing ? (
            <Loader2 className="w-3.5 h-3.5 animate-spin text-accent-foreground" />
          ) : (
            <RefreshCw
              className={cn(
                "w-3.5 h-3.5 text-muted-foreground group-hover:text-accent-foreground transition-colors",
                cooldown && "opacity-40",
              )}
            />
          )}
        </button>
      )}

      {/* Sort options (Marketplace only) */}
      {sortOptions.length > 0 && (
        <div className="flex items-center gap-0.5 border border-border/80 rounded-lg overflow-hidden h-8 p-0.5 bg-sidebar/50 shadow-2xs shrink-0">
          {sortOptions.map((opt) => {
            const isActive = sortBy === opt.value;
            return (
              <button
                key={opt.value}
                onClick={() => {
                  onSortChange(opt.value);
                }}
                aria-pressed={isActive}
                className={cn(
                  "relative h-full px-3 flex items-center justify-center rounded-md text-xs font-medium cursor-pointer whitespace-nowrap z-10 focus-ring select-none",
                  isActive
                    ? "text-accent-foreground font-semibold"
                    : "text-muted-foreground hover:text-foreground hover:bg-sidebar-hover",
                )}
              >
                <div
                  className={cn(
                    "absolute inset-0 bg-accent rounded-md -z-10 [backface-visibility:hidden]",
                    isActive ? "opacity-100" : "opacity-0",
                  )}
                />
                {opt.label}
              </button>
            );
          })}
        </div>
      )}

      {/* Updates filter & action group */}
      {(hasPendingUpdates || onlyUpdatesFilter) && (
        <div className="flex items-center h-8 rounded-lg border border-amber-500/45 bg-background/70 backdrop-blur-md shadow-xs overflow-hidden shrink-0">
          {/* Filter toggle */}
          {onOnlyUpdatesFilterChange ? (
            <button
              type="button"
              onClick={() => onOnlyUpdatesFilterChange(!onlyUpdatesFilter)}
              aria-pressed={onlyUpdatesFilter}
              aria-label={t("toolbar.updateFilterLabel", { count: chipUpdateCount })}
              title={
                onlyUpdatesFilter
                  ? t("toolbar.showAllSkills", { defaultValue: "Show all skills" })
                  : t("toolbar.filterUpdatesHint", { defaultValue: "Filter skills with updates" })
              }
              className={cn(
                "flex items-center h-full gap-1.5 px-2.5 text-xs font-medium cursor-pointer transition-colors focus-ring whitespace-nowrap select-none",
                onlyUpdatesFilter
                  ? "bg-amber-500 text-amber-950 font-bold shadow-xs"
                  : "bg-amber-500/15 text-amber-700 dark:text-amber-300 font-semibold hover:bg-amber-500/25",
              )}
            >
              <ListFilter
                className={cn(
                  "w-3.5 h-3.5 shrink-0",
                  onlyUpdatesFilter ? "text-amber-950" : "text-amber-600 dark:text-amber-400",
                )}
              />
              <span className="tabular-nums font-bold">{chipUpdateCount}</span>
              {onlyUpdatesFilter && <X className="w-3 h-3 ml-0.5 opacity-85 hover:opacity-100" />}
            </button>
          ) : null}

          {/* Update all action */}
          {onUpdateAll && hasPendingUpdates && (
            <>
              {onOnlyUpdatesFilterChange && <div className="w-px h-4 bg-amber-500/40 shrink-0" />}
              <button
                type="button"
                onClick={onUpdateAll}
                disabled={isUpdatingAll}
                title={t("toolbar.updateAllAction", { defaultValue: "Update all" })}
                className={cn(
                  "flex items-center h-full gap-1.5 px-2.5 text-xs font-semibold bg-amber-500/20 text-amber-900 dark:text-amber-200 hover:bg-amber-500/30 transition-colors cursor-pointer focus-ring whitespace-nowrap select-none",
                  isUpdatingAll && "opacity-60 cursor-not-allowed",
                )}
              >
                {isUpdatingAll ? (
                  <Loader2 className="w-3.5 h-3.5 animate-spin text-amber-500" />
                ) : (
                  <ArrowUpCircle className="w-3.5 h-3.5 text-amber-600 dark:text-amber-400" />
                )}
                <span>{isUpdatingAll ? t("common.updating") : t("toolbar.updateAllAction")}</span>
              </button>
            </>
          )}
        </div>
      )}

      {/* View toggle */}
      {!hideViewToggle && <ViewToggle viewMode={viewMode} onViewModeChange={onViewModeChange} />}
    </>
  );

  return <PageToolbar title={titleNode} search={searchSlot} filters={filtersSlot} actions={actionsSlot} />;
}
