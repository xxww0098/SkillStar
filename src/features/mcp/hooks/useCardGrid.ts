import { type CSSProperties, type RefObject, useLayoutEffect, useMemo, useRef, useState } from "react";

/** Matches the `1rem` gap `.ss-cards-grid` sets. */
const GRID_GAP_PX = 16;
const MIN_COLUMN_WIDTH_PX = 320;
/**
 * Hold the previous column count until the container has clearly outgrown it.
 * Without this the sidebar width animation oscillates the grid between two
 * adjacent counts as it crosses a breakpoint, which reads as flicker.
 */
const HYSTERESIS_PX = 8;

/**
 * Column count for a `.ss-cards-grid` container `width` px wide.
 *
 * Pure so the hysteresis rule is testable without a DOM. `previous` is the
 * count this last returned (0 on first run); the caller persists it.
 */
export function mcpGridColumnCount(width: number, previous: number): number {
  if (width === 0) return previous || 1;
  const minWidth = Math.max(220, MIN_COLUMN_WIDTH_PX);
  const stride = minWidth + GRID_GAP_PX;
  let columns = Math.max(1, Math.floor((width + GRID_GAP_PX) / stride));
  if (previous > 0 && columns < previous && width >= previous * stride - GRID_GAP_PX - HYSTERESIS_PX) {
    columns = previous;
  }
  return columns;
}

/**
 * Turn a card container's live width into a column count plus the matching
 * `grid-template-columns`.
 *
 * `.ss-cards-grid` owns the gap and the off-screen `content-visibility`
 * skipping but deliberately leaves the track list to the caller, so every grid
 * built on it needs exactly this. `contentKey` re-observes when the card list
 * itself changes.
 */
export function useCardGridColumns(
  containerRef: RefObject<HTMLElement | null>,
  contentKey: number,
): { columnCount: number; gridStyle: CSSProperties } {
  const [containerWidth, setContainerWidth] = useState(0);
  const previousRef = useRef(0);

  // `previousRef` carries the hysteresis across renders; the write is what makes
  // the "hold the previous count" rule stateful, matching the original inline
  // implementation this replaced.
  const columnCount = useMemo(() => {
    const next = mcpGridColumnCount(containerWidth, previousRef.current);
    previousRef.current = next;
    return next;
  }, [containerWidth]);

  useLayoutEffect(() => {
    const element = containerRef.current;
    if (!element) return;

    const updateWidth = () => setContainerWidth(element.clientWidth);
    updateWidth();

    const observer = new ResizeObserver(updateWidth);
    observer.observe(element);
    return () => observer.disconnect();
  }, [containerRef, contentKey]);

  const gridStyle = useMemo<CSSProperties>(
    () => ({ gridTemplateColumns: `repeat(${columnCount}, minmax(0, 1fr))` }),
    [columnCount],
  );

  return { columnCount, gridStyle };
}
