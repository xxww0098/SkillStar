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

// Shared expanded-track styling — defined, crisp segmented control.
const expandedTrack = cn(
  "relative grid w-full grid-cols-3 gap-0 rounded-xl p-1",
  "bg-muted/60 ring-1 ring-inset ring-border/40 shadow-inner",
);

export function ModeSwitcher({ currentMode, onModeChange, collapsed }: ModeSwitcherProps) {
  const prefersReducedMotion = useReducedMotion();

  if (collapsed) {
    // Collapsed rail: stacked icon-only buttons.
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
                "group relative flex h-9 w-full cursor-pointer items-center justify-center rounded-lg outline-none transition-all duration-150",
                "focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2 focus-visible:ring-offset-background",
                isActive
                  ? "bg-primary/20 text-primary shadow-xs ring-1 ring-inset ring-primary/40"
                  : "text-muted-foreground hover:bg-muted/50 hover:text-foreground",
              )}
            >
              <Icon
                className="h-4 w-4 transition-transform duration-200 group-hover:scale-110 motion-reduce:transform-none"
                strokeWidth={isActive ? 2.5 : 2}
                aria-hidden
              />
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
              "group relative z-0 flex h-8 min-w-0 cursor-pointer items-center justify-center rounded-lg outline-none transition-colors duration-150",
              "focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-1 focus-visible:ring-offset-background",
              isActive
                ? "text-foreground font-semibold"
                : "text-muted-foreground hover:bg-white/[0.06] hover:text-foreground paper:hover:bg-black/[0.035]",
            )}
          >
            {isActive && (
              <motion.div
                layoutId="mode-switcher-active"
                aria-hidden="true"
                className={cn(
                  "pointer-events-none absolute inset-0 z-0 rounded-lg",
                  "bg-background shadow-[0_2px_6px_rgba(15,23,42,0.12),0_1px_2px_rgba(15,23,42,0.08)] ring-1 ring-border/60",
                  "paper:bg-card",
                )}
                transition={
                  prefersReducedMotion ? { duration: 0 } : { type: "spring", stiffness: 520, damping: 38, mass: 0.7 }
                }
              />
            )}
            <Icon
              className={cn(
                "relative z-10 h-4 w-4 shrink-0 transition-[color,transform] duration-200 group-hover:scale-110 motion-reduce:transform-none",
                isActive ? "text-primary drop-shadow-2xs" : "text-current",
              )}
              strokeWidth={isActive ? 2.5 : 2}
              aria-hidden
            />
          </button>
        );
      })}
    </div>
  );
}
