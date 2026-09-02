import { cva, type VariantProps } from "class-variance-authority";
import type * as React from "react";
import { cn } from "@/lib/utils";

/**
 * Compact status mark: installed / healthy / deprecated / missing runtime.
 *
 * Distinct from `Badge`, which is a larger, rounded-xl catalog label that
 * sometimes behaves like a link. These chips sit inside cards and nested
 * panels and must not steal the row's hover or grow on focus.
 */
const statusChipVariants = cva(
  "inline-flex items-center gap-1 rounded px-1.5 text-micro font-medium ring-1 ring-inset",
  {
    variants: {
      tone: {
        success: "bg-emerald-500/12 text-emerald-600 ring-emerald-500/25 paper:text-emerald-700",
        warning: "bg-amber-500/12 text-amber-600 ring-amber-500/25 paper:text-amber-700",
        danger: "bg-destructive/12 text-destructive ring-destructive/25",
        info: "bg-sky-500/12 text-sky-600 ring-sky-500/25 paper:text-sky-700",
        muted: "bg-muted text-muted-foreground ring-border/60",
      },
      size: {
        sm: "h-4",
        md: "h-5",
      },
    },
    defaultVariants: {
      tone: "muted",
      size: "md",
    },
  },
);

export type StatusChipTone = NonNullable<VariantProps<typeof statusChipVariants>["tone"]>;

export function StatusChip({
  className,
  tone,
  size,
  ...props
}: React.ComponentProps<"span"> & VariantProps<typeof statusChipVariants>) {
  return <span className={cn(statusChipVariants({ tone, size }), className)} {...props} />;
}

export { statusChipVariants };
