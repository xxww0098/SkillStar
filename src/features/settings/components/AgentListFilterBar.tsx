import { X } from "lucide-react";
import { useTranslation } from "react-i18next";
import { SearchInput } from "../../../components/ui/SearchInput";
import { cn } from "../../../lib/utils";
import { AGENT_STATUS_FILTERS, type AgentStatusCounts, type AgentStatusFilter } from "../lib/agentFilters";

const STATUS_LABEL_KEY: Record<AgentStatusFilter, string> = {
  all: "settings.filterAgentsAll",
  enabled: "settings.filterAgentsEnabled",
  disabled: "settings.filterAgentsDisabled",
};

interface AgentListFilterBarProps {
  query: string;
  status: AgentStatusFilter;
  /** Status counts computed on the text-searched set, so they track the query. */
  counts: AgentStatusCounts;
  onQueryChange: (query: string) => void;
  onStatusChange: (status: AgentStatusFilter) => void;
}

/**
 * Search + activation-status narrowing for the Settings agent list. Kept next to
 * the list header so the filter state reads as part of the same card.
 */
export function AgentListFilterBar({ query, status, counts, onQueryChange, onStatusChange }: AgentListFilterBarProps) {
  const { t } = useTranslation();
  const searchLabel = t("settings.searchAgents");

  return (
    <div className="flex flex-wrap items-center gap-2 border-b border-border/70 px-4 py-2.5 sm:px-5">
      <SearchInput
        value={query}
        onChange={(event) => onQueryChange(event.target.value)}
        placeholder={searchLabel}
        aria-label={searchLabel}
        containerClassName="min-w-[8rem] flex-1"
        className="h-8 bg-sidebar/40 text-xs focus-visible:bg-background"
        iconClassName="left-2.5"
        suffix={
          query ? (
            <button
              type="button"
              onClick={() => onQueryChange("")}
              aria-label={t("settings.clearAgentSearch")}
              title={t("settings.clearAgentSearch")}
              className="flex h-6 w-6 cursor-pointer items-center justify-center rounded-md text-muted-foreground transition-colors hover:bg-muted hover:text-foreground focus-ring"
            >
              <X className="h-3 w-3" />
            </button>
          ) : undefined
        }
      />

      <div
        role="group"
        aria-label={t("settings.filterAgentsByStatus")}
        className="flex h-8 shrink-0 items-center gap-0.5 rounded-lg border border-border bg-sidebar/30 p-0.5"
      >
        {AGENT_STATUS_FILTERS.map((option) => {
          const isActive = status === option;
          return (
            <button
              key={option}
              type="button"
              onClick={() => onStatusChange(option)}
              aria-pressed={isActive}
              className={cn(
                "relative z-10 flex h-full cursor-pointer items-center gap-1 rounded-md px-2.5 text-xs font-medium whitespace-nowrap focus-ring",
                isActive
                  ? "text-accent-foreground"
                  : "text-muted-foreground hover:bg-sidebar-hover hover:text-foreground",
              )}
            >
              <div
                className={cn(
                  "absolute inset-0 -z-10 rounded-md bg-accent [backface-visibility:hidden]",
                  isActive ? "opacity-100" : "opacity-0",
                )}
              />
              {t(STATUS_LABEL_KEY[option])}
              <span className="text-micro tabular-nums text-muted-foreground/80">{counts[option]}</span>
            </button>
          );
        })}
      </div>
    </div>
  );
}
