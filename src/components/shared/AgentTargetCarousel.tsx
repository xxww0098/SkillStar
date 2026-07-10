import type { AgentProfile } from "../../types";
import { agentIconCls, cn } from "../../lib/utils";
import { AgentIcon } from "../ui/AgentIcon";
import { HScrollRow } from "../ui/HScrollRow";

export type AgentTargetSelection = boolean | "mixed";

export interface AgentTargetCarouselItem {
  /** Consumer-owned target id: an Agent profile id or an MCP tool id. */
  id: string;
  profile: Pick<AgentProfile, "id" | "icon" | "display_name">;
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
 * Shared card rail for toggling one resource across targetable Agents.
 * Availability is projected before rendering; this component owns only the
 * consistent icon-button states and horizontal overflow behavior.
 */
export function AgentTargetCarousel<T extends AgentTargetCarouselItem>({
  items,
  onToggle,
  className,
}: AgentTargetCarouselProps<T>) {
  if (items.length === 0) return null;

  return (
    <HScrollRow count={items.length} itemWidth={28} gap={6} className={cn("min-w-0 gap-1.5", className)}>
      {items.map((item) => {
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
              "flex h-7 w-7 shrink-0 cursor-pointer items-center justify-center rounded-lg border transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-primary/45 focus-visible:ring-offset-1 focus-visible:ring-offset-background disabled:cursor-wait",
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
