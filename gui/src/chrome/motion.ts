import { reduceMotion, type ViewStyle } from "nsh";

// Motion channels (docs/features/frontend/README.md「动效四通道」→ the nsh
// equivalents of chrome/motion.rs):
//
// 1. Hover/press color feedback is ZERO-FRAME: paint it with
//    `hoverStyle`/`activeStyle` on the element — never a transition, never a
//    spring. Colors must not appear in `motionTransition`.
// 2. Layout displacement (selection box, capsule thumb, drawer) uses
//    `motionTransition()` — a CSS transform transition that settles in ~80ms,
//    the settle time of the GPUI critically-damped spring
//    (INTERACTION_SPRING ω=80 ζ=1, motion.rs:25).
// 3. Opacity/glyph animation uses `fadeTransition()` — and must never touch
//    width/height/margin.
// 4. Skeletons pulse on the `pulse` keyframes; spinners turn 0.8s/rev
//    (feedback.tsx).
//
// System Reduce Motion: every helper returns an empty style — things land on
// their end state, and timers that only exist to animate (notice enter/exit)
// must check `motionEnabled()` and skip themselves.

/** ~80ms settle, matching the GPUI spring's tolerance (MOTION_EPSILON 0.02). */
export const MOTION_MS = 80;

export { reduceMotion };

/** True when animation is allowed (the inverse of the system setting). */
export function motionEnabled(): boolean {
  return !reduceMotion();
}

/** Channel-2 transition: transform only, ~80ms settle, none under RM. */
export function motionTransition(): ViewStyle {
  if (!motionEnabled()) return {};
  return {
    transitionProperty: "transform",
    transitionDuration: MOTION_MS,
    transitionTimingFunction: "cubic-bezier(0.25, 0.1, 0.25, 1)",
  };
}

/**
 * The segmented capsule's stretch: transform only, a touch slower than the
 * slide so the capsule leads the glide and settles back after it. Channel 2
 * with its own duration; Reduce Motion empties it like motionTransition().
 */
export function stretchTransition(): ViewStyle {
  if (!motionEnabled()) return {};
  return {
    transitionProperty: "transform",
    transitionDuration: 120,
    transitionTimingFunction: "cubic-bezier(0.25, 0.1, 0.25, 1)",
  };
}

/**
 * Channel-3 transition: opacity (and transform when `move` is set), e.g.
 * dialog FadeSlide entrances and notice enter/exit. Never animate
 * width/height/margin through this.
 */
export function fadeTransition(move = false): ViewStyle {
  if (!motionEnabled()) return {};
  const channels = move ? "opacity, transform" : "opacity";
  return {
    transitionProperty: channels,
    transitionDuration: 160,
    transitionTimingFunction: "ease-out",
  };
}

/**
 * Press layer (motion.rs PRESS_LAYER): a pressed surface moves 12% toward
 * the foreground ink. Use inside `activeStyle` for hand-built controls.
 */
export function pressInk(surface: string, ink: string): string {
  return mixHex(surface, ink, 0.12);
}

function hexToRgb(hex: string): [number, number, number] {
  const n = parseInt(hex.slice(1), 16);
  return [(n >> 16) & 0xff, (n >> 8) & 0xff, n & 0xff];
}

export function mixHex(from: string, to: string, t: number): string {
  const a = hexToRgb(from);
  const b = hexToRgb(to);
  const ch = (i: number) =>
    Math.round(Math.min(255, Math.max(0, a[i] + (b[i] - a[i]) * t)))
      .toString(16)
      .padStart(2, "0");
  return `#${ch(0)}${ch(1)}${ch(2)}`;
}
