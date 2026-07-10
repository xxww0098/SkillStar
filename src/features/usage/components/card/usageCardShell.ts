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
    "group relative flex min-h-[320px] w-full shrink-0 flex-col overflow-hidden rounded-3xl border bg-white/95 backdrop-blur-xl",
    "border-zinc-200/80 shadow-[0_8px_30px_rgba(0,0,0,0.03)] transition-all duration-300",
    "hover:border-zinc-300 hover:shadow-[0_10px_34px_rgba(var(--brand-rgb),0.14)]",
    "sm:w-[280px]",
    state.isActive && "border-emerald-400/60 ring-1 ring-emerald-300/40",
    state.requiresReauth && "border-red-500/40 ring-1 ring-red-500/20",
    state.priorityClass,
    state.className,
  );
}
