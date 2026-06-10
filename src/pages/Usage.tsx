import { UsagePanel } from "../features/usage/components/UsagePanel";
import type { CatalogFilter } from "../features/usage/types";

interface UsageProps {
  filter: CatalogFilter;
  usageCreateRequest: { nonce: number; preselectCatalogId: string | null } | null;
  clearUsageCreateRequest: () => void;
}

export function Usage({ filter, usageCreateRequest, clearUsageCreateRequest }: UsageProps) {
  return (
    <div className="flex-1 min-w-0 flex flex-col overflow-hidden">
      <UsagePanel
        filter={filter}
        usageCreateRequest={usageCreateRequest}
        clearUsageCreateRequest={clearUsageCreateRequest}
      />
    </div>
  );
}
