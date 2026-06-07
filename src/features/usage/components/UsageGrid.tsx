import { Reorder, useDragControls } from "framer-motion";
import { useMemo } from "react";
import { mergeSubscriptionOrder } from "../lib/pricing";
import { FILTER_ALL, type CatalogEntry, type CatalogFilter, type Subscription } from "../types";
import { SubscriptionCard } from "./SubscriptionCard";
import { UsageHomeEmpty } from "./UsageHomeEmpty";
import { VendorPlaceholderCard } from "./VendorPlaceholderCard";

interface UsageGridProps {
  subscriptions: Subscription[];
  allSubscriptions: Subscription[];
  catalog: CatalogEntry[];
  filter: CatalogFilter;
  onRefresh: (id: string) => Promise<void>;
  refreshDisabled?: boolean;
  onEdit: (id: string) => void;
  onDelete: (id: string) => void;
  onReauth?: (id: string) => void;
  onSetActive?: (id: string) => Promise<void>;
  onReorder: (orderedIds: string[]) => void;
  onAddNew: (catalogId?: string) => void;
  onBrowseProviders?: () => void;
}

type CardCallbacks = Pick<
  UsageGridProps,
  "onRefresh" | "onEdit" | "onDelete" | "onReauth" | "onSetActive" | "refreshDisabled"
>;

export function UsageGrid({
  subscriptions,
  allSubscriptions,
  catalog,
  filter,
  onRefresh,
  refreshDisabled = false,
  onEdit,
  onDelete,
  onReauth,
  onSetActive,
  onReorder,
  onAddNew,
  onBrowseProviders,
}: UsageGridProps) {
  const catalogById = useMemo(() => new Map(catalog.map((c) => [c.id, c])), [catalog]);
  const isHomeView = filter === FILTER_ALL;

  const providerEntry = useMemo(() => {
    if (isHomeView) return null;
    return catalog.find((c) => c.id === filter) ?? null;
  }, [catalog, filter, isHomeView]);

  const orderedVisible = useMemo(() => [...subscriptions].sort((a, b) => a.sort_index - b.sort_index), [subscriptions]);

  const handleReorder = (reordered: Subscription[]) => {
    const movableIds = orderedVisible.map((s) => s.id);
    const newMovableOrder = reordered.map((s) => s.id);
    const allIds = allSubscriptions.map((s) => s.id);
    onReorder(mergeSubscriptionOrder(allIds, movableIds, newMovableOrder));
  };

  const cardCallbacks: CardCallbacks = {
    onRefresh,
    refreshDisabled,
    onEdit,
    onDelete,
    onReauth,
    onSetActive,
  };

  const gridClass = "grid gap-3 [grid-template-columns:repeat(auto-fill,minmax(280px,1fr))]";

  const renderReorderGrid = () => (
    <Reorder.Group axis="y" layoutScroll values={orderedVisible} onReorder={handleReorder} className={gridClass}>
      {orderedVisible.map((sub) => (
        <DraggableSubscriptionCard
          key={sub.id}
          subscription={sub}
          catalog={catalogById.get(sub.catalog_id)}
          {...cardCallbacks}
        />
      ))}
    </Reorder.Group>
  );

  return (
    <div className="flex min-h-0 flex-1 flex-col overflow-y-auto p-4">
      {isHomeView ? (
        subscriptions.length === 0 ? (
          <UsageHomeEmpty onBrowseProviders={onBrowseProviders ?? (() => undefined)} />
        ) : (
          renderReorderGrid()
        )
      ) : providerEntry ? (
        <div className="flex flex-1 flex-col items-start">
          {subscriptions.length === 0 ? (
            <VendorPlaceholderCard entry={providerEntry} onClick={() => onAddNew(providerEntry.id)} />
          ) : (
            <div className="w-full">{renderReorderGrid()}</div>
          )}
        </div>
      ) : null}
    </div>
  );
}

function DraggableSubscriptionCard({
  subscription,
  catalog,
  ...callbacks
}: {
  subscription: Subscription;
  catalog: CatalogEntry | undefined;
} & CardCallbacks) {
  const dragControls = useDragControls();

  return (
    <Reorder.Item
      value={subscription}
      dragListener={false}
      dragControls={dragControls}
      className="list-none"
      whileDrag={{ scale: 1.02, zIndex: 30 }}
    >
      <SubscriptionCard
        subscription={subscription}
        catalog={catalog}
        onDragHandlePointerDown={(e) => dragControls.start(e)}
        {...callbacks}
      />
    </Reorder.Item>
  );
}
