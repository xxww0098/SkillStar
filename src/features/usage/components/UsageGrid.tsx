import { Reorder, useDragControls } from "framer-motion";
import { ChevronDown, ListCollapse, ListTree, Plus } from "lucide-react";
import { memo, useCallback, useMemo, useState, type PointerEvent, type ReactNode } from "react";
import { useTranslation } from "react-i18next";
import { Button } from "@/components/ui/button";
import { cn } from "@/lib/utils";
import { mergeSubscriptionOrder } from "../lib/pricing";
import { FILTER_ALL, type CatalogEntry, type CatalogFilter, type CliAccountState, type Subscription } from "../types";
import { ProviderLogo } from "./ProviderLogo";
import { SubscriptionCard } from "./SubscriptionCard";
import { UsageHomeEmpty } from "./UsageHomeEmpty";
import { VendorPlaceholderCard } from "./VendorPlaceholderCard";

interface UsageGridProps {
  subscriptions: Subscription[];
  allSubscriptions: Subscription[];
  catalog: CatalogEntry[];
  /** `catalog_id -> which account that CLI is actually serving`; each card
   *  draws its "current" badge from this rather than from the pin. */
  cliAccounts?: Record<string, CliAccountState>;
  hideAccountEmails?: boolean;
  filter: CatalogFilter;
  onRefresh: (id: string) => Promise<void>;
  /** Consume one real provider-side Grok reset credit for a subscription. */
  onResetQuota?: (id: string) => Promise<void>;
  refreshDisabled?: boolean;
  onEdit: (id: string) => void;
  onDelete: (id: string) => void;
  onReauth?: (id: string) => void;
  onSetActive?: (id: string) => Promise<void>;
  /** Re-push the active account's credentials to its CLI config (retry). */
  onSwitchToCli?: (catalogId: string) => Promise<void>;
  onReorder: (orderedIds: string[]) => void;
  onAddNew: (catalogId?: string) => void;
  onBrowseProviders?: () => void;
}

type CardCallbacks = Pick<
  UsageGridProps,
  | "onRefresh"
  | "onResetQuota"
  | "onEdit"
  | "onDelete"
  | "onReauth"
  | "onSetActive"
  | "onSwitchToCli"
  | "refreshDisabled"
  | "cliAccounts"
>;

interface ProviderGroup {
  catalogId: string;
  entry: CatalogEntry | undefined;
  subscriptions: Subscription[];
  sortIndex: number;
}

const NOOP_BROWSE = () => undefined;

