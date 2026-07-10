import type { ReactNode } from "react";
import { cn } from "@/lib/utils";
import type { BrandTheme } from "../../lib/brandThemes";
import { brandThemeToCssVars } from "./primitives";

export interface LightBodySurfaceProps {
  theme: BrandTheme;
  children: ReactNode;
  className?: string;
}

/**
 * Light island for float-window body: keeps zinc/light vendor panels readable
 * inside dark chrome. **Must** inject brand CSS vars (K11b).
 */
export function LightBodySurface({ theme, children, className }: LightBodySurfaceProps) {
  return (
    <div className={cn("rounded-2xl bg-white/95 p-3 text-zinc-900", className)} style={brandThemeToCssVars(theme)}>
      {children}
    </div>
  );
}
