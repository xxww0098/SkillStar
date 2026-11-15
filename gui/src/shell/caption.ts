// The window's caption area: where the platform draws its own window controls
// over the app while the title bar is hidden (nsh.json `window.titleBar`,
// D-115). Both numbers come from `titleBar()`, which nsh reports in points and
// which is all 0 while the platform shows a title bar of its own, off the
// desktops, and in macOS full screen.
//
// - macOS puts its traffic lights at the top LEFT, over the rail lane, so the
//   rail keeps a top spacer for them (36px, the design's lane).
// - Windows draws its three caption buttons (46pt each) at the top RIGHT, over
//   the main panel, so the page bands stop before them instead.
import { titleBar } from "nsh/desktop";
import { PAGE_PAD, SHELL_GAP } from "../layout";

/** Height of the rail's top spacer on macOS (the traffic-light lane). */
export const TRAFFIC_LIGHT_LANE = 36;

/**
 * The rail's top spacer. Where the platform draws its window controls at the
 * top right (Windows, GTK), nothing overlaps the rail and the brand row starts
 * at the window's top edge.
 */
export function trafficLightLane(): number {
  return titleBar().right > 0 ? 0 : TRAFFIC_LIGHT_LANE;
}

/**
 * A page band's right gutter for caption buttons `captionWidth` wide: PAGE_PAD
 * normally, or far enough in that the band's card ends beside the buttons
 * instead of under them. The page sits `SHELL_GAP` inside the window, so that
 * is what the width is measured from.
 */
export function bandPadRight(captionWidth: number): number {
  return Math.max(PAGE_PAD, captionWidth - SHELL_GAP);
}

/** `bandPadRight` of this window now (reactive): what both page bands use. */
export function pageBandPadRight(): number {
  return bandPadRight(titleBar().right);
}
