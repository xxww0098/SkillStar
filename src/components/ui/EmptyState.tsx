import { motion, useReducedMotion } from "framer-motion";
import { cn } from "../../lib/utils";

interface EmptyStateProps {
  icon?: React.ReactNode;
  title: string;
  description?: string;
  action?: React.ReactNode;
  size?: "sm" | "md" | "lg";
  className?: string;
}

const sizeMap = {
  sm: "py-8",
  md: "py-16",
  lg: "py-20",
};

const iconSizeMap = {
  sm: "w-10 h-10 rounded-xl text-base",
  md: "w-14 h-14 rounded-2xl",
  lg: "w-16 h-16 rounded-2xl text-2xl",
};

const STAGGER_EASE = [0.22, 1, 0.36, 1] as const;

export function EmptyState({ icon, title, description, action, size = "md", className }: EmptyStateProps) {
  const reduce = useReducedMotion();
  // When the user prefers reduced motion, skip the transform/stagger and just
  // fade in — keeps the entrance calm and cheap.
  const rise = (y: number, delay: number) =>
    reduce
      ? { initial: { opacity: 0 }, animate: { opacity: 1 }, transition: { duration: 0.2, delay } }
      : {
          initial: { opacity: 0, y },
          animate: { opacity: 1, y: 0 },
          transition: { delay, duration: 0.3, ease: STAGGER_EASE },
        };

  return (
    <motion.div
      initial={{ opacity: 0 }}
      animate={{ opacity: 1 }}
      transition={{ duration: 0.25 }}
      className={cn("flex flex-col items-center justify-center text-center", sizeMap[size], className)}
    >
      {icon && (
        <motion.div
          initial={{ opacity: 0, scale: reduce ? 1 : 0.85 }}
          animate={{ opacity: 1, scale: 1 }}
          transition={{ duration: 0.35, ease: STAGGER_EASE }}
          className={cn(
            // Give the icon real visual weight so it reads as part of the card
            // language rather than a flat tinted square.
            "flex items-center justify-center mb-4 text-muted-foreground",
            "border border-border/60 bg-gradient-to-br from-muted/70 to-muted/25",
            "shadow-[0_4px_16px_-8px_var(--color-shadow)] ring-1 ring-inset ring-white/[0.04]",
            iconSizeMap[size],
          )}
        >
          {icon}
        </motion.div>
      )}
      <motion.h3 {...rise(8, 0.08)} className="text-heading-sm mb-1">
        {title}
      </motion.h3>
      {description && (
        <motion.p {...rise(6, 0.14)} className="text-caption max-w-sm mb-4">
          {description}
        </motion.p>
      )}
      {action && (
        <motion.div {...rise(4, 0.2)} className="mt-1">
          {action}
        </motion.div>
      )}
    </motion.div>
  );
}
