import { useTranslation } from "react-i18next";
import { Skeleton } from "@/components/ui/Skeleton";

const CARD_COUNT = 6;

/**
 * Card-shaped placeholders matching the usage grid
 * (auto-fill, minmax 280px, padding p-3).
 *
 * Painted before catalog, subscription, and session reads resolve so a mode
 * switch is not a blank pane or a single loading line.
 */
export function UsageGridSkeleton() {
  const { t } = useTranslation();
  return (
    <div
      role="status"
      aria-live="polite"
      aria-busy="true"
      aria-label={t("usage.loading")}
      className="min-h-0 flex-1 overflow-y-auto p-3"
    >
      <span className="sr-only">{t("usage.loading")}</span>
      <div className="grid gap-2.5 [grid-template-columns:repeat(auto-fill,minmax(280px,1fr))]">
        {Array.from({ length: CARD_COUNT }, (_, index) => (
          <UsageCardSkeleton key={index} />
        ))}
      </div>
    </div>
  );
}

/** Full usage pane, used while the lazy page chunk itself is still loading. */
export function UsagePageSkeleton() {
  return (
    <div className="flex h-full min-h-0 flex-1 flex-col overflow-hidden">
      <div className="flex h-12 shrink-0 items-center gap-3 border-b border-border/70 bg-sidebar px-4">
        <Skeleton className="h-8 w-8 rounded-lg" />
        <div className="space-y-1.5">
          <Skeleton className="h-3.5 w-16" />
          <Skeleton className="h-2.5 w-28" />
        </div>
        <div className="flex-1" />
        <Skeleton className="size-8 rounded-md" />
        <Skeleton className="h-8 w-24 rounded-md" />
      </div>
      <UsageGridSkeleton />
    </div>
  );
}

function UsageCardSkeleton() {
  return (
    <div className="flex min-h-[220px] flex-col rounded-3xl border border-zinc-200/80 bg-white/95 p-4 shadow-[0_8px_30px_rgba(0,0,0,0.03)]">
      <div className="flex items-center gap-2.5">
        <Skeleton className="size-8 rounded-lg" />
        <div className="space-y-1.5">
          <Skeleton className="h-3.5 w-28" />
          <Skeleton className="h-2.5 w-16" />
        </div>
      </div>
      <Skeleton className="mt-4 h-2.5 w-24" />
      <Skeleton className="mt-3 h-2 w-full rounded-full" />
      <Skeleton className="mt-2 h-2 w-4/5 rounded-full" />
      <Skeleton className="mt-2 h-2 w-2/3 rounded-full" />
      <div className="mt-auto flex justify-end pt-4">
        <Skeleton className="h-6 w-16 rounded-md" />
      </div>
    </div>
  );
}
