import { useTranslation } from "react-i18next";
import type { AgentProfile } from "../../types";
import { agentIconCls, cn } from "../../lib/utils";
import { AgentIcon } from "./AgentIcon";
import { HScrollRow } from "./HScrollRow";

/** Minimal shape needed to render a filterable icon entry. */
export type AgentFilterItem = Pick<AgentProfile, "id" | "icon" | "display_name">;

interface AgentFilterPillProps {
  /** Icon entries rendered after the "All" button. */
  items: AgentFilterItem[];
  /** Currently selected id, or null for "All". */
  value: string | null;
  /** Fired with an id to select, or null to reset to "All". */
  onChange: (id: string | null) => void;
  /** Label for the leading reset button (defaults to i18n `toolbar.all`). */
  allLabel?: string;
  /** Max icons visible before horizontal scroll kicks in (default 4). */
  maxVisible?: number;
  className?: string;
}

/**
 * Segmented "All + per-icon" filter pill. Extracted from the skills Toolbar so
 * every mode (skills, MCP, …) shares one interactive filter affordance instead
 * of re-implementing the segmented control. Clicking an icon toggles it as the
 * active filter; clicking it again (or "All") clears the filter.
 */
export function AgentFilterPill({ items, value, onChange, allLabel, maxVisible = 4, className }: AgentFilterPillProps) {
  const { t } = useTranslation();

  if (items.length === 0) return null;

  return (
    <div
      className={cn(
        "flex items-center gap-0 border border-border rounded-lg overflow-hidden h-8 p-0.5 bg-sidebar/30 shrink-0",
        className,
      )}
    >
      <button
        type="button"
        onClick={() => onChange(null)}
        aria-pressed={value === null}
        className={cn(
          "relative h-full px-2.5 flex items-center justify-center rounded-md text-xs font-medium cursor-pointer whitespace-nowrap z-10 shrink-0 focus-ring",
          value === null
            ? "text-accent-foreground"
            : "text-muted-foreground hover:text-foreground hover:bg-sidebar-hover",
        )}
      >
        <div
          className={cn(
            "absolute inset-0 bg-accent rounded-md -z-10 [backface-visibility:hidden]",
            value === null ? "opacity-100" : "opacity-0",
          )}
        />
        {allLabel ?? t("toolbar.all")}
      </button>
      <HScrollRow count={items.length} maxVisible={maxVisible} className="gap-0.5">
        {items.map((item) => {
          const isActive = value === item.id;
          return (
            <button
              key={item.id}
              type="button"
              onClick={() => onChange(isActive ? null : item.id)}
              title={item.display_name}
              aria-pressed={isActive}
              className={cn(
                "relative h-full w-7 shrink-0 flex items-center justify-center rounded-md cursor-pointer z-10 focus-ring",
                !isActive && "hover:bg-sidebar-hover",
              )}
            >
              <div
                className={cn(
                  "absolute inset-0 bg-accent rounded-md -z-10 [backface-visibility:hidden]",
                  isActive ? "opacity-100" : "opacity-0",
                )}
              />
              <AgentIcon
                profile={item}
                className={cn(
                  agentIconCls(item.icon),
                  "transition duration-200",
                  isActive ? "drop-shadow-sm scale-[1.1]" : "opacity-60 hover:opacity-90 grayscale-0",
                )}
              />
            </button>
          );
        })}
      </HScrollRow>
    </div>
  );
}