export function UsageGrid({
  subscriptions,
  allSubscriptions,
  catalog,
  cliAccounts,
  hideAccountEmails = false,
  filter,
  onRefresh,
  onResetQuota,
  refreshDisabled = false,
  onEdit,
  onDelete,
  onReauth,
  onSetActive,
  onSwitchToCli,
  onReorder,
  onAddNew,
  onBrowseProviders,
}: UsageGridProps) {
  const { t } = useTranslation();
  const catalogById = useMemo(() => new Map(catalog.map((c) => [c.id, c])), [catalog]);
  const isHomeView = filter === FILTER_ALL;

  const providerEntry = useMemo(() => {
    if (isHomeView) return null;
    return catalog.find((c) => c.id === filter) ?? null;
  }, [catalog, filter, isHomeView]);

  const orderedVisible = useMemo(() => [...subscriptions].sort((a, b) => a.sort_index - b.sort_index), [subscriptions]);

  const providerGroups = useMemo(() => {
    const groups = new Map<string, ProviderGroup>();
    for (const sub of orderedVisible) {
      const existing = groups.get(sub.catalog_id);
      if (existing) {
        existing.subscriptions.push(sub);
        existing.sortIndex = Math.min(existing.sortIndex, sub.sort_index);
        continue;
      }
      groups.set(sub.catalog_id, {
        catalogId: sub.catalog_id,
        entry: catalogById.get(sub.catalog_id),
        subscriptions: [sub],
        sortIndex: sub.sort_index,
      });
    }
    return Array.from(groups.values()).sort((a, b) => a.sortIndex - b.sortIndex);
  }, [catalogById, orderedVisible]);
  const [collapsedGroups, setCollapsedGroups] = useState<Record<string, boolean>>({});
  const allProviderGroupsCollapsed =
    providerGroups.length > 0 && providerGroups.every((group) => collapsedGroups[group.catalogId] ?? false);

  const toggleGroup = (catalogId: string) => {
    setCollapsedGroups((current) => ({
      ...current,
      [catalogId]: !current[catalogId],
    }));
  };

  const toggleAllProviderGroups = () => {
    const nextCollapsed = !allProviderGroupsCollapsed;
    setCollapsedGroups(Object.fromEntries(providerGroups.map((group) => [group.catalogId, nextCollapsed])));
  };

  const handleReorder = (movableItems: Subscription[], reordered: Subscription[]) => {
    const movableIds = movableItems.map((s) => s.id);
    const newMovableOrder = reordered.map((s) => s.id);
    const allIds = allSubscriptions.map((s) => s.id);
    onReorder(mergeSubscriptionOrder(allIds, movableIds, newMovableOrder));
  };

  const cardCallbacks: CardCallbacks = useMemo(
    () => ({
      onRefresh,
      onResetQuota,
      refreshDisabled,
      onEdit,
      onDelete,
      onReauth,
      onSetActive,
      onSwitchToCli,
      cliAccounts,
    }),
    [onRefresh, onResetQuota, refreshDisabled, onEdit, onDelete, onReauth, onSetActive, onSwitchToCli, cliAccounts],
  );

  const gridClass = "grid gap-3 [grid-template-columns:repeat(auto-fill,minmax(280px,1fr))]";

  const renderReorderCards = (items: Subscription[], axis: "x" | "y", className: string, itemClassName?: string) => (
    <Reorder.Group
      axis={axis}
      layoutScroll
      values={items}
      onReorder={(reordered) => handleReorder(items, reordered)}
      className={className}
    >
      {items.map((sub) => (
        <DraggableSubscriptionCard
          key={sub.id}
          subscription={sub}
          catalog={catalogById.get(sub.catalog_id)}
          hideAccountEmails={hideAccountEmails}
          itemClassName={itemClassName}
          {...cardCallbacks}
        />
      ))}
    </Reorder.Group>
  );

  const renderReorderGrid = () => renderReorderCards(orderedVisible, "y", gridClass);

  const renderProviderRows = () => (
    <div className="flex min-w-0 flex-col gap-2">
      <div className="flex min-w-0 justify-end px-1">
        <Button
          type="button"
          size="sm"
          variant="outline"
          onClick={toggleAllProviderGroups}
          className="h-7 shrink-0 gap-1.5 px-2 text-[11px]"
        >
          {allProviderGroupsCollapsed ? <ListTree className="h-3.5 w-3.5" /> : <ListCollapse className="h-3.5 w-3.5" />}
          {allProviderGroupsCollapsed
            ? t("usage.expandAllProviderGroups", "全部展开")
            : t("usage.collapseAllProviderGroups", "全部折叠")}
        </Button>
      </div>
      <div className="flex min-w-0 flex-col">
        {providerGroups.map((group) => (
          <ProviderSubscriptionRow
            key={group.catalogId}
            group={group}
            collapsed={collapsedGroups[group.catalogId] ?? false}
            onToggle={() => toggleGroup(group.catalogId)}
          >
            {renderReorderCards(
              group.subscriptions,
              "x",
              "flex min-w-0 gap-3 overflow-x-auto pb-2 pr-1 [scrollbar-gutter:stable]",
              "w-[min(100%,280px)] shrink-0",
            )}
          </ProviderSubscriptionRow>
        ))}
      </div>
    </div>
  );

  return (
    <div className="flex min-h-0 flex-1 flex-col overflow-y-auto p-4">
      {isHomeView ? (
        subscriptions.length === 0 ? (
          <UsageHomeEmpty onBrowseProviders={onBrowseProviders ?? NOOP_BROWSE} />
        ) : (
          renderProviderRows()
        )
      ) : providerEntry ? (
        <div className="flex flex-1 flex-col items-start">
          {subscriptions.length === 0 ? (
            <VendorPlaceholderCard entry={providerEntry} onClick={() => onAddNew(providerEntry.id)} />
          ) : (
            <div className="flex w-full min-w-0 flex-col gap-3">
              <div className="flex w-full items-center justify-between gap-3">
                <div className="flex min-w-0 items-center gap-2">
                  <ProviderLogo
                    catalogId={providerEntry.id}
                    displayName={providerEntry.display_name}
                    brandColor={providerEntry.brand_color}
                    size="sm"
                  />
                  <div className="min-w-0">
                    <p className="truncate text-sm font-semibold text-foreground">{providerEntry.display_name}</p>
                    <p className="text-[11px] text-muted-foreground">
                      {t("usage.providerSubscriptionCount", { count: subscriptions.length })}
                    </p>
                  </div>
                </div>
                <div className="flex shrink-0 items-center gap-1.5">
                  <Button
                    type="button"
                    size="sm"
                    variant="outline"
                    onClick={() => onAddNew(providerEntry.id)}
                    className="max-w-[min(240px,55%)] shrink-0 overflow-hidden"
                  >
                    <Plus className="h-3.5 w-3.5" />
                    <span className="truncate">
                      {t("usage.addProviderSubscription", { provider: providerEntry.display_name })}
                    </span>
                  </Button>
                </div>
              </div>
              {renderReorderGrid()}
            </div>
          )}
        </div>
      ) : null}
    </div>
  );
}

