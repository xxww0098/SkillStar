import React, { useRef } from "react";
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

export function HScrollRow({
  maxVisible = 4,
  itemWidth = 28,
  gap = 2,
  className,
  children,
  count,
  fixedWidth,
}: HScrollRowProps) {
  const scrollRef = useRef<HTMLDivElement>(null);
  const resolvedMaxVisible = Math.max(
    1,
    typeof maxVisible === "function" ? maxVisible(count) : maxVisible
  );
  const needsScroll = count > resolvedMaxVisible;
  const targetW = resolvedMaxVisible * itemWidth + (resolvedMaxVisible - 1) * gap;

  return (
    <div
      ref={scrollRef}
      onWheel={(e) => {
        if (!needsScroll || !scrollRef.current) return;
        e.preventDefault();
        scrollRef.current.scrollLeft += e.deltaY || e.deltaX;
      }}
      style={fixedWidth ? { width: `${targetW}px` } : needsScroll ? { maxWidth: `${targetW}px` } : undefined}
      className={cn(
        "flex items-center overflow-x-auto [&::-webkit-scrollbar]:hidden [-ms-overflow-style:none] [scrollbar-width:none]",
        className
      )}
    >
      {children}
    </div>
  );
}
