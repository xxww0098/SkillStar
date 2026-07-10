import type { ReactNode } from "react";
import { cn } from "@/lib/utils";

export interface SecondaryPanelProps {
  children: ReactNode;
  /** Brand accent hex for 06/14 alpha surfaces. */
  accent: string;
  className?: string;
}

/**
 * Secondary info shell only (Cursor/Grok/Glm side panels, credits frame).
 * Fixed alpha: bg `06`, border `14`. Do **not** use for primary KPI heroes
 * (DeepSeek balance / account status) — design K18.
 */
export function SecondaryPanel({ children, accent, className }: SecondaryPanelProps) {
  const color = accent.startsWith("#") ? accent : `#${accent}`;
  return (
    <div
      className={cn("space-y-2 rounded-2xl border p-3", className)}
      style={{ backgroundColor: `${color}06`, borderColor: `${color}14` }}
    >
      {children}
    </div>
  );
}
