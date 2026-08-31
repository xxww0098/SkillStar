import type { ReactNode } from "react";
import { cn } from "@/lib/utils";

export interface SecondaryPanelProps {
  children: ReactNode;
  /** Kept for call-site compatibility; grouping is now spacing-only. */
  accent: string;
  className?: string;
}

/** Secondary rows under a quota meter. No nested box. */
export function SecondaryPanel({ children, className }: SecondaryPanelProps) {
  return <div className={cn("space-y-2", className)}>{children}</div>;
}
