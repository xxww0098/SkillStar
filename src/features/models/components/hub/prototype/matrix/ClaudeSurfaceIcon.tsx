import { Monitor, Terminal } from "lucide-react";
import { ClaudeColor } from "../../../../../../components/ui/icons/lobe";
import { LobeIcon } from "../../../../../../components/ui/icons/LobeIcon";
import { cn } from "../../../../../../lib/utils";

/**
 * PROTOTYPE — Claude Code CLI / Desktop glyph, ported from cc-switch
 * `AppSwitcher` (`/Users/xxww/Code/temp/cc-switch/src/components/AppSwitcher.tsx`):
 *
 * - Both surfaces share the same Claude brand mark.
 * - Distinction is a corner badge: Terminal (`>_`) for CLI, Monitor for Desktop.
 * - Desktop badge uses a 0.5px Y offset so the monitor glyph sits optically centered.
 */
export type ClaudeSurface = "cli" | "desktop";

type ClaudeSurfaceIconProps = {
  surface: ClaudeSurface;
  /** Brand mark size in px (cc-switch AppSwitcher uses 20). */
  size?: number;
  className?: string;
  /** Mute the brand when the carousel tile is unselected. */
  muted?: boolean;
};

const BADGE = {
  cli: { Icon: Terminal, offsetY: undefined as number | undefined, label: "CLI" },
  desktop: { Icon: Monitor, offsetY: 0.5, label: "Desktop" },
} as const;

export function ClaudeSurfaceIcon({ surface, size = 20, className, muted }: ClaudeSurfaceIconProps) {
  const { Icon, offsetY } = BADGE[surface];
  // Extra pad so the corner badge isn't clipped by parent overflow / rounding.
  const box = size + 6;
  return (
    <span
      className={cn("relative inline-flex shrink-0 items-center justify-center overflow-visible", className)}
      style={{ width: box, height: box }}
      title={claudeSurfaceLabel(surface)}
    >
      <LobeIcon
        icon={ClaudeColor}
        size={size}
        className={cn("transition-[filter,opacity]", muted && "grayscale opacity-70")}
      />
      <span
        className={cn(
          "absolute right-0 bottom-0 z-[1] flex h-3 w-3 items-center justify-center rounded-[3px] border shadow-sm",
          muted
            ? "border-border/60 bg-muted text-muted-foreground"
            : "border-border bg-background text-foreground",
        )}
        aria-hidden
      >
        <Icon
          className="h-2 w-2"
          strokeWidth={2.5}
          style={offsetY != null ? { transform: `translateY(${offsetY}px)` } : undefined}
        />
      </span>
    </span>
  );
}

export function claudeSurfaceLabel(surface: ClaudeSurface): string {
  return surface === "cli" ? "Claude Code CLI" : "Claude Code Desktop";
}
