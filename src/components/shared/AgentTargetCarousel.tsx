import { useTranslation } from "react-i18next";
import type { AgentProfile } from "../../types";
import { agentIconCls, cn } from "../../lib/utils";
import { AgentIcon } from "../ui/AgentIcon";
import { HScrollRow } from "../ui/HScrollRow";

export type AgentTargetSelection = boolean | "mixed";

export interface AgentTargetCarouselItem {
  /** Consumer-owned target id: an Agent profile id or an MCP tool id. */
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
 * Callers project capability-specific availability before rendering. Settings
 * `enabled` is painted on the SVG: a just-disabled Agent that is still attached
 * to this resource stays in the row as a stopped (grey, non-interactive) icon
 * so start/stop is visible on the card. Operable targets still require `enabled`.
 */
export function AgentTargetCarousel<T extends AgentTargetCarouselItem>({
  items,
  onToggle,
  className,
}: AgentTargetCarouselProps<T>) {
  const { t } = useTranslation();
  if (items.length === 0) return null;

  return (
    <HScrollRow count={items.length} itemWidth={28} gap={6} maxVisible={4} className={cn("min-w-0 gap-1.5", className)}>
      {items.map((item) => {
        const stopped = !item.profile.enabled;
        const active = !stopped && item.selected === true;
        const partial = !stopped && item.selected === "mixed";
        const disabled = stopped || item.pending || item.disabled;
        const title = stopped ? t("common.agentStoppedHint", { name: item.profile.display_name }) : item.title;

        return (
          <button
            key={item.id}
            type="button"
            onClick={(event) => {
              event.stopPropagation();
              if (stopped) return;
              onToggle(item);
            }}
            disabled={disabled}
            aria-label={title}
            aria-pressed={stopped ? false : item.selected}
            aria-busy={item.pending || undefined}
            title={title}
            className={cn(
              "relative flex h-7 w-7 shrink-0 items-center justify-center rounded-lg border transition-[background-color,border-color,box-shadow,transform,filter,opacity] duration-200 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-primary/45 focus-visible:ring-offset-1 focus-visible:ring-offset-background",
              stopped
                ? "cursor-not-allowed border-border/50 bg-muted/40 grayscale opacity-45"
                : "cursor-pointer active:scale-[0.96] disabled:cursor-wait disabled:active:scale-100",
              !stopped &&
                (active
                  ? "border-primary/40 bg-primary/10 shadow-[0_0_0_1px_rgba(var(--color-primary-rgb),0.15)] hover:bg-primary/20 hover:shadow-[0_0_0_1px_rgba(var(--color-primary-rgb),0.3)]"
                  : partial
                    ? "border-warning/30 bg-warning/5"
                    : "border-transparent bg-transparent hover:bg-muted"),
              item.pending && "opacity-65",
            )}
          >
            <AgentIcon
              profile={item.profile}
              className={cn(
                agentIconCls(item.profile.icon, "w-4 h-4"),
                "drop-shadow-sm transition-[filter,opacity]",
                item.pending && "animate-pulse",
                stopped && "grayscale",
                !stopped && !active && !partial && "grayscale opacity-40 hover:opacity-70 hover:grayscale-0",
              )}
            />
          </button>
        );
      })}
    </HScrollRow>
  );
}
