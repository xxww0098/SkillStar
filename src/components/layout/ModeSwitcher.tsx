import { motion, useReducedMotion } from "framer-motion";
import { Cpu, Gauge, LayoutGrid } from "lucide-react";
import { cn } from "../../lib/utils";
import type { AppMode } from "../../types";

interface ModeSwitcherProps {
  currentMode: AppMode;
  onModeChange: (mode: AppMode) => void;
  collapsed: boolean;
}

const modes: { id: AppMode; label: string; icon: React.ElementType }[] = [
  { id: "skills", label: "Skills", icon: LayoutGrid },
  { id: "usage", label: "Usage", icon: Gauge },
  { id: "models", label: "Models", icon: Cpu },
];

// Shared expanded-track styling — soft, flat, low-contrast so it recedes
// behind the active pill instead of fighting it.
const expandedTrack = cn(
  "relative grid w-full grid-cols-3 gap-0 rounded-xl p-1",
  "bg-muted/40 ring-1 ring-inset ring-border/25",
  "dark:bg-muted/20 dark:ring-border/15",
);

export function ModeSwitcher({ currentMode, onModeChange, collapsed }: ModeSwitcherProps) {
  const prefersReducedMotion = useReducedMotion();

  if (collapsed) {
    // Collapsed rail: stacked icon-only buttons. No shared track so the
    // sidebar reads as a clean column of glyphs.
    return (
      <div className="mx-1 flex flex-col gap-1">
        {modes.map((mode) => {
          const Icon = mode.icon;
          const isActive = currentMode === mode.id;

          return (
            <button
              key={mode.id}
              type="button"
              onClick={() => onModeChange(mode.id)}
              aria-pressed={isActive}
              aria-label={mode.label}
              title={mode.label}
              className={cn(
                "relative flex h-9 w-full cursor-pointer items-center justify-center rounded-lg outline-none transition-colors duration-150",
                "focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2 focus-visible:ring-offset-background",
                isActive
                  ? "bg-primary/12 text-primary ring-1 ring-inset ring-primary/25 dark:bg-primary/18"
                  : "text-muted-foreground hover:bg-muted/40 hover:text-foreground",
              )}
            >
              <Icon className="h-4 w-4" strokeWidth={isActive ? 2.25 : 2} />
            </button>
          );
        })}
      </div>
    );
  }

  return (
    <div className={expandedTrack}>
      {modes.map((mode) => {
        const Icon = mode.icon;
        const isActive = currentMode === mode.id;

        return (
          <button
            key={mode.id}
            type="button"
            onClick={() => onModeChange(mode.id)}
            aria-pressed={isActive}
            aria-label={mode.label}
            title={mode.label}
            className={cn(
              "relative z-0 flex h-8 min-w-0 cursor-pointer items-center justify-center rounded-lg outline-none transition-colors duration-150",
              "focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-1 focus-visible:ring-offset-background",
              isActive ? "text-foreground" : "text-muted-foreground hover:text-foreground",
            )}
          >
            {isActive && (
              <motion.div
                layoutId="mode-switcher-active"
                aria-hidden="true"
                className={cn(
                  "pointer-events-none absolute inset-0 z-0 rounded-lg",
                  "bg-background shadow-[0_1px_2px_rgba(15,23,42,0.08),0_1px_1px_rgba(15,23,42,0.04)] ring-1 ring-border/45",
                  "dark:bg-card/80 dark:shadow-[0_1px_2px_rgba(0,0,0,0.45)] dark:ring-border/35",
                )}
                transition={
                  prefersReducedMotion ? { duration: 0 } : { type: "spring", stiffness: 520, damping: 38, mass: 0.7 }
                }
              />
            )}
            <Icon
              className={cn(
                "relative z-10 h-4 w-4 shrink-0 transition-colors",
                isActive ? "text-primary" : "text-current",
              )}
              strokeWidth={isActive ? 2.25 : 2}
            />
          </button>
        );
      })}
    </div>
  );
}
