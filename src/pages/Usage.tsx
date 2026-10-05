import { UsagePanel } from "../features/usage/components/UsagePanel";
import type { CatalogFilter } from "../features/usage/types";

interface UsageProps {
  filter: CatalogFilter;
}

export function Usage({ filter }: UsageProps) {
  return (
    <div className="flex-1 min-w-0 flex flex-col overflow-hidden">
      <UsagePanel filter={filter} />
    </div>
  );
}
