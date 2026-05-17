import { cva, type VariantProps } from "class-variance-authority";
import { Slot } from "radix-ui";
import type * as React from "react";

import { cn } from "@/lib/utils";

const badgeVariants = cva(
  "inline-flex w-fit shrink-0 items-center justify-center gap-1 overflow-hidden rounded-xl border px-2.5 py-0.5 text-xs font-medium whitespace-nowrap transition-[color,box-shadow,background-color] duration-200 focus-visible:border-ring focus-visible:ring-[3px] focus-visible:ring-ring/50 aria-invalid:border-destructive aria-invalid:ring-destructive/20 dark:aria-invalid:ring-destructive/40 [&>svg]:pointer-events-none [&>svg]:size-3",
  {
    variants: {
      variant: {
        default: "bg-primary/15 text-primary border-primary/20 [a&]:hover:bg-primary/25",
        secondary: "bg-secondary text-secondary-foreground [a&]:hover:bg-secondary/90",
        destructive:
          "bg-destructive text-white focus-visible:ring-destructive/20 dark:bg-destructive/60 dark:focus-visible:ring-destructive/40 [a&]:hover:bg-destructive/90",
        outline: "border-border text-muted-foreground bg-muted [a&]:hover:bg-accent [a&]:hover:text-accent-foreground",
        ghost: "[a&]:hover:bg-accent [a&]:hover:text-accent-foreground",
        link: "text-primary underline-offset-4 [a&]:hover:underline",
        hot: "bg-red-500/15 text-red-500 dark:text-red-400 border-red-500/20",
        popular: "bg-amber-500/15 text-amber-600 dark:text-amber-400 border-amber-500/20",
        rising: "bg-emerald-500/15 text-emerald-600 dark:text-emerald-400 border-emerald-500/20",
        new: "bg-violet-500/15 text-violet-600 dark:text-violet-400 border-violet-500/20",
        success: "bg-emerald-500/15 text-emerald-600 dark:text-emerald-400 border-emerald-500/20",
        warning: "bg-amber-500/15 text-amber-600 dark:text-amber-400 border-amber-500/20",
      },
    },
    defaultVariants: {
      variant: "default",
    },
  },
);

function Badge({
  className,
  variant = "default",
  asChild = false,
  ...props
}: React.ComponentProps<"span"> & VariantProps<typeof badgeVariants> & { asChild?: boolean }) {
  const Comp = asChild ? Slot.Root : "span";

  return (
    <Comp data-slot="badge" data-variant={variant} className={cn(badgeVariants({ variant }), className)} {...props} />
  );
}

export { Badge, badgeVariants };
