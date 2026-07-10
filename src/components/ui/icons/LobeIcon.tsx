import { type CSSProperties, useLayoutEffect, useRef } from "react";
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
 * centers the SVG in an inline box and strips the embedded `<title>` so brand
 * names don't surface as browser tooltips.
 */
export function LobeIcon({ icon: Icon, size, className, style }: LobeIconProps) {
  const boxRef = useRef<HTMLSpanElement>(null);

  useLayoutEffect(() => {
    boxRef.current?.querySelectorAll("title").forEach((title) => title.remove());
  });

  return (
    <span ref={boxRef} className={cn("inline-flex shrink-0 items-center justify-center", className)} aria-hidden>
      <Icon size={size} style={style} />
    </span>
  );
}
