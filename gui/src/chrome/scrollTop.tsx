import { View, type JSX, type ListRef, type ScrollRef } from "nsh";
import { paletteToken } from "../services/themeMode";
import { Icon } from "./icons";
import { Tip } from "./tooltip";
import { motionEnabled } from "./motion";

// Floating back-to-top control — chrome::back_to_top_button parity
// (chrome/scroll_top.rs). Mount inside a `position: relative` page body; it
// pins to the bottom-right corner (`bottom-8 right-8`). The caller tracks
// scroll (an `onScroll` handler updating a signal — the render must not read
// the scroll offset per wheel tick); mount this only while scrolled past
// 300px (SHOW_AFTER_PX). A click hops the arrow, then scrolls the ref to
// the top. Reduce Motion skips the hop.

export const SHOW_AFTER_PX = 300;

const CLICK_POP_MS = 160;

export function BackToTopButton(props: {
  label: string;
  scrollRef?: ScrollRef | null;
  listRef?: ListRef | null;
  onClick?: () => void;
}): JSX.Element {
  return (
    <View
      testID="back-to-top"
      tooltip={<Tip text={props.label} />}
      style={{
        position: "absolute",
        bottom: 32,
        right: 32,
        width: 40,
        height: 40,
        display: "flex",
        alignItems: "center",
        justifyContent: "center",
        borderRadius: 999,
        borderWidth: 1,
        borderColor: paletteToken("border"),
        backgroundColor: paletteToken("card"),
        cursor: "pointer",
        zIndex: 20,
      }}
      hoverStyle={{ backgroundColor: paletteToken("card_hover") }}
      onPress={() => {
        props.onClick?.();
        if (props.listRef) {
          props.listRef.scrollToOffset({ offset: 0, animated: false });
          return;
        }
        props.scrollRef?.scrollTo({ y: 0, animated: motionEnabled() });
      }}
    >
      <Icon name="arrowUp" size={16} color={paletteToken("fg_muted")} />
    </View>
  );
}

/** The hop the arrow plays before the list scrolls — a plain glyph
 * transform; its duration is exported for callers that chain the scroll
 * after it (scroll_top.rs CLICK_POP). */
export const CLICK_POP = CLICK_POP_MS;
