import { Wallet } from "lucide-react";
import { EmptyState } from "@/components/ui/EmptyState";
import type { CatalogEntry, Subscription } from "../types";
import { SubscriptionCard } from "./SubscriptionCard";

interface UsageGridProps {
  subscriptions: Subscription[];
  catalog: CatalogEntry[];
  onRefresh: (id: string) => Promise<void>;
  onEdit: (id: string) => void;
  onReauth?: (id: string) => void;
  onAddNew: () => void;
}

export function UsageGrid({ subscriptions, catalog, onRefresh, onEdit, onReauth, onAddNew }: UsageGridProps) {
  if (subscriptions.length === 0) {
    return (
      <div className="flex-1 flex items-center justify-center p-8">
        <EmptyState
          icon={<Wallet className="w-7 h-7 text-muted-foreground" />}
          title="还没有订阅"
          description="从左侧选择一家供应商，点 + 新增订阅；也可以直接录入手动订阅。"
          action={
            <button
              type="button"
              onClick={onAddNew}
              className="rounded-md bg-primary px-4 py-2 text-sm font-medium text-primary-foreground hover:bg-primary/90"
            >
              新增第一个订阅
            </button>
          }
        />
      </div>
    );
  }

  const byId = new Map(catalog.map((c) => [c.id, c]));

  return (
    <div className="flex-1 overflow-y-auto p-4">
      <div className="grid gap-3 [grid-template-columns:repeat(auto-fill,minmax(240px,1fr))]">
        {subscriptions.map((sub) => (
          <SubscriptionCard
            key={sub.id}
            subscription={sub}
            catalog={byId.get(sub.catalog_id)}
            onRefresh={onRefresh}
            onEdit={onEdit}
            onReauth={onReauth}
          />
        ))}
      </div>
    </div>
  );
}
