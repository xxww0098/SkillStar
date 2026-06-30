import type React from "react";
import { useEffect, useRef, useState } from "react";
import { cn } from "../../lib/utils";

export interface PageToolbarProps {
  /** Page title text or ReactNode */
  title?: React.ReactNode;
  /** Search / left-side controls slot */
  search?: React.ReactNode;
  /** Center filter controls slot (between search and actions) */
  filters?: React.ReactNode;
  /** Right-side action buttons slot */
  actions?: React.ReactNode;
  /** Additional className for the root container */
  className?: string;
  /** Children rendered after all slots (escape hatch) */
  children?: React.ReactNode;
}

/**
 * Unified page toolbar with three-zone layout:
 *
 * ┌──────────────────────────────────────────────────────────────────┐
 * │ [Title | Search]  [Filters]···drag filler···  |  [Actions]      │
 * └──────────────────────────────────────────────────────────────────┘
 *
 * The center zone is flexible: filters sit at its start and the trailing
 * drag filler absorbs the slack (and doubles as the macOS window drag
 * region). The filler yields its width first, so filters only scroll
 * horizontally once the window is genuinely too narrow — at which point
 * fade masks on either edge make the overflow discoverable.
 */
export function PageToolbar({ title, search, filters, actions, className, children }: PageToolbarProps) {
  const filtersRef = useRef<HTMLDivElement>(null);
  const [canLeft, setCanLeft] = useState(false);
  const [canRight, setCanRight] = useState(false);

  const updateOverflow = () => {
    const el = filtersRef.current;
    if (!el) return;
    setCanLeft(el.scrollLeft > 1);
    setCanRight(el.scrollLeft + el.clientWidth < el.scrollWidth - 1);
  };

  useEffect(() => {
    const el = filtersRef.current;
    if (!el) return;
    const frame = requestAnimationFrame(updateOverflow);
    const ro = new ResizeObserver(updateOverflow);
    ro.observe(el);
    return () => {
      cancelAnimationFrame(frame);
      ro.disconnect();
    };
  }, [filters]);

  return (
    <div
      data-tauri-drag-region
      className={cn("h-14 flex items-center gap-3 px-6 border-b border-border bg-sidebar shrink-0", className)}
    >
      {/* ── Left zone: Title + Search ── */}
      {title && (
        <div className="flex items-center shrink-0 h-8 whitespace-nowrap">
          <div className="text-sm font-semibold text-foreground">{title}</div>
          <div className="w-px h-5 ml-4 mr-1 bg-border" />
        </div>
      )}

      {search && <div className="shrink-0">{search}</div>}

      {/* ── Center zone: Filters claim the flexible middle; the drag filler yields its
          space first so filters only scroll when the window is genuinely too narrow ── */}
      <div className="flex-1 min-w-0 h-full flex items-center gap-2">
        {filters && (
          <div className="relative min-w-0 flex items-center">
            {/* Left fade mask */}
            {canLeft && (
              <div className="pointer-events-none absolute left-0 top-0 bottom-0 w-6 z-10 bg-gradient-to-r from-sidebar to-transparent" />
            )}
            <div
              ref={filtersRef}
              onWheel={(e) => {
                const el = filtersRef.current;
                if (!el) return;
                if (el.scrollWidth <= el.clientWidth) return;
                e.stopPropagation();
                e.preventDefault();
                el.scrollLeft += e.deltaY || e.deltaX;
              }}
              onScroll={updateOverflow}
              className="flex items-center gap-2 min-w-0 overflow-x-auto [&::-webkit-scrollbar]:hidden [-ms-overflow-style:none] [scrollbar-width:none]"
            >
              {filters}
            </div>
            {/* Right fade mask */}
            {canRight && (
              <div className="pointer-events-none absolute right-0 top-0 bottom-0 w-6 z-10 bg-gradient-to-l from-sidebar to-transparent" />
            )}
          </div>
        )}

        {/* Drag filler: absorbs the slack and shrinks to 0 before filters start scrolling */}
        <div data-tauri-drag-region className="flex-1 min-w-0 h-full" />
      </div>

      {/* ── Right zone: Actions ── */}
      {actions && <div className="flex items-center gap-2 shrink-0">{actions}</div>}

      {children}
    </div>
  );
}
