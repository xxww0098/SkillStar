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
      className={cn(
        "flex flex-col items-center justify-center text-center",
        sizeMap[size],
        className
      )}
    >
      {icon && (
        <div
          className={cn(
            "bg-muted/50 backdrop-blur-sm flex items-center justify-center mb-4",
            iconSizeMap[size]
          )}
        >
          {icon}
        </div>
      )}
      <h3 className="text-heading-sm mb-1">{title}</h3>
      {description && (
        <p className="text-caption max-w-sm mb-4">{description}</p>
      )}
      {action && <div className="mt-1">{action}</div>}
    </motion.div>
  );
}
