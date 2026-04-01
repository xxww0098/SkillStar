import {
  Children,
  useCallback,
  useEffect,
  useRef,
  useState,
  type ReactNode,
  type WheelEvent,
} from "react";
import { ChevronLeft, ChevronRight } from "lucide-react";
import type { Components } from "react-markdown";

function stringifyChildren(children: ReactNode): string {
  return Children.toArray(children)
    .map((child) => (typeof child === "string" || typeof child === "number" ? String(child) : ""))
    .join("");
}

function stripWrappedBackticks(value: string): string {
  if (value.includes("\n")) {
    return value;
  }

  let next = value;
  while (next.length >= 2 && next.startsWith("`") && next.endsWith("`")) {
    next = next.slice(1, -1);
  }
  return next;
}

/* ─── Scroll-isolated container with navigation arrows ─── */

const SCROLL_STEP = 200;

/**
 * A wrapper that:
 * 1. Captures wheel events so the outer page never scrolls while the
 *    user's pointer is inside an overflowing block (code / SVG / etc.).
 * 2. Shows left / right fade-in arrows when the content is wider than
 *    the container — arrows appear on hover, hidden otherwise.
 */
function ScrollBox({
  children,
  className,
  as: Tag = "div",
}: {
  children: ReactNode;
  className?: string;
  as?: "div" | "pre";
}) {
  const scrollRef = useRef<HTMLElement | null>(null);
  const [canScrollLeft, setCanScrollLeft] = useState(false);
  const [canScrollRight, setCanScrollRight] = useState(false);
  const [hovered, setHovered] = useState(false);

  /* ── recalculate arrow visibility ── */
  const updateArrows = useCallback(() => {
    const el = scrollRef.current;
    if (!el) return;
    setCanScrollLeft(el.scrollLeft > 1);
    setCanScrollRight(el.scrollLeft + el.clientWidth < el.scrollWidth - 1);
  }, []);

  useEffect(() => {
    const el = scrollRef.current;
    if (!el) return;
    // Initial check (after children render)
    const frame = requestAnimationFrame(updateArrows);
    const ro = new ResizeObserver(updateArrows);
    ro.observe(el);
    return () => {
      cancelAnimationFrame(frame);
      ro.disconnect();
    };
  }, [updateArrows]);

  /* ── wheel isolation: stop page scroll while inside ── */
  const onWheel = useCallback(
    (e: WheelEvent<HTMLDivElement>) => {
      const el = scrollRef.current;
      if (!el) return;

      const { scrollTop, scrollHeight, clientHeight, scrollLeft, scrollWidth, clientWidth } = el;
      const isVertical = Math.abs(e.deltaY) > Math.abs(e.deltaX);

      if (isVertical) {
        // If the content overflows vertically, eat the event
        if (scrollHeight > clientHeight + 1) {
          const atTop = scrollTop <= 0 && e.deltaY < 0;
          const atBottom = scrollTop + clientHeight >= scrollHeight - 1 && e.deltaY > 0;
          if (!atTop && !atBottom) {
            e.stopPropagation();
          }
        }
        // If only horizontal overflow: convert vertical wheel → horizontal scroll
        if (scrollHeight <= clientHeight + 1 && scrollWidth > clientWidth + 1) {
          e.preventDefault();
          el.scrollLeft += e.deltaY;
          updateArrows();
        }
      } else {
        if (scrollWidth > clientWidth + 1) {
          const atLeft = scrollLeft <= 0 && e.deltaX < 0;
          const atRight = scrollLeft + clientWidth >= scrollWidth - 1 && e.deltaX > 0;
          if (!atLeft && !atRight) {
            e.stopPropagation();
          }
        }
      }
    },
    [updateArrows],
  );

  const scrollBy = useCallback(
    (dir: -1 | 1) => {
      scrollRef.current?.scrollBy({ left: dir * SCROLL_STEP, behavior: "smooth" });
      // Delay arrow update to after smooth scroll starts
      setTimeout(updateArrows, 120);
    },
    [updateArrows],
  );

  const showArrows = hovered && (canScrollLeft || canScrollRight);

  return (
    <div
      className={`scroll-box-wrapper ${className ?? ""}`}
      onMouseEnter={() => { setHovered(true); updateArrows(); }}
      onMouseLeave={() => setHovered(false)}
    >
      {/* Left arrow */}
      <button
        type="button"
        aria-label="Scroll left"
        className={`scroll-box-arrow scroll-box-arrow-left ${showArrows && canScrollLeft ? "scroll-box-arrow-visible" : ""}`}
        onClick={() => scrollBy(-1)}
        tabIndex={-1}
      >
        <ChevronLeft className="w-4 h-4" />
      </button>

      {Tag === "pre" ? (
        <pre
          ref={scrollRef as React.RefObject<HTMLPreElement | null>}
          className="scroll-box-inner"
          onWheel={onWheel as unknown as React.WheelEventHandler<HTMLPreElement>}
          onScroll={updateArrows}
        >
          {children}
        </pre>
      ) : (
        <div
          ref={scrollRef as React.RefObject<HTMLDivElement | null>}
          className="scroll-box-inner"
          onWheel={onWheel as unknown as React.WheelEventHandler<HTMLDivElement>}
          onScroll={updateArrows}
        >
          {children}
        </div>
      )}

      {/* Right arrow */}
      <button
        type="button"
        aria-label="Scroll right"
        className={`scroll-box-arrow scroll-box-arrow-right ${showArrows && canScrollRight ? "scroll-box-arrow-visible" : ""}`}
        onClick={() => scrollBy(1)}
        tabIndex={-1}
      >
        <ChevronRight className="w-4 h-4" />
      </button>
    </div>
  );
}

/* ─── Markdown component overrides ─── */

export const markdownComponents: Components = {
  code({ node: _node, children, className, ...props }) {
    const raw = stringifyChildren(children).replace(/\n$/, "");
    const normalized = stripWrappedBackticks(raw);
    return (
      <code className={className} {...props}>
        {normalized}
      </code>
    );
  },

  // Wrap SVG images in a scrollable container with arrows.
  img({ src, alt, ...props }) {
    const isSvg = typeof src === "string" && src.toLowerCase().endsWith(".svg");
    const imgEl = <img src={src} alt={alt ?? ""} {...props} />;
    return isSvg ? <ScrollBox className="scroll-box-svg">{imgEl}</ScrollBox> : imgEl;
  },

  // Code blocks get scroll isolation + arrow navigation.
  pre({ children, ...props }) {
    void props; // unused rest — className, etc. are on the inner <pre>
    return (
      <ScrollBox as="pre" className="scroll-box-pre">
        {children}
      </ScrollBox>
    );
  },
};
