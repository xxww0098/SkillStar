import { type RefObject, useCallback, useRef } from "react";

/* ── Data-attribute used on card wrapper divs ─────────────────── */
export const DRAG_ATTR_CARD_ID = "data-drag-card-id";

/* ── CSS class names for drag states (applied via classList) ──── */
export const DRAG_CSS = {
  /** Applied to the dragged source card */
  dragging: "drag-source-active",
  /** Applied to the card being hovered above (drop indicator top) */
  dropAbove: "drag-drop-above",
  /** Applied to the card being hovered below (drop indicator bottom) */
  dropBelow: "drag-drop-below",
  /** Ghost element */
  ghost:
    "fixed pointer-events-none z-[9999] rounded-2xl border border-border bg-card shadow-2xl overflow-hidden will-change-transform",
} as const;

interface DragReorderOptions<T extends { id: string }> {
  items: T[];
  ghostRef: RefObject<HTMLDivElement | null>;
  scrollContainerRef: RefObject<HTMLDivElement | null>;
  listContainerRef: RefObject<HTMLDivElement | null>;
  appColor: string;
  onReorder: (reordered: T[]) => void;
}

interface DragReorderReturn {
  handleDragStart: (itemId: string, e: React.PointerEvent) => void;
}

/**
 * Zero-rerender drag-to-reorder hook.
 *
 * All visual feedback during drag is applied via direct DOM manipulation:
 * - Ghost position: translate3d (GPU-composited)
 * - Source card opacity: inline style
 * - Drop indicator: CSS class toggle
 *
 * React state is only updated ONCE at drop time (the reorder callback).
 */
