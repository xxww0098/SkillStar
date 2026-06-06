import type { CSSProperties, ReactNode } from "react";
import { useTranslation } from "react-i18next";
import { cn } from "@/lib/utils";
import { authModeLabel } from "../lib/usageLabels";
import type { AuthMode, CatalogEntry } from "../types";
import { ProviderLogo } from "./ProviderLogo";

export function catalogBrandVars(brandColor: string): CSSProperties {
  const brandRgb = hexToRgb(brandColor);
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
  const chipMode = authMode ?? entry.auth_modes[0] ?? "manual";
  const brandRgb = hexToRgb(entry.brand_color);

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
        {entry.description && (
          <p
            className="mt-0.5 text-[10px] leading-snug text-muted-foreground line-clamp-2 break-words"
            title={entry.description}
          >
            {entry.description}
          </p>
        )}
      </div>
    </div>
  );

  const metaRow = (
    <div className="flex flex-wrap items-center justify-between gap-x-2 gap-y-1.5">
      <span
        className="shrink-0 rounded border px-1.5 py-0.5 text-[9px] font-semibold uppercase tracking-wider"
        style={{
          color: `rgb(${brandRgb})`,
          backgroundColor: `rgba(${brandRgb}, 0.1)`,
          borderColor: `rgba(${brandRgb}, 0.22)`,
        }}
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
        borderColor: `rgba(${brandRgb}, 0.28)`,
        background: `linear-gradient(135deg, rgba(${brandRgb}, 0.12) 0%, rgba(${brandRgb}, 0.03) 48%, transparent 100%)`,
      }}
    >
      <div
        className="pointer-events-none absolute -right-10 -top-10 h-32 w-32 rounded-full opacity-25 blur-[36px]"
        style={{ backgroundColor: `rgb(${brandRgb})` }}
      />
      <div className="relative z-10 space-y-2.5">
        {identity}
        {metaRow}
      </div>
    </div>
  );
}

function hexToRgb(hex: string): string {
  const h = hex.replace("#", "");
  const r = parseInt(h.substring(0, 2), 16);
  const g = parseInt(h.substring(2, 4), 16);
  const b = parseInt(h.substring(4, 6), 16);
  return Number.isNaN(r) || Number.isNaN(g) || Number.isNaN(b) ? "107, 114, 128" : `${r}, ${g}, ${b}`;
}
