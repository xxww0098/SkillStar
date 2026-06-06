import { ChevronDown } from "lucide-react";
import { memo, type ElementType, type ReactNode } from "react";
import { cn } from "../../../../lib/utils";

export const providerCardClass =
  "overflow-hidden rounded-xl border border-border/60 bg-card/55 shadow-sm backdrop-blur-sm";

export const fieldLabelClass = "text-xs font-medium text-muted-foreground";

export interface ConfigCollapseSectionProps {
  id: string;
  title: string;
  summary?: string;
  expanded: boolean;
  onToggle: () => void;
  children: ReactNode;
  icon?: ElementType;
  iconSlot?: ReactNode;
  headerAction?: ReactNode;
  /** Less padding for nested sections */
  nested?: boolean;
}

export const ConfigCollapseSection = memo(function ConfigCollapseSection({
  id,
  icon: Icon,
  iconSlot,
  title,
  summary,
  expanded,
  onToggle,
  headerAction,
  children,
  nested = false,
}: ConfigCollapseSectionProps) {
  const contentId = `${id}-content`;

  return (
    <div className={cn(providerCardClass, nested && "border-border/45 bg-card/40 shadow-none")}>
      <button
        type="button"
        onClick={onToggle}
        aria-expanded={expanded}
        aria-controls={contentId}
        aria-label={expanded ? `折叠 ${title}` : `展开 ${title}`}
        className="flex w-full cursor-pointer items-center gap-2.5 px-3.5 py-2.5 text-left transition-colors hover:bg-muted/25"
      >
        {iconSlot ? (
          iconSlot
        ) : Icon ? (
          <span className="flex h-7 w-7 shrink-0 items-center justify-center rounded-lg border border-primary/15 bg-primary/10 text-primary">
            <Icon className="h-3.5 w-3.5" />
          </span>
        ) : null}
        <div className="min-w-0 flex-1">
          <h3 className="text-sm font-semibold text-foreground">{title}</h3>
          {!expanded && summary && <p className="mt-0.5 truncate text-[11px] text-muted-foreground">{summary}</p>}
        </div>
        {headerAction && (
          <span className="shrink-0" onClick={(e) => e.stopPropagation()} onKeyDown={(e) => e.stopPropagation()}>
            {headerAction}
          </span>
        )}
        <ChevronDown
          className={cn(
            "h-4 w-4 shrink-0 text-muted-foreground transition-transform duration-200",
            !expanded && "-rotate-90",
          )}
        />
      </button>

      {expanded && (
        <div id={contentId} className="space-y-3 border-t border-border/50 px-3.5 pb-3.5 pt-3">
          {children}
        </div>
      )}
    </div>
  );
});
