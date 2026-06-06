import { useMemo } from "react";
import { FILTER_ALL, type CatalogEntry, type CatalogFilter, type Subscription } from "../types";
import { SubscriptionCard } from "./SubscriptionCard";
import { UsageHomeEmpty } from "./UsageHomeEmpty";
import { VendorPlaceholderCard } from "./VendorPlaceholderCard";

interface UsageGridProps {
  subscriptions: Subscription[];
  catalog: CatalogEntry[];
  filter: CatalogFilter;
  onRefresh: (id: string) => Promise<void>;
  refreshDisabled?: boolean;
  onEdit: (id: string) => void;
  onDelete: (id: string) => void;
  onReauth?: (id: string) => void;
  /** Switch a subscription to be the active account for its catalog. */
  onSetActive?: (id: string) => Promise<void>;
  onAddNew: (catalogId?: string) => void;
  /** Focus sidebar so the user can pick a provider to bind (home empty state). */
  onBrowseProviders?: () => void;
}

export function UsageGrid({
  subscriptions,
  catalog,
  filter,
  onRefresh,
  refreshDisabled = false,
  onEdit,
  onDelete,
  onReauth,
  onSetActive,
  onAddNew,
  onBrowseProviders,
}: UsageGridProps) {
  const catalogById = useMemo(() => new Map(catalog.map((c) => [c.id, c])), [catalog]);
  const isHomeView = filter === FILTER_ALL;

  const providerEntry = useMemo(() => {
    if (isHomeView) return null;
    return catalog.find((c) => c.id === filter) ?? null;
  }, [catalog, filter, isHomeView]);

  return (
    <div className="flex min-h-0 flex-1 flex-col overflow-y-auto p-4">
      {isHomeView ? (
        <>
          {subscriptions.length === 0 ? (
            <UsageHomeEmpty onBrowseProviders={onBrowseProviders ?? (() => undefined)} />
          ) : (
            <div className="grid gap-3 [grid-template-columns:repeat(auto-fill,minmax(280px,1fr))]">
              {subscriptions.map((sub) => (
                <SubscriptionCard
                  key={sub.id}
                  subscription={sub}
                  catalog={catalogById.get(sub.catalog_id)}
                  onRefresh={onRefresh}
                  refreshDisabled={refreshDisabled}
                  onEdit={onEdit}
                  onDelete={onDelete}
                  onReauth={onReauth}
                  onSetActive={onSetActive}
                />
              ))}
            </div>
          )}
        </>
      ) : providerEntry ? (
        <div className="flex flex-1 flex-col items-start">
          {subscriptions.length === 0 ? (
            <VendorPlaceholderCard entry={providerEntry} onClick={() => onAddNew(providerEntry.id)} />
          ) : (
            <div className="grid w-full gap-3 [grid-template-columns:repeat(auto-fill,minmax(280px,1fr))]">
              {subscriptions.map((sub) => (
                <SubscriptionCard
                  key={sub.id}
                  subscription={sub}
                  catalog={catalogById.get(sub.catalog_id)}
                  onRefresh={onRefresh}
                  refreshDisabled={refreshDisabled}
                  onEdit={onEdit}
                  onDelete={onDelete}
                  onReauth={onReauth}
                  onSetActive={onSetActive}
                />
              ))}
            </div>
          )}
        </div>
      ) : null}
    </div>
  );
}
