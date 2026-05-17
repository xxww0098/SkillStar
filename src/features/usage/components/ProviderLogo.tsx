import { cn } from "@/lib/utils";

interface ProviderLogoProps {
  catalogId: string;
  displayName: string;
  brandColor: string;
  size?: "sm" | "md" | "lg";
  className?: string;
}

/**
 * Placeholder logo: rounded square with brand color background + first
 * meaningful character. Will be replaced by per-provider SVGs in Phase 7.
 */
export function ProviderLogo({ catalogId, displayName, brandColor, size = "md", className }: ProviderLogoProps) {
  const sizeClasses = {
    sm: "w-5 h-5 text-[10px]",
    md: "w-7 h-7 text-xs",
    lg: "w-10 h-10 text-sm",
  }[size];

  // Strip leading "智谱 " / "阿里 " etc. and take first non-space char.
  const initial = pickInitial(displayName, catalogId);
  const bg = brandColor.startsWith("#") ? brandColor : `#${brandColor}`;

  return (
    <div
      className={cn(
        "flex items-center justify-center rounded-md font-semibold text-white shrink-0",
        sizeClasses,
        className,
      )}
      style={{ background: bg }}
      aria-hidden="true"
    >
      {initial}
    </div>
  );
}

function pickInitial(name: string, fallback: string): string {
  const trimmed = name.trim();
  if (trimmed.length === 0) return fallback.charAt(0).toUpperCase();
  // Prefer first ASCII letter if present.
  for (const ch of trimmed) {
    if (/[A-Za-z0-9]/.test(ch)) return ch.toUpperCase();
  }
  // Otherwise first character (will be a CJK glyph).
  return Array.from(trimmed)[0] ?? "?";
}
