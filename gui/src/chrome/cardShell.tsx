import { Text, View, type JSX, type ViewStyle } from "nsh";
import { CARD_GAP, CARD_ROW_W, SKILL_CARD_H, SKILL_CARD_W } from "../layout";
import { paletteToken } from "../services/themeMode";

// Shared 16px-radius card & row surface tokens for Skills, Marketplace, and Decks.
// Keeps dimensions pinned to `layout.ts` (`SKILL_CARD_W × SKILL_CARD_H`) so grid
// invariants remain exact while delivering a single unified workbench silhouette.

/** Fixed-width card column count for `contentWidth` (`grid.rs columns_for`). */
export function columnsFor(contentWidth: number, cardW = SKILL_CARD_W, gap = CARD_GAP): number {
  if (contentWidth < cardW) return 1;
  const pitch = cardW + gap;
  return Math.floor((contentWidth + gap) / pitch);
}

/** Column count for a measured pane width, falling back to `fallbackContentW`
 * before the first `onLayout` measurement so initial `<List>` packing matches
 * the steady-state 3-column / 2-column track. */
export function gridColumnsForPane(
  paneWidth: number,
  listMode: boolean,
  padX = 0,
  fallbackContentW = CARD_ROW_W,
): number {
  if (listMode) return 1;
  const contentW = paneWidth > 0 ? Math.max(0, paneWidth - padX * 2) : fallbackContentW;
  return columnsFor(contentW);
}

/** Responsive column width matching `repeat(auto-fill, minmax(SKILL_CARD_W, 1fr))`:
 * equals `SKILL_CARD_W` (336) at default 1040px (3 cols) and 688px (2 cols), and
 * distributes residual horizontal slack evenly across columns so left/right edges
 * stay flush with `PageBar` at any window or sidebar width. */
export function cardTrackWidth(
  paneWidth: number,
  listMode: boolean,
  padX = 0,
  fallbackContentW = CARD_ROW_W,
  minCardW = SKILL_CARD_W,
  gap = CARD_GAP,
): number {
  const contentW = paneWidth > 0 ? Math.max(0, paneWidth - padX * 2) : fallbackContentW;
  if (contentW <= 0) return minCardW;
  if (listMode) return contentW;
  const cols = columnsFor(contentW, minCardW, gap);
  if (cols <= 1) return contentW;
  return Math.floor((contentW - gap * (cols - 1)) / cols);
}

/** Pack a flat item list into fixed-column rows for row-virtualized `<List>`. */
export function packCardRows<T>(items: readonly T[], columns: number): T[][] {
  const cols = Math.max(1, columns);
  const rows: T[][] = [];
  for (let ix = 0; ix < items.length; ix += cols) {
    rows.push(items.slice(ix, ix + cols));
  }
  return rows;
}

export function skillCardFrameStyle(selected: boolean): ViewStyle {
  return {
    width: SKILL_CARD_W,
    height: SKILL_CARD_H,
    borderRadius: 16,
    borderWidth: 1,
    borderColor: selected ? paletteToken("accent") : paletteToken("border"),
    backgroundColor: selected ? paletteToken("card_active") : paletteToken("card"),
    overflow: "hidden",
    display: "flex",
    flexDirection: "column",
    cursor: "pointer",
  };
}

export function skillCardHoverStyle(selected: boolean): ViewStyle {
  return {
    backgroundColor: selected ? paletteToken("card_active_hover") : paletteToken("card_hover"),
    borderColor: selected ? paletteToken("accent") : paletteToken("edge"),
  };
}

/** Omit redundant source strings that duplicate the skill's own name. */
export function distinctSource(
  source: string | null | undefined,
  skillName: string,
): string | null {
  const trimmed = source?.trim();
  if (!trimmed || trimmed === skillName.trim()) return null;
  return trimmed;
}

/** Lightweight inline source metadata rendered in card footers without chrome boxes. */
export function SourceMeta(props: {
  label: string;
  onPress?: () => void;
  testID?: string;
  maxWidth?: number;
}): JSX.Element {
  return (
    <View
      testID={props.testID}
      style={{
        display: "flex",
        flexDirection: "row",
        alignItems: "center",
        maxWidth: props.maxWidth ?? 144,
        flexShrink: 1,
        minWidth: 0,
        cursor: props.onPress ? "pointer" : "default",
      }}
      hoverStyle={props.onPress ? { opacity: 0.78 } : undefined}
      onPress={props.onPress}
    >
      <Text
        style={{
          fontSize: 11,
          color: paletteToken("fg_muted"),
          numberOfLines: 1,
        }}
      >
        {props.label}
      </Text>
    </View>
  );
}
