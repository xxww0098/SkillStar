import type { CSSProperties } from "react";
import { cn } from "@/lib/utils";
import type { LobeIconComponent } from "./lobe";

interface LobeIconProps {
  icon: LobeIconComponent;
  /** SVG size in px; `"100%"` fills the wrapper box (size the box via `className`). */
  size?: number | string;
  /** Classes for the wrapper span (box size, border, background, …). */
  className?: string;
  /** Forwarded to the SVG — e.g. `{ color }` to tint a Mono variant. */
  style?: CSSProperties;
}

/**
 * Canonical renderer for `@lobehub/icons` glyphs (imported via `./lobe`):
 * centers the SVG in an inline box.
 *
 * The glyphs embed a `<title>` the browser would surface as a hover tooltip.
 * `pointer-events-none` inherits into the SVG, so hit-testing passes through to
 * whatever wraps the icon — the tooltip never fires and the parent keeps its own
 * hover, click and `title`. Doing it in CSS keeps this component hook-free:
 * stripping the `<title>` node in a `useLayoutEffect` (which had no dependency
 * array) re-scanned every icon's subtree synchronously before every paint, and
 * these render by the dozen per screen.
 */
export function LobeIcon({ icon: Icon, size, className, style }: LobeIconProps) {
  return (
    <span className={cn("pointer-events-none inline-flex shrink-0 items-center justify-center", className)} aria-hidden>
      <Icon size={size} style={style} />
    </span>
  );
}
