import type { CSSProperties } from "react";
import { type BrandTheme, hexToRgbTriplet } from "../../../lib/brandThemes";

/**
 * Phase-1 brand CSS variables for usage card surfaces.
 * Matches today's SubscriptionCard injection:
 * `--brand-rgb` / `--brand-color` / `--brand-color-2`.
 *
 * Header gradients stay inline on the theme object (see design K5).
 */
export function brandThemeToCssVars(theme: BrandTheme): CSSProperties {
  return {
    "--brand-rgb": hexToRgbTriplet(theme.glow),
    "--brand-color": theme.bar[0],
    "--brand-color-2": theme.bar[1],
  } as CSSProperties;
}