function ProviderSubscriptionRow({
  group,
  collapsed,
  onToggle,
  children,
}: {
  group: ProviderGroup;
  collapsed: boolean;
  onToggle: () => void;
  children: ReactNode;
}) {
  const { t } = useTranslation();
  const displayName = group.entry?.display_name ?? group.catalogId;
  const brandColor = group.entry?.brand_color ?? "6B7280";
  const countLabel = t("usage.providerSubscriptionCount", { count: group.subscriptions.length });
  const groupId = `usage-provider-group-${group.catalogId}`;

  return (
    <section
      aria-label={`${displayName} ${countLabel}`}
      className="border-b border-border/45 py-3 first:pt-0 last:border-b-0"
    >
      <div className="flex w-full items-center gap-1.5">
        <button
          type="button"
          aria-expanded={!collapsed}
          aria-controls={groupId}
          onClick={onToggle}
          className={cn(
            "flex min-w-0 flex-1 items-center gap-2.5 rounded-xl px-1 py-1 text-left transition-colors focus-ring",
            "hover:bg-muted/20",
          )}
        >
          <ProviderLogo
            catalogId={group.catalogId}
            displayName={displayName}
            brandColor={brandColor}
            size="sm"
            className="shrink-0"
          />
          <div className="min-w-0 flex-1">
            <p className="truncate text-sm font-semibold text-foreground">{displayName}</p>
            <p className="truncate text-[11px] text-muted-foreground">{countLabel}</p>
          </div>
          <ChevronDown
            className={cn(
              "h-4 w-4 shrink-0 text-muted-foreground transition-transform duration-200",
              collapsed && "-rotate-90",
            )}
          />
        </button>
      </div>
      {!collapsed && (
        <div id={groupId} className="mt-3 min-w-0">
          {children}
        </div>
      )}
    </section>
  );
}

const DraggableSubscriptionCard = memo(function DraggableSubscriptionCard({
  subscription,
  catalog,
  hideAccountEmails,
  itemClassName,
  ...callbacks
}: {
  subscription: Subscription;
  catalog: CatalogEntry | undefined;
  hideAccountEmails: boolean;
  itemClassName?: string;
} & CardCallbacks) {
  const dragControls = useDragControls();
  const onDragHandlePointerDown = useCallback(
    (event: PointerEvent) => {
      dragControls.start(event);
    },
    [dragControls],
  );

  return (
    <Reorder.Item
      value={subscription}
      dragListener={false}
      dragControls={dragControls}
      className={cn("list-none flex", itemClassName)}
      whileDrag={{ scale: 1.02, zIndex: 30 }}
    >
      <SubscriptionCard
        subscription={subscription}
        catalog={catalog}
        hideAccountEmails={hideAccountEmails}
        onDragHandlePointerDown={onDragHandlePointerDown}
        {...callbacks}
      />
    </Reorder.Item>
  );
});
