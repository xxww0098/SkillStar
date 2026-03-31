import { motion, useReducedMotion } from "framer-motion";
import { cn } from "../../lib/utils";

interface SuccessCheckmarkProps {
  size?: number;
  className?: string;
}

/**
 * Animated SVG checkmark with stroke-draw effect.
 * Used for install/update/save success celebrations.
 */
export function SuccessCheckmark({ size = 20, className }: SuccessCheckmarkProps) {
  const prefersReducedMotion = useReducedMotion();

  if (prefersReducedMotion) {
    return (
      <svg width={size} height={size} viewBox="0 0 20 20" fill="none" className={className}>
        <circle cx="10" cy="10" r="9" stroke="currentColor" strokeWidth="1.5" opacity="0.2" />
        <path d="M6 10.5l2.5 2.5L14 7.5" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round" />
      </svg>
    );
  }

  return (
    <motion.div
      initial={{ scale: 0.6, opacity: 0 }}
      animate={{ scale: 1, opacity: 1 }}
      transition={{ duration: 0.35, ease: [0.22, 1, 0.36, 1] }}
      className={cn("animate-success-glow rounded-full", className)}
    >
      <svg width={size} height={size} viewBox="0 0 20 20" fill="none">
        {/* Background ring */}
        <motion.circle
          cx="10"
          cy="10"
          r="9"
          stroke="currentColor"
          strokeWidth="1.5"
          initial={{ pathLength: 0, opacity: 0 }}
          animate={{ pathLength: 1, opacity: 0.2 }}
          transition={{ duration: 0.4, ease: [0.22, 1, 0.36, 1] }}
        />
        {/* Checkmark path */}
        <path
          d="M6 10.5l2.5 2.5L14 7.5"
          stroke="currentColor"
          strokeWidth="1.5"
          strokeLinecap="round"
          strokeLinejoin="round"
          className="animate-checkmark-draw"
        />
      </svg>
    </motion.div>
  );
}
