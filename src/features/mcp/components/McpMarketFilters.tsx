import { ArrowDownWideNarrow, ArrowUpWideNarrow, FilterX, SlidersHorizontal } from "lucide-react";
import { useTranslation } from "react-i18next";
import { Button } from "../../../components/ui/button";
import { Input } from "../../../components/ui/input";
import { cn } from "../../../lib/utils";
import type { McpServerKind, McpSortKey } from "../../../types";
import { activeMcpFilterCount, type McpMarketFilterState, toggleFilterValue } from "../lib/marketQuery";

/**
 * Filter + sort controls for the catalog browse.
 *
 * Every control here maps 1:1 onto a column the snapshot can filter or order
 * on, so the whole panel compiles into one backend query rather than trimming a
 * page that was already fetched.
 */

const KINDS: McpServerKind[] = ["stdio", "remote", "both"];

/**
 * Runner commands offered as one-click filters. Not an exhaustive list of what
 * the catalog contains — it is the short head of runners a user recognises;
 * anything else is still reachable through search.
 */
const RUNTIMES = ["npx", "uvx", "docker", "dnx", "bunx"];

const LICENSES = ["MIT", "Apache-2.0", "BSD-3-Clause", "GPL-3.0", "AGPL-3.0"];

const SORTS: McpSortKey[] = ["default", "stars", "name", "updated", "published"];

interface McpMarketFiltersProps {
  filters: McpMarketFilterState;
  onChange: (next: McpMarketFilterState) => void;
  onReset: () => void;
  className?: string;
}

function Chip({
  active,
  onClick,
  children,
  title,
}: {
  active: boolean;
  onClick: () => void;
  children: React.ReactNode;
  title?: string;
}) {
  return (
    <button
      type="button"
      onClick={onClick}
      title={title}
      className={cn(
        "rounded-md border px-2 py-1 text-[11px] font-medium transition-colors",
        active
          ? "border-primary/60 bg-primary/10 text-primary"
          : "border-border/70 bg-background/50 text-muted-foreground hover:bg-muted/40 hover:text-foreground",
      )}
    >
      {children}
    </button>
  );
}

function Group({ label, children }: { label: string; children: React.ReactNode }) {
  return (
    <div className="space-y-1.5">
      <p className="text-micro font-semibold uppercase tracking-wider text-muted-foreground">{label}</p>
      <div className="flex flex-wrap gap-1.5">{children}</div>
    </div>
  );
}

/** Parse a stars input; empty means "no bound", not zero. */
function parseStars(raw: string): number | null {
  const digits = raw.replace(/[^0-9]/g, "");
  return digits ? Number(digits) : null;
}

export function McpMarketFilters({ filters, onChange, onReset, className }: McpMarketFiltersProps) {
  const { t } = useTranslation();
  const activeCount = activeMcpFilterCount(filters);

  const patch = (next: Partial<McpMarketFilterState>) => onChange({ ...filters, ...next });

  return (
    <div className={cn("space-y-4 rounded-xl border border-border/60 bg-background/40 p-3.5", className)}>
      <div className="flex items-center justify-between gap-2">
        <p className="flex items-center gap-1.5 text-xs font-semibold text-foreground">
          <SlidersHorizontal className="h-3.5 w-3.5 text-primary" />
          {t("mcp.filtersTitle")}
        </p>
        <Button
          type="button"
          variant="ghost"
          size="sm"
          className="h-7 gap-1 px-2 text-[11px] text-muted-foreground"
          onClick={onReset}
          disabled={activeCount === 0}
        >
          <FilterX className="h-3 w-3" />
          {t("mcp.filtersClear", { count: activeCount })}
        </Button>
      </div>

      <Group label={t("mcp.filterKind")}>
        {KINDS.map((kind) => (
          <Chip
            key={kind}
            active={filters.kinds.includes(kind)}
            onClick={() => patch({ kinds: toggleFilterValue(filters.kinds, kind) })}
          >
            {t(`mcp.kind_${kind}`)}
          </Chip>
        ))}
      </Group>

      <Group label={t("mcp.filterRuntime")}>
        {RUNTIMES.map((runtime) => (
          <Chip
            key={runtime}
            active={filters.runtimes.includes(runtime)}
            onClick={() => patch({ runtimes: toggleFilterValue(filters.runtimes, runtime) })}
          >
            <span className="font-mono">{runtime}</span>
          </Chip>
        ))}
      </Group>

      <Group label={t("mcp.filterLicense")}>
        {LICENSES.map((license) => (
          <Chip
            key={license}
            active={filters.licenses.includes(license)}
            onClick={() => patch({ licenses: toggleFilterValue(filters.licenses, license) })}
          >
            {license}
          </Chip>
        ))}
      </Group>

      <Group label={t("mcp.filterStatus")}>
        <Chip
          active={filters.recommendedOnly}
          onClick={() => patch({ recommendedOnly: !filters.recommendedOnly })}
          title={t("mcp.filterRecommendedHint")}
        >
          {t("mcp.filterRecommended")}
        </Chip>
        <Chip
          active={filters.latestOnly}
          onClick={() => patch({ latestOnly: !filters.latestOnly })}
          title={t("mcp.filterLatestOnlyHint")}
        >
          {t("mcp.filterLatestOnly")}
        </Chip>
        <Chip
          active={filters.includeDeprecated}
          onClick={() => patch({ includeDeprecated: !filters.includeDeprecated })}
          title={t("mcp.filterIncludeDeprecatedHint")}
        >
          {t("mcp.filterIncludeDeprecated")}
        </Chip>
      </Group>

      <Group label={t("mcp.filterStars")}>
        <div className="flex w-full items-center gap-2">
          <Input
            value={filters.minStars ?? ""}
            onChange={(event) => patch({ minStars: parseStars(event.target.value) })}
            inputMode="numeric"
            placeholder={t("mcp.filterStarsMin")}
            className="h-8 flex-1 font-mono text-xs"
          />
          <span className="text-xs text-muted-foreground">—</span>
          <Input
            value={filters.maxStars ?? ""}
            onChange={(event) => patch({ maxStars: parseStars(event.target.value) })}
            inputMode="numeric"
            placeholder={t("mcp.filterStarsMax")}
            className="h-8 flex-1 font-mono text-xs"
          />
        </div>
      </Group>

      <Group label={t("mcp.filterSort")}>
        {SORTS.map((sort) => (
          <Chip key={sort} active={filters.sort === sort} onClick={() => patch({ sort })}>
            {t(`mcp.sort_${sort}`)}
          </Chip>
        ))}
        <Chip
          active={filters.descending != null}
          onClick={() => patch({ descending: filters.descending == null ? true : filters.descending ? false : null })}
          title={t("mcp.filterDirectionHint")}
        >
          {filters.descending === false ? (
            <ArrowUpWideNarrow className="h-3 w-3" />
          ) : (
            <ArrowDownWideNarrow className="h-3 w-3" />
          )}
          {filters.descending == null
            ? t("mcp.sortDirectionNatural")
            : filters.descending
              ? t("mcp.sortDirectionDesc")
              : t("mcp.sortDirectionAsc")}
        </Chip>
      </Group>
    </div>
  );
}
