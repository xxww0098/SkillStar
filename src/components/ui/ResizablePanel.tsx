import { type ReactNode, useCallback, useEffect, useRef, useState } from "react";
import { cn } from "../../lib/utils";

interface ResizablePanelProps {
  /** Default width in pixels */
  defaultWidth: number;
  /** Minimum width in pixels (default: 360) */
  minWidth?: number;
  /** Maximum width as percentage of parent container (default: 90) */
  maxWidthPercent?: number;
  /** localStorage key for width persistence (omit to skip persistence) */
  storageKey?: string;
  /** Additional class names for the outer container */
  className?: string;
  children: ReactNode;
}

/**
 * Right-anchored panel with a draggable left edge for resizing.
 *
 * Usage:
 * ```tsx
 * <ResizablePanel defaultWidth={600} storageKey="skill-reader-width">
 *   {content}
 * </ResizablePanel>
 * ```
 */
export function ResizablePanel({
  defaultWidth,
  minWidth = 360,
  maxWidthPercent = 90,
  storageKey,
  className,
  children,
}: ResizablePanelProps) {
  const [width, setWidth] = useState(() => {
    if (storageKey) {
      const stored = localStorage.getItem(`skillstar:panel:${storageKey}`);
      if (stored) {
        const parsed = Number(stored);
        if (!Number.isNaN(parsed) && parsed >= minWidth) return parsed;
      }
    }
    return defaultWidth;
  });

  const dragging = useRef(false);
  const startX = useRef(0);
  const startWidth = useRef(0);
  const panelRef = useRef<HTMLDivElement>(null);

  // Clamp width to parent container bounds (not viewport) so the panel never
  // extends behind the sidebar or other siblings.
  const clamp = useCallback(
    (w: number) => {
      const parentWidth = panelRef.current?.parentElement?.clientWidth ?? window.innerWidth;
      const maxPx = Math.floor((parentWidth * maxWidthPercent) / 100);
      return Math.max(minWidth, Math.min(w, maxPx));
    },
    [minWidth, maxWidthPercent],
  );
  // Re-clamp width after mount (parent ref available) and on window resize,
  // so restored widths that exceeded the current parent area get corrected.
  useEffect(() => {
    const reclamp = () => setWidth((w) => clamp(w));
    // Defer initial reclamp to next frame so the parent is measured
    const raf = requestAnimationFrame(reclamp);
    window.addEventListener("resize", reclamp);
    return () => {
      cancelAnimationFrame(raf);
      window.removeEventListener("resize", reclamp);
    };
  }, [clamp]);

  // Persist width to localStorage
  useEffect(() => {
    if (storageKey) {
      localStorage.setItem(`skillstar:panel:${storageKey}`, String(width));
    }
  }, [width, storageKey]);

  // Handle mouse events for dragging
  const onMouseDown = useCallback(
    (e: React.MouseEvent) => {
      e.preventDefault();
      e.stopPropagation();
      dragging.current = true;
      startX.current = e.clientX;
      startWidth.current = width;
      document.body.style.cursor = "col-resize";
      document.body.style.userSelect = "none";
    },
    [width],
  );

  useEffect(() => {
    const onMouseMove = (e: MouseEvent) => {
      if (!dragging.current) return;
      // Dragging left → panel wider (deltaX negative = width increase)
      const delta = startX.current - e.clientX;
      setWidth(clamp(startWidth.current + delta));
    };

    const onMouseUp = () => {
      if (!dragging.current) return;
      dragging.current = false;
      document.body.style.cursor = "";
      document.body.style.userSelect = "";
    };

    window.addEventListener("mousemove", onMouseMove);
    window.addEventListener("mouseup", onMouseUp);
    return () => {
      window.removeEventListener("mousemove", onMouseMove);
      window.removeEventListener("mouseup", onMouseUp);
    };
  }, [clamp]);

  return (
    <div
      ref={panelRef}
      className={cn(
        "absolute right-0 top-0 bottom-0 h-full border-l border-border bg-background shadow-[0_4px_20px_-8px_rgba(0,0,0,0.3)] overflow-hidden z-50 rounded-tl-xl rounded-bl-xl flex flex-col",
        className,
      )}
      style={{ width }}
    >
      {/* Drag handle — left edge */}
      <div onMouseDown={onMouseDown} className="absolute left-0 top-0 bottom-0 w-1 z-[60] cursor-col-resize group">
        {/* Visual indicator — subtle line that glows on hover */}
        <div className="absolute inset-y-0 left-0 w-px bg-border group-hover:bg-primary/60 transition-colors duration-150" />
        {/* Wider hit area */}
        <div className="absolute inset-y-0 -left-1 w-3" />
      </div>

      {children}
    </div>
  );
}
