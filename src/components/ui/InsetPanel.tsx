import type * as React from "react";
import { cn } from "@/lib/utils";

/**
 * Nested surface inside a drawer, page or card.
 *
 * Distinct from `Card`: cards lift on hover, use the 24px product radius and
 * are a list identity. This is a quiet inset used for probe results, filter
 * groups, confirm blocks and source lists — the same nested-panel intent
 * copied across MCP (and similar settings blocks) until it drifted.
 */
export function InsetPanel({ className, ...props }: React.ComponentProps<"div">) {
  return (
    <div
      className={cn("space-y-2.5 rounded-xl border border-border/60 bg-background/40 p-3.5", className)}
      {...props}
    />
  );
}
