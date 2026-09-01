import type * as React from "react";
import { cn } from "@/lib/utils";

/**
 * Multi-line twin of `Input`. Form fields (KEY=VALUE, notes, share codes)
 * were each inventing their own border/focus recipe; the full-pane skill
 * editor is a different intent and stays a raw textarea.
 */
function Textarea({ className, ...props }: React.ComponentProps<"textarea">) {
  return (
    <textarea
      data-slot="textarea"
      className={cn(
        "flex min-h-16 w-full rounded-[9px] border border-input-border bg-input px-3 py-2 text-sm text-foreground shadow-sm backdrop-blur-sm placeholder:text-foreground/45",
        "outline-none transition duration-200 focus-visible:border-primary/60 focus-visible:ring-2 focus-visible:ring-primary/40",
        "disabled:cursor-not-allowed disabled:bg-muted/50 disabled:text-foreground/55 disabled:placeholder:text-foreground/40",
        "aria-invalid:border-destructive aria-invalid:ring-destructive/20",
        className,
      )}
      {...props}
    />
  );
}

export { Textarea };
