import * as React from "react";
import { cva, type VariantProps } from "class-variance-authority";
import { cn } from "../../lib/utils";

const badgeVariants = cva(
  "inline-flex items-center rounded-xl px-2.5 py-0.5 text-xs font-medium transition duration-200 border",
  {
    variants: {
      variant: {
        default: "bg-primary/15 text-primary border-primary/20",
        hot: "bg-red-500/15 text-red-400 border-red-500/20",
        popular: "bg-amber-500/15 text-amber-400 border-amber-500/20",
        rising: "bg-emerald-500/15 text-emerald-400 border-emerald-500/20",
        new: "bg-violet-500/15 text-violet-400 border-violet-500/20",
        outline: "border-border text-muted-foreground bg-muted",
        success: "bg-emerald-500/15 text-emerald-400 border-emerald-500/20",
        warning: "bg-amber-500/15 text-amber-400 border-amber-500/20",
      },
    },
    defaultVariants: {
      variant: "default",
    },
  }
);

export interface BadgeProps
  extends React.HTMLAttributes<HTMLDivElement>,
    VariantProps<typeof badgeVariants> {}

function Badge({ className, variant, ...props }: BadgeProps) {
  return (
    <div className={cn(badgeVariants({ variant }), className)} {...props} />
  );
}

export { Badge, badgeVariants };
