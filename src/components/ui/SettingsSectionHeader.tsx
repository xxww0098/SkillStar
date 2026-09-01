import type { ReactNode } from "react";
import { cn } from "../../lib/utils";

/**
 * Shared heading for Settings sections.
 *
 * Every section used to pick its own accent (pink / teal / indigo / red).
 * That reads as a second palette. One primary well keeps the page on the
 * product accent and makes the nav icons on the right the way-finding, not
 * a rainbow of wells.
 */
export function SettingsSectionHeader({
  icon,
  title,
  titleId,
  meta,
  action,
  className,
}: {
  icon: ReactNode;
  title: ReactNode;
  titleId?: string;
  meta?: ReactNode;
  action?: ReactNode;
  className?: string;
}) {
  return (
    <div className={cn("mb-3 flex items-center justify-between gap-3 px-1", className)}>
      <div className="flex min-w-0 items-center gap-2">
        <div
          aria-hidden
          className="flex h-7 w-7 shrink-0 items-center justify-center rounded-lg border border-primary/20 bg-primary/10 text-primary"
        >
          {icon}
        </div>
        <h2 id={titleId} className="text-sm font-semibold tracking-tight text-foreground">
          {title}
        </h2>
        {meta}
      </div>
      {action}
    </div>
  );
}
