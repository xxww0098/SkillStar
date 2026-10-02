import { cn } from "@/lib/utils";

/**
 * The CSS-only stacked token bar the cross-view chips share (spec slice 13,
 * magpie's shape): input stacked at the bottom, output on top, no chart
 * library. Heights are percentages of `max` (the strip's busiest chip), so
 * one glance compares chips against each other. Cache tokens ride the
 * input side (inbound traffic); reasoning is already counted inside output.
 *
 * Static by design — nothing animates, so `prefers-reduced-motion` needs no
 * branch here.
 */
export function StackedTokenBar({
  input,
  output,
  max,
  label,
  className,
}: {
  /** Inbound tokens (input + cache read/write) — the bottom segment. */
  input: number;
  /** Outbound tokens (output, reasoning already inside) — the top segment. */
  output: number;
  /** The scale: the tallest bar of the surrounding strip renders at 100%. */
  max: number;
  /** Accessible description of the two segments (numbers are in the label). */
  label: string;
  className?: string;
}) {
  const scale = max > 0 ? max : 1;
  const inputPct = Math.round((Math.min(input, scale) / scale) * 100);
  const outputPct = Math.round((Math.min(output, scale) / scale) * 100);
  return (
    <span
      role="img"
      aria-label={label}
      title={label}
      className={cn("flex h-8 w-2 shrink-0 flex-col-reverse overflow-hidden rounded-sm bg-muted/60", className)}
    >
      {/* flex-col-reverse: input grows from the bottom, output stacks on top. */}
      <span className="w-full bg-primary/75" style={{ height: `${inputPct}%` }} />
      <span className="w-full bg-accent-foreground/45" style={{ height: `${outputPct}%` }} />
    </span>
  );
}
