import { motion } from "framer-motion";
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

export function EmptyState({
  icon,
  title,
  description,
  action,
  size = "md",
  className,
}: EmptyStateProps) {
  return (
    <motion.div
      initial={{ opacity: 0 }}
      animate={{ opacity: 1 }}
      transition={{ duration: 0.25 }}
      className={cn(
        "flex flex-col items-center justify-center text-center",
        sizeMap[size],
        className
      )}
    >
      {icon && (
        <motion.div
          initial={{ opacity: 0, scale: 0.85 }}
          animate={{ opacity: 1, scale: 1 }}
          transition={{ duration: 0.35, ease: STAGGER_EASE }}
          className={cn(
            "bg-muted/50 backdrop-blur-sm flex items-center justify-center mb-4",
            iconSizeMap[size]
          )}
        >
          {icon}
        </motion.div>
      )}
      <motion.h3
        initial={{ opacity: 0, y: 8 }}
        animate={{ opacity: 1, y: 0 }}
        transition={{ delay: 0.08, duration: 0.3, ease: STAGGER_EASE }}
        className="text-heading-sm mb-1"
      >
        {title}
      </motion.h3>
      {description && (
        <motion.p
          initial={{ opacity: 0, y: 6 }}
          animate={{ opacity: 1, y: 0 }}
          transition={{ delay: 0.14, duration: 0.3, ease: STAGGER_EASE }}
          className="text-caption max-w-sm mb-4"
        >
          {description}
        </motion.p>
      )}
      {action && (
        <motion.div
          initial={{ opacity: 0, y: 4 }}
          animate={{ opacity: 1, y: 0 }}
          transition={{ delay: 0.2, duration: 0.3, ease: STAGGER_EASE }}
          className="mt-1"
        >
          {action}
        </motion.div>
      )}
    </motion.div>
  );
}

