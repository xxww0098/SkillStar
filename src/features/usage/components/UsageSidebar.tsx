import { Plus } from "lucide-react";
import { Button } from "@/components/ui/button";
import { cn } from "@/lib/utils";
import { type CatalogEntry, type CatalogFilter, FILTER_ALL, type Subscription } from "../types";
import { ProviderLogo } from "./ProviderLogo";

interface UsageSidebarProps {
  catalog: CatalogEntry[];
  subscriptions: Subscription[];
  selected: CatalogFilter;
  onSelect: (filter: CatalogFilter) => void;
  onAddNew: () => void;
}

export function UsageSidebar({ catalog, subscriptions, selected, onSelect, onAddNew }: UsageSidebarProps) {
  const totalCount = subscriptions.length;
  const counts = new Map<string, number>();
  for (const sub of subscriptions) {
    counts.set(sub.catalog_id, (counts.get(sub.catalog_id) ?? 0) + 1);
  }

  return (
    <aside className="hidden md:flex w-[200px] shrink-0 flex-col border-r border-border/60 bg-sidebar/60 backdrop-blur-sm">
      <div className="p-3">
        <SidebarItem
          label="全部"
          count={totalCount}
          selected={selected === FILTER_ALL}
          onClick={() => onSelect(FILTER_ALL)}
        />
      </div>
      <div className="border-t border-border/40" />
      <nav className="flex-1 overflow-y-auto px-3 py-2 space-y-0.5" aria-label="供应商">
        {catalog.map((entry) => (
          <SidebarItem
            key={entry.id}
            label={entry.display_name}
            description={entry.description}
            count={counts.get(entry.id) ?? 0}
            selected={selected === entry.id}
            onClick={() => onSelect(entry.id)}
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
      </nav>
      <div className="border-t border-border/40 p-2">
        <Button onClick={onAddNew} size="sm" className="w-full justify-start gap-2">
          <Plus className="w-3.5 h-3.5" />
          新增订阅
        </Button>
      </div>
    </aside>
  );
}

interface SidebarItemProps {
  label: string;
  description?: string;
  count: number;
  selected: boolean;
  onClick: () => void;
  logo?: React.ReactNode;
}

function SidebarItem({ label, description, count, selected, onClick, logo }: SidebarItemProps) {
  return (
    <button
      type="button"
      onClick={onClick}
      className={cn(
        "group flex w-full items-center gap-2 rounded-md px-2 py-1.5 text-left transition-colors",
        selected
          ? "bg-primary/10 text-foreground ring-1 ring-primary/30"
          : "text-muted-foreground hover:bg-accent/10 hover:text-foreground",
      )}
    >
      {logo}
      <div className="min-w-0 flex-1">
        <div className="truncate text-[12px] font-medium">{label}</div>
        {description && <div className="truncate text-[10px] text-muted-foreground/70">{description}</div>}
      </div>
      {count > 0 && (
        <span
          className={cn(
            "inline-flex h-5 min-w-[20px] items-center justify-center rounded-full px-1 text-[10px] font-semibold tabular-nums",
            selected ? "bg-primary/20 text-primary-foreground/90" : "bg-muted/60 text-muted-foreground",
          )}
        >
          {count}
        </span>
      )}
    </button>
  );
}
