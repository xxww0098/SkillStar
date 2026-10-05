import type { ReactNode } from "react";
import { cn } from "@/lib/utils";

/**
 * The hub's one segmented primitive: a pill track, quiet options, and a
 * raised white cell for the pressed option. Used for listen mode, routing,
 * and affinity so those three controls share weight and rhythm.
 */
export function SegmentedControl<T extends string>({
  options,
  value,
  onSelect,
  mono = false,
  ariaLabel,
}: {
  options: readonly { id: T; label: ReactNode }[];
  value: T | "";
  onSelect: (id: T) => void;
  /** Model refs and mode words stay mono; localized labels stay sans. */
  mono?: boolean;
  ariaLabel?: string;
}) {
  return (
    <div
      role="group"
      aria-label={ariaLabel}
      className="inline-flex flex-wrap items-center gap-0.5 rounded-lg border border-border/50 bg-muted/50 p-0.5"
    >
      {options.map((option) => {
        const active = value === option.id;
        return (
          <button
            key={option.id}
            type="button"
            aria-pressed={active}
            onClick={() => onSelect(option.id)}
            className={cn(
              "rounded-[7px] px-2.5 py-1 text-[11px] leading-4 transition-colors",
              mono && "font-mono tabular-nums",
              active
                ? "bg-background font-medium text-foreground shadow-[0_1px_2px_rgba(15,23,42,0.08)] ring-1 ring-border/60"
                : "text-muted-foreground hover:text-foreground",
            )}
          >
            {option.label}
          </button>
        );
      })}
    </div>
  );
}
