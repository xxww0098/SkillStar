import { cn } from "@/lib/utils";

interface PlanBadgeProps {
  /** Raw plan tier text from fetcher / manual entry. Empty/null → not rendered. */
  plan: string | null | undefined;
  /** `onBrand` renders a frosted white chip for use over a colored brand band. */
  variant?: "default" | "onBrand";
  className?: string;
}

interface ToneSpec {
  bg: string;
  text: string;
  ring: string;
}

const TONE_DEFAULT: ToneSpec = {
  bg: "bg-muted",
  text: "text-muted-foreground",
  ring: "ring-border",
};

const TONE_MAP: Record<string, ToneSpec> = {
  PRO: { bg: "bg-blue-500/15", text: "text-blue-400", ring: "ring-blue-500/30" },
  PLUS: { bg: "bg-green-500/15", text: "text-green-400", ring: "ring-green-500/30" },
  MAX: { bg: "bg-purple-500/15", text: "text-purple-400", ring: "ring-purple-500/30" },
  ULTRA: { bg: "bg-purple-500/15", text: "text-purple-400", ring: "ring-purple-500/30" },
  TEAM: { bg: "bg-amber-500/15", text: "text-amber-400", ring: "ring-amber-500/30" },
  ENTERPRISE: { bg: "bg-red-500/15", text: "text-red-400", ring: "ring-red-500/30" },
  BUSINESS: { bg: "bg-amber-500/15", text: "text-amber-400", ring: "ring-amber-500/30" },
  FREE: { bg: "bg-muted/60", text: "text-muted-foreground", ring: "ring-border" },
  PAYG: { bg: "bg-muted/60", text: "text-muted-foreground", ring: "ring-border" },
};

const TONE_ON_BRAND: ToneSpec = {
  bg: "bg-white/20 backdrop-blur-sm",
  text: "text-white",
  ring: "ring-white/40",
};

export function PlanBadge({ plan, variant = "default", className }: PlanBadgeProps) {
  if (!plan) return null;
  const normalized = plan.trim();
  if (normalized.length === 0) return null;
  const upper = normalized.toUpperCase();
  const tone = variant === "onBrand" ? TONE_ON_BRAND : (TONE_MAP[upper] ?? TONE_DEFAULT);
  const display = upper.length > 6 ? `${upper.slice(0, 6)}…` : upper;
  return (
    <span
      className={cn(
        "inline-flex items-center rounded-full px-2 py-0.5 text-[10px] font-semibold uppercase tracking-wider ring-1",
        tone.bg,
        tone.text,
        tone.ring,
        className,
      )}
      title={normalized}
    >
      {display}
    </span>
  );
}
