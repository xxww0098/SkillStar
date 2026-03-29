import { useState, useCallback } from "react";
import { Search, RefreshCw, Loader2, Sparkles, Download } from "lucide-react";
import { useTranslation } from "react-i18next";
import { Input } from "../ui/input";
import { cn } from "../../lib/utils";
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
  /** Optional callback for Import from File (.agentskill) button */
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
  /** Optional title node to render at the start of the toolbar */
  titleNode?: React.ReactNode;
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
  titleNode,
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
    <div className="h-14 flex items-center gap-3 px-6 border-b border-border bg-card/30 backdrop-blur-sm overflow-x-auto [&::-webkit-scrollbar]:hidden">
      {titleNode && (
        <div className="flex items-center shrink-0 h-8 whitespace-nowrap">
          {titleNode}
          <div className="w-px h-5 ml-4 mr-1 bg-border" />
        </div>
      )}
      
      <div className="relative w-56 shrink-0">
        <Search className="absolute left-2.5 top-1/2 -translate-y-1/2 w-3.5 h-3.5 text-muted-foreground" />
        <Input
          value={searchQuery}
          onChange={(e) => onSearchChange(e.target.value)}
          placeholder={t("toolbar.searchPlaceholder")}
          className="pl-8 h-8 text-xs bg-sidebar/50 focus-visible:bg-background"
        />
      </div>

      {/* AI Pick Skills */}
      {onAiPick && (
        <button
          onClick={onAiPick}
          className="flex items-center h-8 px-3 rounded-lg border border-ai-border bg-transparent text-[12px] font-medium text-ai-text hover:text-ai-text-hover hover:bg-ai-bg-hover hover:border-ai-border-hover transition-all duration-300 cursor-pointer shadow-[0_0_8px_var(--color-ai-shadow)] gap-1.5 whitespace-nowrap shrink-0"
        >
          <Sparkles className="w-3.5 h-3.5 shrink-0" />
          {t("toolbar.aiPick")}
        </button>
      )}

      {/* Agent filter */}
      {enabledProfiles.length > 0 && onAgentFilterChange && (
        <div className="flex items-center gap-0.5 border border-border rounded-lg overflow-hidden backdrop-blur-sm h-8 p-0.5 bg-sidebar/30 shrink-0">
          <button
            onClick={() => onAgentFilterChange(null)}
            className={cn(
              "h-full px-2.5 flex items-center justify-center rounded-md text-[11px] font-medium transition-all duration-200 cursor-pointer whitespace-nowrap",
              agentFilter === null
                ? "bg-accent text-accent-foreground shadow-[0_0_8px_rgba(59,130,246,0.1)]"
                : "text-muted-foreground hover:text-foreground hover:bg-sidebar-hover"
            )}
          >
            {t("toolbar.all")}
          </button>
          {enabledProfiles.map((profile) => (
            <button
              key={profile.id}
              onClick={() =>
                onAgentFilterChange(
                  agentFilter === profile.id ? null : profile.id
                )
              }
              title={profile.display_name}
              className={cn(
                "h-full w-7 flex items-center justify-center rounded-md transition-all duration-200 cursor-pointer",
                agentFilter === profile.id
                  ? "bg-accent shadow-[0_0_8px_rgba(59,130,246,0.1)]"
                  : "hover:bg-sidebar-hover"
              )}
            >
              <img
                src={`/${profile.icon}`}
                alt={profile.display_name}
                className={cn(
                  "w-3.5 h-3.5 transition-all duration-200",
                  agentFilter === profile.id
                    ? "drop-shadow-sm scale-[1.1]"
                    : "opacity-60 hover:opacity-90 grayscale-0"
                )}
              />
            </button>
          ))}
        </div>
      )}

      {/* Import */}
      {onImport && (
        <button
          onClick={onImport}
          className="flex items-center h-8 px-3 gap-1.5 rounded-lg border border-border/80 bg-background/50 shadow-sm backdrop-blur-md text-[12px] font-medium text-foreground/80 hover:text-foreground hover:bg-accent/10 hover:border-accent/50 transition-all duration-200 cursor-pointer group whitespace-nowrap shrink-0"
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
            "flex items-center justify-center w-8 h-8 rounded-lg border border-border/80 bg-background/50 shadow-sm backdrop-blur-md text-foreground/80 hover:text-foreground hover:bg-accent/10 hover:border-accent/50 transition-all duration-200 cursor-pointer group shrink-0",
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
        <div className="h-8 px-3 flex items-center justify-center rounded-lg border border-border/70 bg-background/50 shadow-sm text-[12px] font-medium text-foreground/80 tabular-nums whitespace-nowrap shrink-0">
          {countText}
        </div>
      )}

      {onToggleUpdateOnly && (
        <button
          onClick={onToggleUpdateOnly}
          className={cn(
            "h-8 px-3 text-[12px] font-medium rounded-lg border transition-all duration-200 cursor-pointer flex items-center gap-1.5 shadow-sm whitespace-nowrap shrink-0",
            showUpdateOnly
              ? "bg-accent text-accent-foreground border-accent shadow-[0_0_8px_rgba(59,130,246,0.1)]"
              : "border-border/80 bg-background/50 text-foreground/80 hover:text-foreground hover:bg-accent/10 hover:border-accent/50",
            shouldAnimateUpdateOnly &&
              "border-warning/50 text-warning-foreground bg-warning/10 shadow-[0_0_14px_rgba(245,158,11,0.2)]"
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
                "min-w-[1.25rem] h-4 px-1 rounded-full text-[10px] leading-4 text-center tabular-nums",
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
      <div className="flex items-center gap-0.5 border border-border rounded-lg overflow-hidden backdrop-blur-sm h-8 p-0.5 bg-sidebar/30 shadow-sm ml-auto shrink-0">
        {sortOptions.map((opt) => (
          <button
            key={opt.value}
            onClick={() => onSortChange(opt.value)}
            className={cn(
              "h-full px-3 flex items-center justify-center rounded-md text-[11px] font-medium transition-all duration-200 cursor-pointer whitespace-nowrap",
              sortBy === opt.value
                ? "bg-accent text-accent-foreground shadow-[0_0_8px_rgba(59,130,246,0.1)]"
                : "text-muted-foreground hover:text-foreground hover:bg-sidebar-hover"
            )}
          >
            {opt.label}
          </button>
        ))}
      </div>

      {/* View toggle */}
      <div className="flex items-center gap-0.5 border border-border rounded-lg overflow-hidden backdrop-blur-sm h-8 p-0.5 bg-sidebar/30 shadow-sm">
        <button
          onClick={() => onViewModeChange("grid")}
          className={cn(
            "h-full w-8 flex items-center justify-center rounded-md transition-all duration-200 cursor-pointer",
            viewMode === "grid"
              ? "bg-accent text-accent-foreground shadow-[0_0_8px_rgba(59,130,246,0.1)]"
              : "text-muted-foreground hover:text-foreground hover:bg-sidebar-hover"
          )}
        >
          <svg width="14" height="14" viewBox="0 0 14 14" fill="currentColor"><rect x="1" y="1" width="5" height="5" rx="1"/><rect x="8" y="1" width="5" height="5" rx="1"/><rect x="1" y="8" width="5" height="5" rx="1"/><rect x="8" y="8" width="5" height="5" rx="1"/></svg>
        </button>
        <button
          onClick={() => onViewModeChange("list")}
          className={cn(
            "h-full w-8 flex items-center justify-center rounded-md transition-all duration-200 cursor-pointer",
            viewMode === "list"
              ? "bg-accent text-accent-foreground shadow-[0_0_8px_rgba(59,130,246,0.1)]"
              : "text-muted-foreground hover:text-foreground hover:bg-sidebar-hover"
          )}
        >
          <svg width="14" height="14" viewBox="0 0 14 14" fill="currentColor"><rect x="1" y="2" width="12" height="2" rx="0.5"/><rect x="1" y="6" width="12" height="2" rx="0.5"/><rect x="1" y="10" width="12" height="2" rx="0.5"/></svg>
        </button>
      </div>
    </div>
  );
}
