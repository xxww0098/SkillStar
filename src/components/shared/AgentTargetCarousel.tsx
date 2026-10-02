import type { AgentProfile } from "../../types";
import { agentIconCls, cn } from "../../lib/utils";
import { AgentIcon } from "../ui/AgentIcon";
import { HScrollRow } from "../ui/HScrollRow";

export type AgentTargetSelection = boolean | "mixed";

export interface AgentTargetCarouselItem {
  /** Consumer-owned target id: e.g. an Agent profile id. */
  id: string;
  profile: Pick<AgentProfile, "id" | "icon" | "display_name" | "enabled">;
  /** Resource-local selection; independent from the Settings enabled flag. */
  selected: AgentTargetSelection;
  title: string;
  pending?: boolean;
  disabled?: boolean;
}

interface AgentTargetCarouselProps<T extends AgentTargetCarouselItem> {
  items: readonly T[];
  onToggle: (item: T) => void;
  className?: string;
}

/**
 * Shared card rail for toggling one resource across Agents.
 * Callers project capability-specific availability before rendering. The rail
 * only paints Settings-enabled profiles: a disabled Agent does not take a slot,
 * even when the resource is still attached. Operable targets still require
 * `enabled`.
 */
export function AgentTargetCarousel<T extends AgentTargetCarouselItem>({
  items,
  onToggle,
  className,
}: AgentTargetCarouselProps<T>) {
  const visible = items.filter((item) => item.profile.enabled);
  if (visible.length === 0) return null;

  return (
    // Fill the card footer: the track stretches to the wrapper's free width
    // so the scroll arrows pin to its far edges, while the icons themselves
    // keep their fixed spacing (start-anchored, never spread apart).
    // No fixed visible-icon cap: narrow cards scroll, wide cards show all.
    <HScrollRow
      count={visible.length}
      itemWidth={28}
      gap={6}
      className={cn("min-w-0 w-full flex-1 gap-1.5", className)}
    >
      {visible.map((item) => {
        const active = item.selected === true;
        const partial = item.selected === "mixed";
        const disabled = item.pending || item.disabled;

        return (
          <button
            key={item.id}
            type="button"
            onClick={(event) => {
              event.stopPropagation();
              onToggle(item);
            }}
            disabled={disabled}
            aria-label={item.title}
            aria-pressed={item.selected}
            aria-busy={item.pending || undefined}
            title={item.title}
            className={cn(
              "relative flex h-7 w-7 shrink-0 items-center justify-center rounded-lg border transition-[background-color,border-color,box-shadow,transform,filter,opacity] duration-200 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-primary/45 focus-visible:ring-offset-1 focus-visible:ring-offset-background",
              "cursor-pointer active:scale-[0.96] disabled:cursor-wait disabled:active:scale-100",
              active
                ? "border-primary/40 bg-primary/10 shadow-[0_0_0_1px_rgba(var(--color-primary-rgb),0.15)] hover:bg-primary/20 hover:shadow-[0_0_0_1px_rgba(var(--color-primary-rgb),0.3)]"
                : partial
                  ? "border-warning/30 bg-warning/5"
                  : "border-transparent bg-transparent hover:bg-muted",
              item.pending && "opacity-65",
            )}
          >
            <AgentIcon
              profile={item.profile}
              className={cn(
                agentIconCls(item.profile.icon, "w-4 h-4"),
                "drop-shadow-sm transition-[filter,opacity]",
                item.pending && "animate-pulse",
                !active && !partial && "grayscale opacity-40 hover:opacity-70 hover:grayscale-0",
              )}
            />
          </button>
        );
      })}
    </HScrollRow>
  );
}
