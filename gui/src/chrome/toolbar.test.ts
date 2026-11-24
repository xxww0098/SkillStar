// The bar control scale as data: all six page toolbars compose these numbers,
// so a page re-hardcoding 13px icons or a taller chip fails here instead of
// showing up as two toolbars that no longer line up.
import { expect, test } from "nsh/test";
import { BAR_MENU_ROW_H } from "./barMenu";
import {
  BAR_CHEVRON,
  BAR_FONT,
  BAR_H,
  BAR_ICON,
  BAR_PAD_X,
  BAR_RADIUS,
  BAR_SEARCH_W,
  BAR_SLOT_GAP,
  BAR_SLOT_H,
  BAR_SLOT_PAD,
  BAR_SLOT_RADIUS,
  BAR_SLOT_W,
} from "./toolbar";

test("toolbar control scale: 32px controls, 26px slots, one search width", () => {
  expect(BAR_H).toBe(32);
  expect(BAR_RADIUS).toBe(10);
  // A segmented track is one control box: 26px slot + 2px padding + 1px border.
  expect(BAR_SLOT_H + BAR_SLOT_PAD * 2 + 2).toBe(BAR_H);
  expect(BAR_SLOT_GAP).toBe(2);
  expect(BAR_SLOT_RADIUS).toBe(7);
  expect(BAR_SLOT_W).toBe(28);
  // Control typography and rhythm.
  expect(BAR_FONT).toBe(12);
  expect(BAR_ICON).toBe(14);
  expect(BAR_CHEVRON).toBe(11);
  expect(BAR_PAD_X).toBe(10);
  // One search width for every page bar; menu rows match the chip height.
  expect(BAR_SEARCH_W).toBe(190);
  expect(BAR_MENU_ROW_H).toBe(BAR_H);
});
