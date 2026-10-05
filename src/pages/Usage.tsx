import { UsagePanel } from "../features/usage/components/UsagePanel";
import type { CatalogFilter } from "../features/usage/types";

interface UsageProps {
  filter: CatalogFilter;
  usageCreateRequest: { nonce: number; preselectCatalogId: string | null } | null;
  clearUsageCreateRequest: () => void;
  /** Cross-view navigation (Usage → Models triangle): open one agent's model routes. */
  /** Cross-view navigation (Usage → Models triangle): which agents route to this catalog. */
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
