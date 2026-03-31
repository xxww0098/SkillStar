import { motion, useReducedMotion } from "framer-motion";
import { cn } from "../../lib/utils";

interface LoadingLogoProps {
  size?: "sm" | "md" | "lg";
  label?: string;
  className?: string;
}

const sizeMap = {
  sm: "w-5 h-5",
  md: "w-8 h-8",
  lg: "w-12 h-12",
};

const containerSizeMap = {
  sm: "gap-2",
  md: "gap-3",
  lg: "gap-4",
};

const textSizeMap = {
  sm: "text-xs",
  md: "text-sm",
  lg: "text-sm",
};

export function LoadingLogo({ size = "md", label, className }: LoadingLogoProps) {
  const prefersReducedMotion = useReducedMotion();

  return (
    <div className={cn("flex flex-col items-center justify-center", containerSizeMap[size], className)}>
      <motion.div
        initial={{ opacity: 0, scale: 0.8 }}
        animate={{ opacity: 1, scale: 1 }}
        transition={{ duration: 0.3, ease: [0.22, 1, 0.36, 1] }}
        className={cn(
          "rounded-xl overflow-hidden bg-white shadow-sm shrink-0",
          sizeMap[size]
        )}
      >
        <img
          src="/skillstar-icon.svg"
          alt=""
          className={cn(
            "w-full h-full origin-center",
            !prefersReducedMotion && "animate-logo-spin"
          )}
        />
      </motion.div>
      {label && (
        <motion.span
          initial={{ opacity: 0 }}
          animate={{ opacity: 1 }}
          transition={{ delay: 0.15, duration: 0.2 }}
          className={cn("text-muted-foreground font-medium", textSizeMap[size])}
        >
          {label}
        </motion.span>
      )}
    </div>
  );
}
