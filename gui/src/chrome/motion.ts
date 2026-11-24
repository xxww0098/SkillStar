import { reduceMotion, type ViewStyle } from "nsh";

// Motion tokens. The spec is docs/features/frontend/README.md「动画」.
// Call sites use the helpers below and do not pick their own curves.
//
// nsh parses timing functions itself (keywords, `cubic-bezier()`, and
// native physical `spring(stiffness, damping[, mass])`), does not resolve
// `var()`, and theme.json drops values that are not colors, fonts or
// lengths, so the `--ease-*` / `--duration-*` names live as these
// constants. CSS `linear()` is still rejected; physical springs use
// `spring(stiffness, damping[, mass])` instead (`EASE_SPRING`), where nsh
// derives the settle duration from the spring physics and ignores
// `transitionDuration`.

/** `--ease-out`: entrances. */
export const EASE_OUT = "cubic-bezier(0, 0, .58, 1)";

/** `--ease-out-strong`: a small part settling right after a click. */
export const EASE_OUT_STRONG = "cubic-bezier(.22, 1, .36, 1)";

/** `--ease-in`: exits only. */
export const EASE_IN = "cubic-bezier(.42, 0, 1, 1)";

/** `--ease-in-out`: on-screen moves. Not for entrances. */
export const EASE_IN_OUT = "cubic-bezier(.42, 0, .58, 1)";

/** `--ease-overshoot`: about 10% past, then back. Sub-64px confirmations. */
export const EASE_OVERSHOOT = "cubic-bezier(.34, 1.56, .64, 1)";

/**
 * `--ease-spring`: nsh 0.6+ native physical spring (`zeta ≈ 0.598` -> ~9.6%
 * overshoot, `settle_ms ≈ 300ms` matching `DURATION_SETTLE`). Self-timed
 * natively; `transitionDuration` is unused when paired with this curve.
 */
export const EASE_SPRING = "spring(1480, 46)";

/** `--duration-sm`: small UI entrances and short on-screen moves. */
export const DURATION_SM = 200;

/** `--duration-md`: dialogs and popovers with content. */
export const DURATION_MD = 300;

/** `--duration-lg`: sheets and the toast entrance. */
export const DURATION_LG = 400;

/** Small exits. Shorter than `--duration-sm`, inside 120–200ms. */
export const DURATION_SM_OUT = 160;

/** Hover and press, inside 100–150ms. */
export const DURATION_PRESS = 120;

/** Reduce Motion fade. Movement and scale are dropped around it. */
export const DURATION_REDUCED = 150;

/** Overshoot settle: 1.5× `--duration-sm`, so the return has time. */
export const DURATION_SETTLE = Math.round(DURATION_SM * 1.5);

/**
 * Segmented stretch. A touch slower than the slide (`DURATION_SM`) so the
 * capsule leads and settles after the glide. The 12% scale is unchanged.
 */
export const DURATION_STRETCH = DURATION_SM + 50;

/** On-screen move duration. The segmented stretch starts back after this. */
export const MOTION_MS = DURATION_SM;

export type MotionScale = "sm" | "md" | "lg";
export type MotionPhase = "enter" | "exit";

export { reduceMotion };

/** True when travel and scale are allowed (the inverse of the system setting). */
export function motionEnabled(): boolean {
  return !reduceMotion();
}

function durationOf(phase: MotionPhase, scale: MotionScale): number {
  if (phase === "exit") {
    // Shorter than the matching entrance, and still inside that size's band
    // where the band allows it (small exits are 120–200).
    if (scale === "sm") return DURATION_SM_OUT;
    if (scale === "md") return DURATION_SM;
    return DURATION_MD;
  }
  if (scale === "sm") return DURATION_SM;
  if (scale === "md") return DURATION_MD;
  return DURATION_LG;
}

function reducedFade(): ViewStyle {
  return {
    transitionProperty: "opacity",
    transitionDuration: DURATION_REDUCED,
    transitionTimingFunction: EASE_OUT,
  };
}

/**
 * Enter or leave. `move` adds transform to the opacity channel. Reduce
 * Motion keeps the fade and drops the transform channel.
 */
export function phaseTransition(
  phase: MotionPhase,
  scale: MotionScale = "md",
  move = false,
): ViewStyle {
  if (!motionEnabled()) return reducedFade();
  return {
    transitionProperty: move ? "opacity, transform" : "opacity",
    transitionDuration: durationOf(phase, scale),
    transitionTimingFunction: phase === "exit" ? EASE_IN : EASE_OUT,
  };
}

/**
 * Dialog / popover entrance: opacity, plus transform when `move` is set.
 * Medium duration. Never animate width, height or margin through this.
 */
export function fadeTransition(move = false): ViewStyle {
  return phaseTransition("enter", "md", move);
}

/** On-screen move: transform only, `--ease-in-out`. Empty under Reduce Motion. */
export function motionTransition(): ViewStyle {
  if (!motionEnabled()) return {};
  return {
    transitionProperty: "transform",
    transitionDuration: DURATION_SM,
    transitionTimingFunction: EASE_IN_OUT,
  };
}

/** Segmented capsule stretch: transform only, a touch slower than the slide. */
export function stretchTransition(): ViewStyle {
  if (!motionEnabled()) return {};
  return {
    transitionProperty: "transform",
    transitionDuration: DURATION_STRETCH,
    transitionTimingFunction: EASE_IN_OUT,
  };
}

/**
 * Native physical spring transition (`spring(stiffness, damping[, mass])`).
 * Reduce Motion snaps immediately with an empty style.
 */
export function springTransition(
  property = "transform",
  timing: `spring(${string})` = EASE_SPRING,
): ViewStyle {
  if (!motionEnabled()) return {};
  return {
    transitionProperty: property,
    transitionTimingFunction: timing,
  };
}

/**
 * Switch knob and other sub-64px confirms. Uses nsh's native physical
 * spring (`EASE_SPRING`, ~10% overshoot, ~300ms settle). Reduce Motion lands
 * immediately: the knob has no opacity to keep.
 */
export function settleTransition(): ViewStyle {
  return springTransition("transform", EASE_SPRING);
}

/**
 * The uninstall control's fill. Hover/press band, `--ease-out`. This is the
 * only color transition; every other hover stays zero-frame. Reduce Motion
 * snaps the fill.
 */
export function pressTransition(): ViewStyle {
  if (!motionEnabled()) return {};
  return {
    transitionProperty: "background-color, border-color",
    transitionDuration: DURATION_PRESS,
    transitionTimingFunction: EASE_OUT,
  };
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