export function useDragReorder<T extends { id: string }>(options: DragReorderOptions<T>): DragReorderReturn {
  const optionsRef = useRef(options);
  optionsRef.current = options;

  // Mutable drag session state (never triggers renders)
  const session = useRef<{
    dragId: string;
    overId: string | null;
    overPos: "above" | "below";
    offsetY: number;
    startX: number;
    sourceEl: HTMLElement | null;
    rafId: number;
    pendingY: number;
    lastIndicatorId: string | null;
    lastIndicatorPos: "above" | "below" | null;
  } | null>(null);

  /**
   * Query all card wrapper elements from the list container.
   * Uses the data attribute instead of a Map for ref registration.
   */
  const getCardElements = useCallback((): Map<string, HTMLElement> => {
    const container = optionsRef.current.listContainerRef.current;
    if (!container) return new Map();
    const map = new Map<string, HTMLElement>();
    const nodes = container.querySelectorAll(`[${DRAG_ATTR_CARD_ID}]`);
    for (const node of nodes) {
      const id = (node as HTMLElement).getAttribute(DRAG_ATTR_CARD_ID);
      if (id) map.set(id, node as HTMLElement);
    }
    return map;
  }, []);

  /** Clear all drop indicator classes from every card */
  const clearAllIndicators = useCallback(() => {
    const container = optionsRef.current.listContainerRef.current;
    if (!container) return;
    for (const el of container.querySelectorAll(`.${DRAG_CSS.dropAbove}, .${DRAG_CSS.dropBelow}`)) {
      el.classList.remove(DRAG_CSS.dropAbove, DRAG_CSS.dropBelow);
    }
  }, []);

  const onPointerMove = useCallback(
    (e: PointerEvent) => {
      const s = session.current;
      if (!s) return;

      // Store pending Y for the rAF loop
      s.pendingY = e.clientY - s.offsetY;

      // Move ghost via rAF (coalesces multiple moves per frame)
      if (!s.rafId) {
        s.rafId = requestAnimationFrame(() => {
          const ghost = optionsRef.current.ghostRef.current;
          if (ghost && session.current) {
            ghost.style.transform = `translate3d(0, ${session.current.pendingY}px, 0) scale(1.02)`;
          }
          if (session.current) session.current.rafId = 0;
        });
      }

      // Auto-scroll near edges
      const scrollContainer = optionsRef.current.scrollContainerRef.current;
      if (scrollContainer) {
        const rect = scrollContainer.getBoundingClientRect();
        const EDGE = 48;
        const SPEED = 10;
        if (e.clientY < rect.top + EDGE) {
          scrollContainer.scrollTop -= SPEED;
        } else if (e.clientY > rect.bottom - EDGE) {
          scrollContainer.scrollTop += SPEED;
        }
      }

      // Find closest card
      const cards = getCardElements();
      let closestId: string | null = null;
      let closestPos: "above" | "below" = "below";
      let closestDist = Number.POSITIVE_INFINITY;

      for (const [id, el] of cards.entries()) {
        if (id === s.dragId) continue;
        const r = el.getBoundingClientRect();
        const midY = r.top + r.height / 2;
        const dist = Math.abs(e.clientY - midY);
        if (dist < closestDist) {
          closestDist = dist;
          closestId = id;
          closestPos = e.clientY < midY ? "above" : "below";
        }
      }

      // Only update DOM if the indicator target changed
      if (s.lastIndicatorId !== closestId || s.lastIndicatorPos !== closestPos) {
        // Remove old indicator
        if (s.lastIndicatorId) {
          const oldEl = cards.get(s.lastIndicatorId);
          oldEl?.classList.remove(DRAG_CSS.dropAbove, DRAG_CSS.dropBelow);
        }
        // Add new indicator
        if (closestId) {
          const newEl = cards.get(closestId);
          if (newEl) {
            newEl.classList.add(closestPos === "above" ? DRAG_CSS.dropAbove : DRAG_CSS.dropBelow);
          }
        }
        s.lastIndicatorId = closestId;
        s.lastIndicatorPos = closestPos;
      }

      s.overId = closestId;
      s.overPos = closestPos;
    },
    [getCardElements],
  );

  const onPointerUp = useCallback(() => {
    const s = session.current;
    if (!s) return;

    // Cancel any pending rAF
    if (s.rafId) cancelAnimationFrame(s.rafId);

    const opts = optionsRef.current;

    // Perform reorder
    if (s.overId && s.dragId !== s.overId) {
      const items = [...opts.items];
      const fromIdx = items.findIndex((p) => p.id === s.dragId);
      if (fromIdx !== -1) {
        const [moved] = items.splice(fromIdx, 1);
        let toIdx = items.findIndex((p) => p.id === s.overId);
        if (toIdx !== -1) {
          if (s.overPos === "below") toIdx += 1;
          items.splice(toIdx, 0, moved);
          opts.onReorder(items);
        }
      }
    }

    // Clean up ghost
    const ghost = opts.ghostRef.current;
    if (ghost) {
      ghost.style.display = "none";
      ghost.innerHTML = "";
      ghost.style.transform = "";
    }

    // Restore source card
    if (s.sourceEl) {
      s.sourceEl.classList.remove(DRAG_CSS.dragging);
      s.sourceEl.style.opacity = "";
    }

    // Clear all indicators
    clearAllIndicators();

    // Tear down
    session.current = null;
    document.removeEventListener("pointermove", onPointerMove);
    document.removeEventListener("pointerup", onPointerUp);
  }, [onPointerMove, clearAllIndicators]);

  const handleDragStart = useCallback(
    (itemId: string, e: React.PointerEvent) => {
      const opts = optionsRef.current;
      const ghost = opts.ghostRef.current;
      if (!ghost) return;

      const cards = getCardElements();
      const cardEl = cards.get(itemId);
      if (!cardEl) return;

      const rect = cardEl.getBoundingClientRect();

      // Calculate the starting translateY so ghost is positioned at the card
      // We use translate3d relative to the ghost's fixed position origin (0,0 of viewport)
      const startY = rect.top;
      const offsetY = e.clientY - startY;

      // Set up ghost — position with left/width, translate for Y (GPU path)
      ghost.style.width = `${rect.width}px`;
      ghost.style.height = `${rect.height}px`;
      ghost.style.left = `${rect.left}px`;
      ghost.style.top = "0px";
      ghost.style.transform = `translate3d(0, ${startY}px, 0) scale(1.02)`;
      ghost.style.display = "block";

      // Clone card content into ghost (deep clone, faster than innerHTML)
      ghost.innerHTML = "";
      const clone = cardEl.cloneNode(true) as HTMLElement;
      // Remove drag handle grab cursor from clone
      clone.style.pointerEvents = "none";
      ghost.appendChild(clone);

      // Dim the source card
      cardEl.classList.add(DRAG_CSS.dragging);
      cardEl.style.opacity = "0.35";

      // Initialize session
      session.current = {
        dragId: itemId,
        overId: null,
        overPos: "below",
        offsetY,
        startX: rect.left,
        sourceEl: cardEl,
        rafId: 0,
        pendingY: startY,
        lastIndicatorId: null,
        lastIndicatorPos: null,
      };

      document.addEventListener("pointermove", onPointerMove);
      document.addEventListener("pointerup", onPointerUp);
    },
    [getCardElements, onPointerMove, onPointerUp],
  );

  return { handleDragStart };
}
