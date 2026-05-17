import type React from "react";
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
 * │ [Title | Search]  ···drag zone···  [Filters]  |  [Actions]      │
 * └──────────────────────────────────────────────────────────────────┘
 *
 * The flexible gap between search and filters/actions serves as the
 * window drag region on macOS (titleBarStyle: Overlay).
 */
export function PageToolbar({ title, search, filters, actions, className, children }: PageToolbarProps) {
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

      {/* ── Center zone: Filters (scrollable when overflowing) ── */}
      {filters && (
        <div className="flex items-center gap-2 min-w-0 overflow-x-auto [&::-webkit-scrollbar]:hidden">{filters}</div>
      )}

      {/* ── Drag spacer: fills remaining space ── */}
      <div data-tauri-drag-region className="flex-1 min-w-[48px] h-full" />

      {/* ── Right zone: Actions ── */}
      {actions && <div className="flex items-center gap-2 shrink-0">{actions}</div>}

      {children}
    </div>
  );
}
