import type { CSSProperties, ReactNode } from "react";
import { useTranslation } from "react-i18next";
import { cn } from "@/lib/utils";
import { brandChipStyle, brandHeroSurface, hexToRgbTriplet } from "../lib/brandThemes";
import { authModeLabel } from "../lib/usageLabels";
import type { AuthMode, CatalogEntry } from "../types";
import { ProviderLogo } from "./ProviderLogo";

export function catalogBrandVars(brandColor: string): CSSProperties {
  const brandRgb = hexToRgbTriplet(brandColor);
  return {
    "--brand-rgb": brandRgb,
    "--brand-color": `#${brandColor.replace("#", "")}`,
  } as CSSProperties;
}

interface ProviderCatalogHeroProps {
  entry: CatalogEntry;
  /** Override title (e.g. custom subscription display name when editing). */
  displayTitle?: string;
  /** Highlight a specific auth mode chip; defaults to the provider's first supported mode. */
  authMode?: AuthMode;
  variant?: "panel" | "inline";
  trailing?: ReactNode;
  className?: string;
}

/** Branded provider identity — shared by placeholder cards and the subscription dialog. */
export function ProviderCatalogHero({
  entry,
  displayTitle,
  authMode,
  variant = "panel",
  trailing,
  className,
}: ProviderCatalogHeroProps) {
  const { t } = useTranslation();
  const title = displayTitle?.trim() || entry.display_name;
  const chipMode = authMode ?? entry.auth_modes[0] ?? "o-auth";
  const chipStyle = brandChipStyle(entry.brand_color);
  const surface = brandHeroSurface(entry.brand_color);

  const identity = (
    <div className="flex items-start gap-2.5">
      <ProviderLogo
        catalogId={entry.id}
        displayName={entry.display_name}
        brandColor={entry.brand_color}
        size={variant === "panel" ? "md" : "md"}
        className="shrink-0"
      />
      <div className="min-w-0 flex-1">
        <h3
          className={cn(
            "font-bold leading-snug text-foreground line-clamp-2",
            variant === "panel" ? "text-base" : "text-sm",
          )}
          title={title}
        >
          {title}
        </h3>
      </div>
    </div>
  );

  const metaRow = (
    <div className="flex flex-wrap items-center justify-between gap-x-2 gap-y-1.5">
      <span
        className="shrink-0 rounded border px-1.5 py-0.5 text-[10px] font-semibold uppercase tracking-wider"
        style={chipStyle}
      >
        {authModeLabel(chipMode, t)}
      </span>
      {trailing}
    </div>
  );

  if (variant === "inline") {
    return (
      <div className={cn("relative z-10 space-y-2", className)} style={catalogBrandVars(entry.brand_color)}>
        {identity}
        {metaRow}
      </div>
    );
  }

  return (
    <div
      className={cn("relative overflow-hidden rounded-2xl border p-4", className)}
      style={{
        ...catalogBrandVars(entry.brand_color),
        borderColor: surface.borderColor,
        background: surface.background,
      }}
    >
      <div
        className="pointer-events-none absolute -right-10 -top-10 h-32 w-32 rounded-full opacity-25 blur-[36px]"
        style={{ backgroundColor: surface.glow }}
      />
      <div className="relative z-10 space-y-2.5">
        {identity}
        {metaRow}
      </div>
    </div>
  );
}
