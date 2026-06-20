import { ChevronLeft, ChevronRight } from "lucide-react";
import type React from "react";
import { useCallback, useEffect, useRef, useState } from "react";
import { cn } from "../../lib/utils";

type VisibleCountResolver = number | ((count: number) => number);

interface HScrollRowProps {
  /** Maximum number of child slots visible before scrolling activates. Each slot = itemWidth px. */
  maxVisible?: VisibleCountResolver;
  /** Width of each child slot in px (default: 28) */
  itemWidth?: number;
  /** Gap between items in px (default: 2, matching gap-0.5) */
  gap?: number;
  /** Extra className on the scroll container */
  className?: string;
  children: React.ReactNode;
  /** Total number of items (used to decide if scroll is needed) */
  count: number;
  /** If true, the container will have a fixed width matching the maxVisible space */
  fixedWidth?: boolean;
}

const ARROW_SCROLL_STEP = 80;

export function HScrollRow({
  maxVisible,
  itemWidth = 28,
  gap = 2,
  className,
  children,
  count,
  fixedWidth,
}: HScrollRowProps) {
  const scrollRef = useRef<HTMLDivElement>(null);
  const [canScrollLeft, setCanScrollLeft] = useState(false);
  const [canScrollRight, setCanScrollRight] = useState(false);
  const [hovered, setHovered] = useState(false);

  const targetW =
    maxVisible !== undefined
      ? (() => {
          const resolved = Math.max(1, typeof maxVisible === "function" ? maxVisible(count) : maxVisible);
          return resolved * itemWidth + (resolved - 1) * gap;
        })()
      : undefined;

  /* ── arrow state ── */
  const updateArrows = useCallback(() => {
    const el = scrollRef.current;
    if (!el) return;
    setCanScrollLeft(el.scrollLeft > 1);
    setCanScrollRight(el.scrollLeft + el.clientWidth < el.scrollWidth - 1);
  }, []);

  useEffect(() => {
    const el = scrollRef.current;
    if (!el) return;
    const frame = requestAnimationFrame(updateArrows);
    const ro = new ResizeObserver(updateArrows);
    ro.observe(el);
    return () => {
      cancelAnimationFrame(frame);
      ro.disconnect();
    };
  }, [updateArrows]);

  const scrollByStep = useCallback(
    (dir: -1 | 1) => {
      scrollRef.current?.scrollBy({ left: dir * ARROW_SCROLL_STEP, behavior: "smooth" });
      setTimeout(updateArrows, 120);
    },
    [updateArrows],
  );

  // Only mount the arrow buttons when the content actually overflows.
  // When there is nothing to scroll (the common case: a handful of agent
  // chips that fit comfortably), we avoid emitting "Scroll left/right"
  // buttons into the DOM and accessibility tree entirely, instead of just
  // hiding them with CSS. `needsScroll` is driven by the ResizeObserver
  // above, so it stays correct when the row resizes or items change.
  const needsScroll = canScrollLeft || canScrollRight;
  const showArrows = hovered && needsScroll;

  return (
    <div
      className="hscroll-row-wrapper"
      onMouseEnter={() => {
        setHovered(true);
        updateArrows();
      }}
      onMouseLeave={() => setHovered(false)}
      onClick={(e) => e.stopPropagation()}
    >
      {/* Left arrow — only rendered when scrolling is possible */}
      {needsScroll && (
        <button
          type="button"
          aria-label="Scroll left"
          className={cn("hscroll-arrow hscroll-arrow-left", showArrows && canScrollLeft && "hscroll-arrow-visible")}
          onClick={(e) => {
            e.stopPropagation();
            scrollByStep(-1);
          }}
          tabIndex={-1}
        >
          <ChevronLeft className="w-3 h-3" />
        </button>
      )}

      <div
        ref={scrollRef}
        onWheel={(e) => {
          if (!scrollRef.current) return;
          const { scrollWidth, clientWidth } = scrollRef.current;
          if (scrollWidth <= clientWidth) return;
          // Stop page scroll AND convert vertical → horizontal
          e.stopPropagation();
          e.preventDefault();
          scrollRef.current.scrollLeft += e.deltaY || e.deltaX;
          updateArrows();
        }}
        onScroll={updateArrows}
        style={fixedWidth && targetW ? { width: `${targetW}px` } : targetW ? { maxWidth: `${targetW}px` } : undefined}
        className={cn(
          "flex items-center overflow-x-auto [&::-webkit-scrollbar]:hidden [-ms-overflow-style:none] [scrollbar-width:none]",
          className,
        )}
      >
        {children}
      </div>

      {/* Right arrow — only rendered when scrolling is possible */}
      {needsScroll && (
        <button
          type="button"
          aria-label="Scroll right"
          className={cn("hscroll-arrow hscroll-arrow-right", showArrows && canScrollRight && "hscroll-arrow-visible")}
          onClick={(e) => {
            e.stopPropagation();
            scrollByStep(1);
          }}
          tabIndex={-1}
        >
          <ChevronRight className="w-3 h-3" />
        </button>
      )}
    </div>
  );
}
