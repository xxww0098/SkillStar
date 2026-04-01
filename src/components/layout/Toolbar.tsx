import { useState, useCallback } from "react";

import { RefreshCw, Loader2, Sparkles, Download } from "lucide-react";
import { useTranslation } from "react-i18next";
import { SearchInput } from "../ui/SearchInput";
import { HScrollRow } from "../ui/HScrollRow";
import { AgentIcon } from "../ui/AgentIcon";
import { cn, agentIconCls } from "../../lib/utils";
import type { AgentProfile, SortOption, ViewMode } from "../../types";

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
  /** Optional callback for Import from File (.ags) button */
  onImportBundle?: () => void;
  /** Optional callback to refresh the data */
  onRefresh?: () => void;
  /** Whether the data is currently refreshing */
  isRefreshing?: boolean;
  /** Optional count label shown in toolbar (e.g. "12 cards") */
  countText?: React.ReactNode;
  /** Show only cards with update available */
  showUpdateOnly?: boolean;
  /** Toggle update-only filter */
  onToggleUpdateOnly?: () => void;
  /** Number of cards with available updates */
  pendingUpdateCount?: number;
  /** Hide the "Stars" sort option */
  hideStarsSort?: boolean;
  /** Optional callback for AI pick skills button */
  onAiPick?: () => void;
  /** Optional callback for AI marketplace search */
  onAiSearch?: () => void;
  /** Whether AI search is in progress */
  aiSearching?: boolean;
  /** Optional title node to render at the start of the toolbar */
  titleNode?: React.ReactNode;
  /** Source type filter: "all" | "hub" | "local" */
  sourceFilter?: "all" | "hub" | "local";
  /** Callback when source filter changes */
  onSourceFilterChange?: (filter: "all" | "hub" | "local") => void;
  /** Number of local skills */
  localCount?: number;
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
  showUpdateOnly,
  onToggleUpdateOnly,
  pendingUpdateCount,
  hideStarsSort,
  onAiPick,
  onAiSearch,
  aiSearching,
  titleNode,
  sourceFilter,
  onSourceFilterChange,
  localCount,
}: ToolbarProps) {
  const { t } = useTranslation();

  const enabledProfiles = agentProfiles?.filter((p) => p.enabled) ?? [];
  const hasPendingUpdates = (pendingUpdateCount ?? 0) > 0;
  const shouldAnimateUpdateOnly = hasPendingUpdates && !showUpdateOnly;

  const [cooldown, setCooldown] = useState(false);
  const handleRefresh = useCallback(() => {
    if (cooldown || isRefreshing) return;
    setCooldown(true);
    onRefresh?.();
    setTimeout(() => {
      setCooldown(false);
    }, 5000);
  }, [cooldown, isRefreshing, onRefresh]);

  const sortOptions: { value: SortOption; label: string }[] = [
    ...(hideStarsSort ? [] : [{ value: "stars-desc" as SortOption, label: t("toolbar.stars") }]),
    { value: "updated", label: t("toolbar.updated") },
    { value: "name", label: t("toolbar.name") },
  ];

  return (
    <div className="h-14 flex items-center gap-3 px-6 border-b border-border bg-sidebar overflow-x-auto [&::-webkit-scrollbar]:hidden">
      {titleNode && (
        <div className="flex items-center shrink-0 h-8 whitespace-nowrap">
          {titleNode}
          <div className="w-px h-5 ml-4 mr-1 bg-border" />
        </div>
      )}
      
      <SearchInput
        containerClassName="w-56 shrink-0"
        value={searchQuery}
        onChange={(e) => onSearchChange(e.target.value)}
        placeholder={t("toolbar.searchPlaceholder")}
        className="pl-8 h-8 text-xs bg-sidebar/50 focus-visible:bg-background"
        iconClassName="left-2.5"
        suffix={onAiSearch ? (
          <button
            onClick={onAiSearch}
            disabled={aiSearching || !searchQuery.trim()}
            className={cn(
              "flex items-center justify-center w-6 h-6 rounded-md transition-all duration-300 cursor-pointer shrink-0",
              aiSearching
                ? "text-ai-text-hover animate-pulse"
                : searchQuery.trim()
                  ? "text-ai-text hover:text-ai-text-hover hover:bg-ai-bg-hover"
                  : "text-muted-foreground/30 cursor-not-allowed"
            )}
            title={t("marketplace.aiSearch", { defaultValue: "AI Search" })}
          >
            {aiSearching ? (
              <Loader2 className="w-3.5 h-3.5 animate-spin" />
            ) : (
              <Sparkles className="w-3.5 h-3.5" />
            )}
          </button>
        ) : undefined}
      />

      {/* AI Pick Skills */}
      {onAiPick && (
        <button
          onClick={onAiPick}
          className="flex items-center h-8 px-3 rounded-lg border border-ai-border bg-transparent text-xs font-medium text-ai-text hover:text-ai-text-hover hover:bg-ai-bg-hover hover:border-ai-border-hover transition duration-300 cursor-pointer shadow-[0_0_8px_var(--color-ai-shadow)] gap-1.5 whitespace-nowrap shrink-0 focus-ring"
        >
          <Sparkles className="w-3.5 h-3.5 shrink-0" />
          {t("toolbar.aiPick")}
        </button>
      )}

      {/* Agent filter */}
      {enabledProfiles.length > 0 && onAgentFilterChange && (
        <div className="flex items-center gap-0 border border-border rounded-lg overflow-hidden h-8 p-0.5 bg-sidebar/30 shrink-0">
          <button
            onClick={() => onAgentFilterChange(null)}
            aria-pressed={agentFilter === null}
            className={cn(
              "relative h-full px-2.5 flex items-center justify-center rounded-md text-xs font-medium cursor-pointer whitespace-nowrap z-10 shrink-0 focus-ring",
              agentFilter === null
                ? "text-accent-foreground"
                : "text-muted-foreground hover:text-foreground hover:bg-sidebar-hover"
            )}
          >
            <div className={cn(
              "absolute inset-0 bg-accent rounded-md -z-10 [backface-visibility:hidden]",
              agentFilter === null ? "opacity-100" : "opacity-0"
            )} />
            {t("toolbar.all")}
          </button>
          <HScrollRow count={enabledProfiles.length} maxVisible={4} className="gap-0.5">
          {enabledProfiles.map((profile) => {
            const isActive = agentFilter === profile.id;
            return (
              <button
                key={profile.id}
                onClick={() =>
                  onAgentFilterChange(
                    agentFilter === profile.id ? null : profile.id
                  )
                }
                title={profile.display_name}
                aria-pressed={isActive}
                className={cn(
                  "relative h-full w-7 shrink-0 flex items-center justify-center rounded-md cursor-pointer z-10 focus-ring",
                  !isActive && "hover:bg-sidebar-hover"
                )}
              >
                <div className={cn(
                  "absolute inset-0 bg-accent rounded-md -z-10 [backface-visibility:hidden]",
                  isActive ? "opacity-100" : "opacity-0"
                )} />
                <AgentIcon
                  profile={profile}
                  className={cn(
                    agentIconCls(profile.icon),
                    "transition duration-200",
                    isActive
                      ? "drop-shadow-sm scale-[1.1]"
                      : "opacity-60 hover:opacity-90 grayscale-0"
                  )}
                />
              </button>
            );
          })}
          </HScrollRow>
        </div>
      )}

      {/* Source type filter (Hub / Local) */}
      {onSourceFilterChange && (localCount ?? 0) > 0 && (
        <div className="flex items-center gap-0.5 border border-border rounded-lg overflow-hidden h-8 p-0.5 bg-sidebar/30 shrink-0">
          {(["all", "hub", "local"] as const).map((f) => {
            const isActive = sourceFilter === f;
            return (
              <button
                key={f}
                onClick={() => onSourceFilterChange(f)}
                aria-pressed={isActive}
                className={cn(
                  "relative h-full px-2.5 flex items-center justify-center rounded-md text-xs font-medium cursor-pointer whitespace-nowrap z-10 focus-ring",
                  isActive
                    ? "text-accent-foreground"
                    : "text-muted-foreground hover:text-foreground hover:bg-sidebar-hover"
                )}
              >
                <div className={cn(
                  "absolute inset-0 bg-accent rounded-md -z-10 [backface-visibility:hidden]",
                  isActive ? "opacity-100" : "opacity-0"
                )} />
                {f === "all" ? t("toolbar.all") : f === "hub" ? "Hub" : "Local"}
              </button>
            );
          })}
        </div>
      )}

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
            "flex items-center justify-center w-8 h-8 rounded-lg border border-border/80 bg-background/50 shadow-sm backdrop-blur-md text-foreground/80 hover:text-foreground hover:bg-accent/10 hover:border-accent/50 transition duration-200 cursor-pointer group shrink-0 focus-ring",
            (isRefreshing || cooldown) && "opacity-50 cursor-not-allowed"
          )}
          title={t("common.refresh", { defaultValue: "Refresh" })}
        >
          {isRefreshing ? (
            <Loader2 className="w-3.5 h-3.5 animate-spin text-accent-foreground" />
          ) : (
            <RefreshCw className={cn("w-3.5 h-3.5 text-muted-foreground group-hover:text-accent-foreground transition-colors", cooldown && "opacity-40")} />
          )}
        </button>
      )}

      {countText && (
        <div className="h-8 px-3 flex items-center justify-center rounded-lg border border-border/70 bg-background/50 shadow-sm text-xs font-medium text-foreground/80 tabular-nums whitespace-nowrap shrink-0">
          {countText}
        </div>
      )}

      {onToggleUpdateOnly && (
        <button
          onClick={onToggleUpdateOnly}
          className={cn(
            "h-8 px-3 text-xs font-medium rounded-lg border transition duration-200 cursor-pointer flex items-center gap-1.5 shadow-sm whitespace-nowrap shrink-0 focus-ring",
            showUpdateOnly
              ? "bg-accent text-accent-foreground border-accent shadow-[0_0_8px_rgba(var(--color-primary-rgb),0.1)]"
              : "border-border/80 bg-background/50 text-foreground/80 hover:text-foreground hover:bg-accent/10 hover:border-accent/50",
            shouldAnimateUpdateOnly &&
              "border-warning/50 text-warning-foreground bg-warning/10 shadow-[0_0_14px_rgba(var(--color-warning-rgb),0.2)]"
          )}
        >
          {shouldAnimateUpdateOnly && (
            <span className="relative flex h-2 w-2 shrink-0">
              <span className="animate-ping-limited absolute inline-flex h-full w-full rounded-full bg-warning opacity-75"></span>
              <span className="relative inline-flex h-2 w-2 rounded-full bg-warning"></span>
            </span>
          )}
          {t("toolbar.updateOnly")}
          {hasPendingUpdates && (
            <span
              className={cn(
                "min-w-[1.25rem] h-[1.125rem] px-1 rounded-full text-[11px] leading-[1.125rem] text-center tabular-nums",
                shouldAnimateUpdateOnly
                  ? "bg-warning/20 text-warning-foreground"
                  : "bg-muted text-muted-foreground"
              )}
            >
              {pendingUpdateCount}
            </span>
          )}
        </button>
      )}

      {/* Sort pills */}
      <div className="flex items-center gap-0.5 border border-border rounded-lg overflow-hidden h-8 p-0.5 bg-sidebar/30 shadow-sm ml-auto shrink-0">
        {sortOptions.map((opt) => {
          const isActive = sortBy === opt.value;
          return (
            <button
              key={opt.value}
              onClick={() => onSortChange(opt.value)}
              aria-pressed={isActive}
              className={cn(
                "relative h-full px-3 flex items-center justify-center rounded-md text-xs font-medium cursor-pointer whitespace-nowrap z-10 focus-ring",
                isActive
                  ? "text-accent-foreground"
                  : "text-muted-foreground hover:text-foreground hover:bg-sidebar-hover"
              )}
            >
              <div className={cn(
                "absolute inset-0 bg-accent rounded-md -z-10 [backface-visibility:hidden]",
                isActive ? "opacity-100" : "opacity-0"
              )} />
              {opt.label}
            </button>
          );
        })}
      </div>

      {/* View toggle */}
      <div className="flex items-center gap-0.5 border border-border rounded-lg overflow-hidden h-8 p-0.5 bg-sidebar/30 shadow-sm">
        <button
          onClick={() => onViewModeChange("grid")}
          aria-label="Grid view"
          className={cn(
            "relative h-full w-8 flex items-center justify-center rounded-md cursor-pointer z-10 focus-ring",
            viewMode === "grid"
              ? "text-accent-foreground"
              : "text-muted-foreground hover:text-foreground hover:bg-sidebar-hover"
          )}
        >
          <div className={cn(
            "absolute inset-0 bg-accent rounded-md -z-10 [backface-visibility:hidden]",
            viewMode === "grid" ? "opacity-100" : "opacity-0"
          )} />
          <svg width="14" height="14" viewBox="0 0 14 14" fill="currentColor"><rect x="1" y="1" width="5" height="5" rx="1"/><rect x="8" y="1" width="5" height="5" rx="1"/><rect x="1" y="8" width="5" height="5" rx="1"/><rect x="8" y="8" width="5" height="5" rx="1"/></svg>
        </button>
        <button
          onClick={() => onViewModeChange("list")}
          aria-label="List view"
          className={cn(
            "relative h-full w-8 flex items-center justify-center rounded-md cursor-pointer z-10 focus-ring",
            viewMode === "list"
              ? "text-accent-foreground"
              : "text-muted-foreground hover:text-foreground hover:bg-sidebar-hover"
          )}
        >
          <div className={cn(
            "absolute inset-0 bg-accent rounded-md -z-10 [backface-visibility:hidden]",
            viewMode === "list" ? "opacity-100" : "opacity-0"
          )} />
          <svg width="14" height="14" viewBox="0 0 14 14" fill="currentColor"><rect x="1" y="2" width="12" height="2" rx="0.5"/><rect x="1" y="6" width="12" height="2" rx="0.5"/><rect x="1" y="10" width="12" height="2" rx="0.5"/></svg>
        </button>
      </div>
    </div>
  );
}
