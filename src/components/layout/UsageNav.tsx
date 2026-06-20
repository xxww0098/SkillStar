import { LayoutGrid, Search } from "lucide-react";
import { useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { ProviderLogo } from "@/features/usage/components/ProviderLogo";
import { useUsageDataContext } from "@/features/usage/context/UsageDataContext";
import { FILTER_ALL, type CatalogEntry, type CatalogFilter } from "@/features/usage/types";
import { cn } from "@/lib/utils";

export interface UsageNavProps {
  selected: CatalogFilter;
  onSelect: (filter: CatalogFilter) => void;
  collapsed: boolean;
}

function filterCatalog(catalog: CatalogEntry[], query: string): CatalogEntry[] {
  if (!query.trim()) return catalog;
  const lower = query.toLowerCase();
  return catalog.filter(
    (entry) =>
      entry.display_name.toLowerCase().includes(lower) ||
      entry.description.toLowerCase().includes(lower) ||
      entry.id.toLowerCase().includes(lower),
  );
}

export function UsageNav({ selected, onSelect, collapsed }: UsageNavProps) {
  const { t } = useTranslation();
  const { catalog, subscriptions } = useUsageDataContext();
  const [searchQuery, setSearchQuery] = useState("");

  const counts = useMemo(() => {
    const map = new Map<string, number>();
    for (const sub of subscriptions) {
      map.set(sub.catalog_id, (map.get(sub.catalog_id) ?? 0) + 1);
    }
    return map;
  }, [subscriptions]);

  const filteredCatalog = useMemo(() => filterCatalog(catalog, searchQuery), [catalog, searchQuery]);
  const totalCount = subscriptions.length;

  return (
    <div className="flex h-full min-h-0 flex-col">
      <div className={cn("mb-1 shrink-0", collapsed ? "px-1.5" : "px-0")}>
        <NavItem
          label={t("usage.allSubscriptions", "全部订阅")}
          count={totalCount}
          selected={selected === FILTER_ALL}
          onClick={() => onSelect(FILTER_ALL)}
          collapsed={collapsed}
          logo={
            collapsed ? (
              <LayoutGrid className="h-4 w-4 shrink-0" aria-hidden />
            ) : (
              <LayoutGrid className="h-3.5 w-3.5 shrink-0 text-muted-foreground" aria-hidden />
            )
          }
        />
      </div>

      {!collapsed && (
        <div className="mb-2 shrink-0 px-2">
          <div className="relative">
            <input
              type="text"
              value={searchQuery}
              onChange={(e) => setSearchQuery(e.target.value)}
              placeholder={t("usage.searchCatalog", "搜索订阅商...")}
              className="h-7 w-full rounded-md border border-border/50 bg-muted/30 pl-7 pr-2 text-[12px] text-foreground transition placeholder:text-muted-foreground/60 focus:border-primary/50 focus:outline-none focus:ring-1 focus:ring-primary/40"
            />
            <Search
              aria-hidden
              className="pointer-events-none absolute left-2 top-1/2 h-3 w-3 -translate-y-1/2 text-muted-foreground/60"
            />
          </div>
        </div>
      )}

      <div className="min-h-0 flex-1 overflow-y-auto">
        {filteredCatalog.length === 0 && searchQuery.trim() ? (
          <div className="px-3 py-4 text-center">
            <p className="text-[11px] text-muted-foreground">{t("usage.noCatalogMatch", "无匹配结果")}</p>
          </div>
        ) : (
          <div className="flex flex-col gap-0.5">
            {filteredCatalog.map((entry) => (
              <NavItem
                key={entry.id}
                label={entry.display_name}
                description={collapsed ? undefined : entry.description}
                count={counts.get(entry.id) ?? 0}
                selected={selected === entry.id}
                onClick={() => onSelect(entry.id)}
                collapsed={collapsed}
                logo={
                  <ProviderLogo
                    catalogId={entry.id}
                    displayName={entry.display_name}
                    brandColor={entry.brand_color}
                    size="sm"
                  />
                }
              />
            ))}
          </div>
        )}
      </div>
    </div>
  );
}

interface NavItemProps {
  label: string;
  description?: string;
  count: number;
  selected: boolean;
  onClick: () => void;
  collapsed: boolean;
  logo?: React.ReactNode;
}

function NavItem({ label, description, count, selected, onClick, collapsed, logo }: NavItemProps) {
  if (collapsed) {
    return (
      <button
        type="button"
        onClick={onClick}
        title={label}
        aria-current={selected ? "page" : undefined}
        className={cn(
          "relative mx-auto mb-1 flex h-9 w-9 items-center justify-center rounded-lg transition duration-150 cursor-pointer focus-ring",
          selected ? "bg-primary/10 text-primary ring-1 ring-primary/25" : "text-muted-foreground hover:bg-muted/40",
        )}
      >
        {logo ?? <span className="text-[11px] font-semibold">{label.charAt(0)}</span>}
        {count > 0 && (
          <span className="absolute -top-0.5 -right-0.5 flex h-3.5 min-w-[14px] items-center justify-center rounded-full bg-primary px-0.5 text-[8px] font-bold text-primary-foreground">
            {count > 9 ? "9+" : count}
          </span>
        )}
      </button>
    );
  }

  return (
    <button
      type="button"
      onClick={onClick}
      aria-current={selected ? "page" : undefined}
      className={cn(
        "group mb-0.5 flex w-full items-center gap-2 rounded-lg px-2 py-1.5 text-left transition duration-150 cursor-pointer focus-ring",
        selected
          ? "bg-primary/10 font-medium text-primary ring-1 ring-primary/25"
          : "text-muted-foreground hover:bg-muted/30",
      )}
    >
      {logo}
      <div className="min-w-0 flex-1">
        <div className="truncate text-[12px]">{label}</div>
        {description && <div className="truncate text-[10px] text-muted-foreground/70">{description}</div>}
      </div>
      {count > 0 && (
        <span
          className={cn(
            "inline-flex h-5 min-w-[20px] shrink-0 items-center justify-center rounded-full px-1 text-[10px] font-semibold tabular-nums",
            selected ? "bg-primary/20 text-primary" : "bg-muted/60 text-muted-foreground",
          )}
        >
          {count}
        </span>
      )}
    </button>
  );
}
