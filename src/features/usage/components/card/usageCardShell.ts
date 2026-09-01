import { cn } from "@/lib/utils";

/**
 * Frozen shell class tokens for subscription / placeholder cards.
 * Callers own the element type (`motion.article` | `button`) — see design K14.
 *
 * @see docs/design-usage-card.md §2 Shell API
 */
export function usageCardShellClassName(state: {
  isActive?: boolean;
  requiresReauth?: boolean;
  priorityClass?: string | false | null;
  className?: string;
}): string {
  return cn(
    "group relative flex w-full min-w-0 shrink-0 flex-col overflow-hidden rounded-3xl border bg-white/95 backdrop-blur-xl",
    "border-zinc-200/80 shadow-[0_8px_30px_rgba(0,0,0,0.03)]",
    "motion-safe:transition-[border-color,box-shadow] motion-safe:duration-300 motion-reduce:transition-none",
    "hover:border-zinc-300 hover:shadow-[0_10px_34px_rgba(var(--brand-rgb),0.14)]",
    "focus-within:border-zinc-300 focus-within:shadow-[0_10px_34px_rgba(var(--brand-rgb),0.12)]",
    state.isActive && "border-emerald-400/60 ring-1 ring-emerald-300/40",
    state.requiresReauth && "border-red-500/40 ring-1 ring-red-500/20",
    state.priorityClass,
    state.className,
  );
}

/**
 * Slot rhythm shared by SubscriptionCard and VendorPlaceholderCard.
 *
 * Identity band is the recognizer; meta is a hairline under it; quota leads;
 * footer is a toolbelt (type, not nested KPI tiles).
 */
export const usageCardSlotClassName = {
  headerBand: "relative overflow-hidden px-4 pt-3.5 pb-3",
  meta: "space-y-1 border-b border-zinc-100/80 px-4 py-1.5",
  body: "relative z-10 flex min-h-0 flex-1 flex-col space-y-2 overflow-y-auto px-4 pt-2.5 pb-1.5",
  footer: "relative z-10 flex items-center justify-end border-t border-zinc-100 bg-zinc-50/50 px-4 py-2",
} as const;
