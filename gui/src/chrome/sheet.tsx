import { View, type JSX, type StyleProp, type ViewStyle } from "nsh";
import { paletteToken } from "../services/themeMode";
import { DETAIL_COLUMN_W } from "../layout";
import { SHEET_SHADOW } from "./dialog";

// The floating detail sheet — chrome::sheet parity (chrome/sheet.rs).
//
// The three detail drawers (skill / market / deck) share this surface: the
// page row reserves a fixed-width track (DETAIL_COLUMN_W), the track paints
// a SHEET_GAP ring as padding on every side, and the sheet inside that ring
// gets the card tier's 16px radius, a full hairline, and the kit Dialog's
// resting shadow. It appears in ONE repaint — no slide-in, no fade — and
// the main grid drops a column in the same frame.
//
// Scrolling containment (the_sheet_wheel_scrolls_the_copy_only): the sheet
// body is the only `Scroll`; the column stops propagation on wheel so the
// page scroller never feels the sheet's wheel.

export const SHEET_GAP = 12;

/**
 * The floating surface inside a padded detail track. Children mount as the
 * drawer's header/body/actions stack. `style` shapes the outer track.
 */
export function Sheet(props: { children?: JSX.Element; style?: StyleProp<ViewStyle> }): JSX.Element {
  return (
    <View
      style={{
        position: "relative",
        width: DETAIL_COLUMN_W,
        height: "100%",
        flexShrink: 0,
        display: "flex",
        flexDirection: "column",
        padding: SHEET_GAP,
        ...(props.style as object),
      }}
    >
      <View
        style={{
          flex: 1,
          minWidth: 0,
          minHeight: 0,
          display: "flex",
          flexDirection: "column",
          overflow: "hidden",
          borderRadius: 16,
          backgroundColor: paletteToken("panel"),
          borderWidth: 1,
          borderColor: paletteToken("border"),
          boxShadow: SHEET_SHADOW,
        }}
      >
        {props.children}
      </View>
    </View>
  );
}
